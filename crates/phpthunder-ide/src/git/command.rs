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

#[cfg(test)]
mod tests {
    use super::*;

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
