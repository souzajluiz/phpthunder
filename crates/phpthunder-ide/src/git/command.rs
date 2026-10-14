//! Running `git` in the project. Every call is asynchronous (run from a
//! `Task`), never prompts (credentials must come from a helper or SSH
//! agent), and reports git's own error text on failure.

use super::*;
use crate::tr;
use std::path::{Path, PathBuf};

/// Runs `git args…` in `root` and returns its standard output.
///
/// # Errors
/// Returns git's error output (or why it could not start) when it fails.
async fn run(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .await
        .map_err(|e| tr!("error-git-start", error = e.to_string()))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        };
        Err(message.trim().to_string())
    }
}

/// Paths of `files` relative to `root`, for git arguments.
fn relative(root: &Path, files: &[PathBuf]) -> Vec<String> {
    files
        .iter()
        .map(|p| {
            p.strip_prefix(root)
                .unwrap_or(p)
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

/// Who last changed each line of `path` (as saved on disk).
///
/// # Errors
/// Returns git's error text (for example, a file Git does not track).
pub async fn blame(root: PathBuf, path: PathBuf) -> Result<Vec<LineBlame>, String> {
    let rel = relative(&root, std::slice::from_ref(&path));
    let mut args = vec!["blame", "--porcelain", "--"];
    args.extend(rel.iter().map(String::as_str));
    run(&root, &args).await.map(|out| parse_blame(&out))
}

/// Runs `git <command> -- <files>`.
///
/// # Errors
/// Returns git's error text.
async fn run_on_files(root: &Path, command: &[&str], files: &[PathBuf]) -> Result<(), String> {
    let rel = relative(root, files);
    let mut args: Vec<&str> = command.to_vec();
    args.push("--");
    args.extend(rel.iter().map(String::as_str));
    run(root, &args).await.map(|_| ())
}

/// Whether `root` is the top of a git work tree.
pub fn is_repository(root: &Path) -> bool {
    root.join(".git").exists()
}

/// Working tree status.
///
/// # Errors
/// Returns git's error text.
pub async fn status(root: PathBuf) -> Result<RepoStatus, String> {
    let out = run(
        &root,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
        ],
    )
    .await?;
    Ok(parse_status(&out, &root))
}

/// Content of `path` at `HEAD`; `None` when it is not in `HEAD` (new or
/// untracked file, or no commit yet).
pub async fn head_content(root: PathBuf, path: PathBuf) -> Option<String> {
    let rel = relative(&root, std::slice::from_ref(&path)).pop()?;
    run(&root, &["show", &format!("HEAD:{rel}")]).await.ok()
}

/// Adds `files` to the index.
///
/// # Errors
/// Returns git's error text.
pub async fn stage(root: PathBuf, files: Vec<PathBuf>) -> Result<(), String> {
    run_on_files(&root, &["add", "--all"], &files).await
}

/// Removes `files` from the index, keeping the working tree.
///
/// # Errors
/// Returns git's error text.
pub async fn unstage(root: PathBuf, files: Vec<PathBuf>) -> Result<(), String> {
    run_on_files(&root, &["reset", "--quiet", "HEAD"], &files).await
}

/// Throws away the changes of tracked `files`, in the index and the
/// working tree, restoring them from `HEAD`.
///
/// # Errors
/// Returns git's error text.
pub async fn discard(root: PathBuf, files: Vec<PathBuf>) -> Result<(), String> {
    run_on_files(
        &root,
        &["restore", "--source=HEAD", "--staged", "--worktree"],
        &files,
    )
    .await
}

/// Commits the index with `message`; returns git's summary line.
///
/// # Errors
/// Returns git's error text (nothing staged, hooks failing, …).
pub async fn commit(root: PathBuf, message: String) -> Result<String, String> {
    let out = run(&root, &["commit", "--message", &message]).await?;
    Ok(out.lines().next().unwrap_or_default().to_string())
}

/// Pushes the current branch, setting its upstream on the first push.
///
/// # Errors
/// Returns git's error text (rejected, no remote, authentication).
pub async fn push(root: PathBuf) -> Result<(), String> {
    run(&root, &["push", "--set-upstream", "origin", "HEAD"])
        .await
        .map(|_| ())
}

/// Pulls the current branch, fast-forward only so it never starts a merge.
///
/// # Errors
/// Returns git's error text (diverged, conflicts, authentication).
pub async fn pull(root: PathBuf) -> Result<(), String> {
    run(&root, &["pull", "--ff-only"]).await.map(|_| ())
}

/// Local and remote branches.
///
/// # Errors
/// Returns git's error text.
pub async fn branches(root: PathBuf) -> Result<Vec<Branch>, String> {
    let out = run(&root, &["branch", "--all", "--format=%(HEAD)\t%(refname)"]).await?;
    Ok(parse_branches(&out))
}

/// Checks out `branch`; a remote branch (`origin/x`) becomes a local
/// tracking branch `x`.
///
/// # Errors
/// Returns git's error text (local changes would be overwritten, …).
pub async fn checkout(root: PathBuf, branch: Branch) -> Result<(), String> {
    let args: Vec<&str> = if branch.remote {
        vec!["switch", "--track", &branch.name]
    } else {
        vec!["switch", &branch.name]
    };
    run(&root, &args).await.map(|_| ())
}

/// Creates branch `name` from `HEAD` and checks it out.
///
/// # Errors
/// Returns git's error text (invalid or existing name).
pub async fn create_branch(root: PathBuf, name: String) -> Result<(), String> {
    run(&root, &["switch", "--create", &name]).await.map(|_| ())
}

/// One stash entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stash {
    /// Position in `git stash list` (0 is the newest).
    pub index: usize,
    /// Its message (`WIP on main: …` or the given one).
    pub message: String,
}

/// Stashes the working tree and index, untracked files included.
///
/// # Errors
/// Returns git's error text (nothing to stash, …).
pub async fn stash(root: PathBuf, message: String) -> Result<(), String> {
    let mut args = vec!["stash", "push", "--include-untracked"];
    if !message.trim().is_empty() {
        args.extend(["--message", message.trim()]);
    }
    run(&root, &args).await.map(|_| ())
}

/// The stashes, newest first.
///
/// # Errors
/// Returns git's error text.
pub async fn stashes(root: PathBuf) -> Result<Vec<Stash>, String> {
    let out = run(&root, &["stash", "list", "--format=%gs"]).await?;
    Ok(out
        .lines()
        .enumerate()
        .map(|(index, message)| Stash {
            index,
            message: message.to_string(),
        })
        .collect())
}

/// Applies stash `index` and drops it (`pop`), or only applies it.
///
/// # Errors
/// Returns git's error text (conflicts, …).
pub async fn unstash(root: PathBuf, index: usize, keep: bool) -> Result<(), String> {
    let reference = format!("stash@{{{index}}}");
    let verb = if keep { "apply" } else { "pop" };
    run(&root, &["stash", verb, &reference]).await.map(|_| ())
}

/// Deletes stash `index`.
///
/// # Errors
/// Returns git's error text.
pub async fn drop_stash(root: PathBuf, index: usize) -> Result<(), String> {
    let reference = format!("stash@{{{index}}}");
    run(&root, &["stash", "drop", &reference]).await.map(|_| ())
}

/// Rebases the current branch onto `onto`.
///
/// # Errors
/// Returns git's error text; a conflict stops the rebase in progress
/// (continue or abort it).
pub async fn rebase(root: PathBuf, onto: String) -> Result<(), String> {
    run(&root, &["rebase", &onto]).await.map(|_| ())
}

/// Merges `branch` into the current one (no fast-forward commit editor).
///
/// # Errors
/// Returns git's error text; conflicts leave the merge in progress.
pub async fn merge(root: PathBuf, branch: String) -> Result<(), String> {
    run(&root, &["merge", "--no-edit", &branch])
        .await
        .map(|_| ())
}

/// Continues (`--continue`) or aborts (`--abort`) the rebase or merge in
/// progress.
///
/// # Errors
/// Returns git's error text (unresolved conflicts, nothing in progress).
pub async fn continue_or_abort(root: PathBuf, abort: bool) -> Result<(), String> {
    let flag = if abort { "--abort" } else { "--continue" };
    let git_dir = root.join(".git");
    let verb = if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        "rebase"
    } else {
        "merge"
    };
    let output = tokio::process::Command::new("git")
        .current_dir(&root)
        .args([verb, flag])
        // `rebase --continue` would open an editor for the message.
        .env("GIT_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .await
        .map_err(|e| tr!("error-git-start", error = e.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// The web page of `path` (relative to the repository) at `line` on
/// GitHub, GitLab or Bitbucket, from the `origin` URL and the current
/// branch; `None` for other hosts.
pub fn remote_file_url(origin: &str, branch: &str, path: &str, line: usize) -> Option<String> {
    let (host, repo) = remote_parts(origin)?;
    Some(if host.contains("gitlab") {
        format!("https://{host}/{repo}/-/blob/{branch}/{path}#L{line}")
    } else if host.contains("bitbucket") {
        format!("https://{host}/{repo}/src/{branch}/{path}#lines-{line}")
    } else {
        format!("https://{host}/{repo}/blob/{branch}/{path}#L{line}")
    })
}

/// The page opening a pull (merge) request from `branch`.
pub fn pull_request_url(origin: &str, branch: &str) -> Option<String> {
    let (host, repo) = remote_parts(origin)?;
    Some(if host.contains("gitlab") {
        format!(
            "https://{host}/{repo}/-/merge_requests/new?merge_request%5Bsource_branch%5D={branch}"
        )
    } else if host.contains("bitbucket") {
        format!("https://{host}/{repo}/pull-requests/new?source={branch}")
    } else {
        format!("https://{host}/{repo}/compare/{branch}?expand=1")
    })
}

/// Host and `owner/repo` of a remote URL: `git@host:owner/repo.git`,
/// `ssh://git@host/owner/repo.git` or `https://host/owner/repo(.git)`.
fn remote_parts(origin: &str) -> Option<(String, String)> {
    let origin = origin.trim().trim_end_matches('/').trim_end_matches(".git");
    let rest = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
        .or_else(|| origin.strip_prefix("ssh://"))
        .map(|r| r.to_string())
        .or_else(|| origin.split_once('@').map(|(_, r)| r.replacen(':', "/", 1)))?;
    let rest = rest.rsplit_once('@').map_or(rest.as_str(), |(_, r)| r);
    let (host, repo) = rest.split_once('/')?;
    let host = host.split(':').next().unwrap_or(host);
    (!repo.is_empty()).then(|| (host.to_string(), repo.to_string()))
}

/// The `origin` URL and the current branch.
///
/// # Errors
/// Returns git's error text (no `origin`, detached `HEAD`).
pub async fn origin_and_branch(root: PathBuf) -> Result<(String, String), String> {
    let origin = run(&root, &["remote", "get-url", "origin"]).await?;
    let branch = run(&root, &["rev-parse", "--abbrev-ref", "HEAD"]).await?;
    Ok((origin.trim().to_string(), branch.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_urls_should_follow_each_host() {
        assert_eq!(
            remote_file_url("git@github.com:me/shop.git", "main", "app/User.php", 12).as_deref(),
            Some("https://github.com/me/shop/blob/main/app/User.php#L12")
        );
        assert_eq!(
            remote_file_url("https://gitlab.com/team/shop.git", "dev", "a.php", 3).as_deref(),
            Some("https://gitlab.com/team/shop/-/blob/dev/a.php#L3")
        );
        assert_eq!(
            pull_request_url("ssh://git@github.com/me/shop.git", "feature").as_deref(),
            Some("https://github.com/me/shop/compare/feature?expand=1")
        );
        assert_eq!(remote_file_url("nonsense", "main", "a", 1), None);
    }

    #[tokio::test]
    async fn stash_should_round_trip_local_changes_and_merge_branches() {
        let dir = std::env::temp_dir().join(format!("phpthunder-stash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "--quiet", "--initial-branch=main"]);
        git(&dir, &["config", "user.email", "test@example.com"]);
        git(&dir, &["config", "user.name", "Test"]);
        let file = dir.join("a.php");
        std::fs::write(&file, "one\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "--quiet", "-m", "one"]);

        std::fs::write(&file, "two\n").unwrap();
        stash(dir.clone(), "wip".into()).await.unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "one\n");
        let list = stashes(dir.clone()).await.unwrap();
        assert_eq!((list.len(), list[0].message.contains("wip")), (1, true));
        unstash(dir.clone(), 0, false).await.unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "two\n");
        assert!(stashes(dir.clone()).await.unwrap().is_empty());

        git(&dir, &["checkout", "--quiet", "--", "."]);
        git(&dir, &["switch", "--quiet", "--create", "side"]);
        std::fs::write(dir.join("b.php"), "b\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "--quiet", "-m", "side"]);
        git(&dir, &["switch", "--quiet", "main"]);
        merge(dir.clone(), "side".into()).await.unwrap();
        assert!(dir.join("b.php").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Runs `git args` in `dir` synchronously, for test setup.
    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .is_ok_and(|o| o.status.success());
        assert!(ok, "git {args:?} failed");
    }

    #[tokio::test]
    async fn commands_should_stage_commit_and_branch_in_a_repository() {
        let dir = std::env::temp_dir().join(format!("phpthunder-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "--quiet", "--initial-branch=main"]);
        git(&dir, &["config", "user.email", "test@example.com"]);
        git(&dir, &["config", "user.name", "Test"]);
        let file = dir.join("a.php");
        std::fs::write(&file, "<?php\necho 1;\n").unwrap();

        let s = status(dir.clone()).await.unwrap();
        assert_eq!(s.files[0].change, Change::Untracked);
        assert_eq!(head_content(dir.clone(), file.clone()).await, None);

        stage(dir.clone(), vec![file.clone()]).await.unwrap();
        assert!(status(dir.clone()).await.unwrap().has_staged());
        let summary = commit(dir.clone(), "first".into()).await.unwrap();
        assert!(summary.contains("first"), "{summary}");
        assert_eq!(
            head_content(dir.clone(), file.clone()).await.as_deref(),
            Some("<?php\necho 1;\n")
        );

        std::fs::write(&file, "<?php\necho 2;\n").unwrap();
        let s = status(dir.clone()).await.unwrap();
        assert_eq!(
            (s.files[0].change, s.files[0].unstaged),
            (Change::Modified, true)
        );
        discard(dir.clone(), vec![file.clone()]).await.unwrap();
        assert!(status(dir.clone()).await.unwrap().files.is_empty());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "<?php\necho 1;\n");

        create_branch(dir.clone(), "feature".into()).await.unwrap();
        let b = branches(dir.clone()).await.unwrap();
        assert!(b.iter().any(|b| b.name == "feature" && b.current));
        assert!(commit(dir.clone(), "empty".into()).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
