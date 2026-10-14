//! Backing up and syncing the user's settings: the files of
//! `~/.config/phpthunder` that describe how the user works (settings,
//! saved words, dictionaries, file templates, completion statistics),
//! exported to one JSON file and imported back, or kept in a Git
//! repository shared between machines. The session, logs, local history
//! and task timers stay on each machine.

use std::path::{Path, PathBuf};

/// Top-level entries that belong to one machine and are never shared.
const LOCAL_ONLY: &[&str] = &[
    "session.json",
    "errors.log",
    "tasks",
    "history",
    "local-history",
    ".git",
];

/// The `.gitignore` of a settings repository.
const GITIGNORE: &str = "session.json\nerrors.log\ntasks/\nhistory/\nlocal-history/\n";

/// Every shared file under `dir`, as (path relative to `dir` with `/`,
/// text). Binary or unreadable files are skipped.
pub fn shared_files(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(dir) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if LOCAL_ONLY
                .iter()
                .any(|l| rel == *l || rel.starts_with(&format!("{l}/")))
            {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                out.push((rel, text));
            }
        }
    }
    out.sort();
    out
}

/// The export file's text: a JSON object with the format version and the
/// shared files.
pub fn export(dir: &Path) -> String {
    let files: serde_json::Map<String, serde_json::Value> = shared_files(dir)
        .into_iter()
        .map(|(k, v)| (k, serde_json::Value::String(v)))
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({ "phpthunder-settings": 1, "files": files }))
        .unwrap_or_default()
}

/// Writes the files of an export into `dir`, replacing same-named ones;
/// returns how many were written.
///
/// # Errors
/// Returns a message when `text` is not an export or a file cannot be
/// written. Paths that would leave `dir` are refused.
pub fn import(dir: &Path, text: &str) -> Result<usize, String> {
    let json: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if json.get("phpthunder-settings").is_none() {
        return Err(crate::tr!("settings-import-invalid"));
    }
    let files = json["files"]
        .as_object()
        .ok_or_else(|| crate::tr!("settings-import-invalid"))?;
    let mut written = 0;
    for (rel, content) in files {
        let safe = Path::new(rel)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
        if !safe {
            return Err(crate::tr!("settings-import-invalid"));
        }
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, content.as_str().unwrap_or_default()).map_err(|e| e.to_string())?;
        written += 1;
    }
    Ok(written)
}

/// Runs `git args` in `dir`; its output, or its error text.
async fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = tokio::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Syncs `dir` with the Git repository `remote`: makes `dir` a repository
/// on first use, commits local changes, pulls (rebasing) and pushes.
///
/// # Errors
/// Returns Git's message when a step fails (no access, conflicts).
pub async fn sync(dir: PathBuf, remote: String) -> Result<(), String> {
    if !dir.join(".git").is_dir() {
        git(&dir, &["init", "-q", "-b", "main"]).await?;
        git(&dir, &["remote", "add", "origin", &remote]).await?;
    } else {
        let _ = git(&dir, &["remote", "set-url", "origin", &remote]).await;
    }
    std::fs::write(dir.join(".gitignore"), GITIGNORE).map_err(|e| e.to_string())?;
    git(&dir, &["add", "-A"]).await?;
    // Nothing to commit is fine.
    let _ = git(
        &dir,
        &[
            "-c",
            "user.name=PhpThunder",
            "-c",
            "user.email=settings@phpthunder.local",
            "commit",
            "-q",
            "-m",
            "Sync settings",
        ],
    )
    .await;
    // An empty remote has nothing to pull yet.
    if git(
        &dir,
        &["ls-remote", "--exit-code", "--heads", "origin", "main"],
    )
    .await
    .is_ok()
    {
        git(&dir, &["pull", "-q", "--rebase", "origin", "main"]).await?;
    }
    git(&dir, &["push", "-q", "origin", "HEAD:main"]).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config folder with shared and machine-only files.
    fn folder(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("phpthunder-sync-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("templates")).unwrap();
        std::fs::write(dir.join("settings.toml"), "keymap = \"vscode\"\n").unwrap();
        std::fs::write(dir.join("templates/Action.php"), "<?php").unwrap();
        std::fs::write(dir.join("session.json"), "{}").unwrap();
        dir
    }

    #[test]
    fn export_should_leave_machine_files_out() {
        let dir = folder("export");
        let names: Vec<String> = shared_files(&dir).into_iter().map(|(n, _)| n).collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(names, ["settings.toml", "templates/Action.php"]);
    }

    #[test]
    fn import_should_restore_an_export() {
        let from = folder("from");
        let text = export(&from);
        let to = std::env::temp_dir().join(format!("phpthunder-sync-to-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&to);
        let written = import(&to, &text).unwrap();
        let restored = std::fs::read_to_string(to.join("templates/Action.php")).unwrap();
        std::fs::remove_dir_all(&from).unwrap();
        std::fs::remove_dir_all(&to).unwrap();
        assert_eq!((written, restored.as_str()), (2, "<?php"));
    }

    #[test]
    fn import_should_refuse_paths_leaving_the_folder() {
        let text = r#"{"phpthunder-settings": 1, "files": {"../evil": "x"}}"#;
        assert!(import(Path::new("/nonexistent"), text).is_err());
    }
}
