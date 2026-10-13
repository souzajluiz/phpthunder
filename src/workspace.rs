//! Initial project scan: find PHP files and index them in parallel.

use crate::index::{FileEntry, Index, content_hash, index_source};
use parking_lot::RwLock;
use rayon::prelude::*;
use std::path::{Path, PathBuf};

/// Directories that never contain project sources worth indexing.
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    ".idea",
    ".vscode",
    "node_modules",
    "target",
    ".codegraph",
];

/// Recursively lists `*.php` files under `root`, skipping VCS, editor, and
/// build directories (`.git`, `node_modules`, `target`, ...). Unreadable
/// entries are ignored. Called once per workspace root by the
/// server's initial scan; `vendor/` is included on purpose so library classes
/// resolve.
pub fn php_files(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            !(e.file_type().is_dir()
                && e.file_name()
                    .to_str()
                    .is_some_and(|n| SKIPPED_DIRS.contains(&n)))
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().is_some_and(|x| x == "php"))
        .map(|e| e.into_path())
        .collect()
}

/// Reads and indexes `paths` in parallel, applying results in batches so the
/// index stays readable (partially) while the scan runs. Files whose content
/// hash is unchanged are skipped. `progress(done, total)` is called per batch.
///
/// Runs on a blocking worker during the server's initial scan; unreadable files
/// are skipped silently.
pub fn index_files(
    index: &RwLock<Index>,
    paths: &[PathBuf],
    mut progress: impl FnMut(usize, usize),
) {
    const BATCH: usize = 256;
    let mut done = 0;
    for chunk in paths.chunks(BATCH) {
        let entries: Vec<(PathBuf, FileEntry)> = chunk
            .par_iter()
            .filter_map(|p| {
                let src = std::fs::read_to_string(p).ok()?;
                let current = index.read().is_current(p, content_hash(&src));
                (!current).then(|| (p.clone(), index_source(&src)))
            })
            .collect();
        {
            let mut idx = index.write();
            for (p, entry) in entries {
                idx.update(p, entry);
            }
        }
        done += chunk.len();
        progress(done, paths.len());
    }
}
