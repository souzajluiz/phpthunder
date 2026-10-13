//! Watching the project folder for changes made outside the editor (git in
//! the terminal, Composer, code generators), so tabs, the tree, the index
//! and the Git status stay current.

use iced::futures::Stream;
use notify::{RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// How long a burst of changes may keep coming before it is reported as
/// one batch (a `git checkout` or `composer update` touches many files).
const SETTLE: Duration = Duration::from_millis(200);

/// Folders whose churn is never interesting, by name.
const IGNORED_DIRS: &[&str] = &["node_modules", "target", ".idea", ".codegraph"];

/// Whether a change to `path` is worth reporting. Inside `.git` only the
/// files that move `HEAD`, the index or refs matter (they change the Git
/// status); its object store is noise.
fn is_interesting(path: &Path) -> bool {
    let mut parts = path.components().map(|c| c.as_os_str().to_string_lossy());
    if parts.clone().any(|p| IGNORED_DIRS.contains(&p.as_ref())) {
        return false;
    }
    match parts.position(|p| p == ".git") {
        None => true,
        Some(_) => {
            let rest: Vec<String> = path
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .skip_while(|p| p != ".git")
                .skip(1)
                .collect();
            matches!(
                rest.first().map(String::as_str),
                Some("HEAD" | "index" | "refs" | "MERGE_HEAD")
            )
        }
    }
}

/// Watcher plus the channel its callback feeds.
type WatchState = (notify::RecommendedWatcher, UnboundedReceiver<PathBuf>);

/// Starts watching `root` recursively; `None` when the platform refuses.
fn start(root: &Path) -> Option<WatchState> {
    let (tx, rx) = unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        if let Ok(event) = result {
            for path in event.paths {
                let _ = tx.send(path);
            }
        }
    })
    .ok()?;
    watcher.watch(root, RecursiveMode::Recursive).ok()?;
    Some((watcher, rx))
}

/// Batches of changed paths under `root` (deduplicated, filtered), for
/// `Subscription::run_with`. Each batch is sent once changes settle.
#[expect(
    clippy::ptr_arg,
    reason = "Subscription::run_with passes &D with D = PathBuf"
)]
pub fn file_changes(root: &PathBuf) -> impl Stream<Item = Vec<PathBuf>> + use<> {
    let root = root.clone();
    iced::futures::stream::unfold(None::<WatchState>, move |state| {
        let root = root.clone();
        async move {
            let (watcher, mut rx) = match state {
                Some(s) => s,
                None => start(&root)?,
            };
            loop {
                let first = rx.recv().await?;
                let mut batch = vec![first];
                while let Ok(Some(path)) = tokio::time::timeout(SETTLE, rx.recv()).await {
                    batch.push(path);
                }
                batch.retain(|p| is_interesting(p));
                batch.sort();
                batch.dedup();
                if !batch.is_empty() {
                    return Some((batch, Some((watcher, rx))));
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_interesting_should_skip_git_objects_and_build_folders() {
        assert!(is_interesting(Path::new("/p/app/User.php")));
        assert!(is_interesting(Path::new("/p/.git/HEAD")));
        assert!(is_interesting(Path::new("/p/.git/refs/heads/main")));
        assert!(!is_interesting(Path::new("/p/.git/objects/ab/cdef")));
        assert!(!is_interesting(Path::new("/p/node_modules/x/index.js")));
    }

    #[tokio::test]
    async fn file_changes_should_report_new_files() {
        use iced::futures::StreamExt;
        let dir = std::env::temp_dir().join(format!("phpthunder-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let mut stream = Box::pin(file_changes(&dir));
        let file = dir.join("New.php");
        // The watcher may take a moment to start (FSEvents on a busy
        // machine): keep touching the file until a change is seen.
        let writer = {
            let file = file.clone();
            tokio::spawn(async move {
                for n in 0..30 {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    std::fs::write(&file, format!("<?php // {n}")).unwrap();
                }
            })
        };
        // Earlier batches may report the folder itself being created.
        let found = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(batch) = stream.next().await {
                if batch.iter().any(|p| p.ends_with("New.php")) {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap_or(false);
        writer.abort();
        assert!(found, "no change reported for New.php");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
