//! Initial project scan: find PHP files and index them in parallel.

use crate::index::{content_hash, index_source, FileEntry, Index};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Directories that never contain project sources worth indexing.
const SKIPPED_DIRS: &[&str] = &[".git", ".idea", ".vscode", "node_modules", "target", ".codegraph"];

pub fn php_files(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            !(e.file_type().is_dir()
                && e.file_name().to_str().is_some_and(|n| SKIPPED_DIRS.contains(&n)))
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
pub fn index_files(index: &RwLock<Index>, paths: &[PathBuf], mut progress: impl FnMut(usize, usize)) {
    const BATCH: usize = 256;
    let mut done = 0;
    for chunk in paths.chunks(BATCH) {
        let entries: Vec<(PathBuf, FileEntry)> = chunk
            .par_iter()
            .filter_map(|p| {
                let src = std::fs::read_to_string(p).ok()?;
                let current = index.read().ok()?.is_current(p, content_hash(&src));
                (!current).then(|| (p.clone(), index_source(&src)))
            })
            .collect();
        {
            let mut idx = index.write().expect("index lock poisoned");
            for (p, entry) in entries {
                idx.update(p, entry);
            }
        }
        done += chunk.len();
        progress(done, paths.len());
    }
}
