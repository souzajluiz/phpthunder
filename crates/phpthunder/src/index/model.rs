//! Index data: per-file entries and references into them.

use crate::symbol::Symbol;
use std::collections::HashSet;
use std::path::PathBuf;

/// Everything the index stores for one file.
///
/// Built off-lock by [`index_source`] (in parallel during the initial scan) and
/// installed with [`Index::update`]. Serializable for the on-disk cache.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct FileEntry {
    /// [`content_hash`] of the source; lets rescans skip unchanged files.
    pub hash: u64,
    /// Declaration stubs in source order; a [`SymRef`] indexes into this vec.
    pub symbols: Vec<Symbol>,
    /// Lowercased identifiers in the file (its word index entries).
    pub words: HashSet<String>,
}

/// Reference to a symbol stored in the index: the file plus the position in
/// its [`FileEntry::symbols`]. Invalid once that file is re-indexed.
pub type SymRef = (PathBuf, usize);
