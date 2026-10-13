//! Index data: per-file entries and references into them.

use crate::symbol::Symbol;

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
    pub words: rustc_hash::FxHashSet<String>,
}

/// Reference to a symbol stored in the index: the file (shared, so the
/// many references into one file cost one allocation) plus the position in
/// its [`FileEntry::symbols`]. Invalid once that file is re-indexed.
pub type SymRef = (std::sync::Arc<std::path::Path>, usize);

/// An indexed file as a member of the word and augment sets.
///
/// Hashes and compares the raw path bytes in one call: `Path`'s own `Hash`
/// walks components and dominated the time to load the index cache. Paths
/// come from the same walker and editor, so byte equality matches.
#[derive(Clone, Debug)]
pub struct FileKey(pub std::sync::Arc<std::path::Path>);

impl std::hash::Hash for FileKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write(self.0.as_os_str().as_encoded_bytes());
    }
}

impl PartialEq for FileKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_os_str() == other.0.as_os_str()
    }
}

impl Eq for FileKey {}
