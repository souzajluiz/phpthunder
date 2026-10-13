//! On-disk cache of the index, so a project opens without reparsing every
//! file: the entries are saved after a scan and loaded before the next one,
//! which then only reparses files whose content changed.

use super::*;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

/// Format tag written first; a cache from another engine version (whose
/// symbol layout may differ) is ignored.
const FORMAT: &str = concat!("phpthunder-index/2/", env!("CARGO_PKG_VERSION"));

/// Size and modification time of a file when it was cached; a file whose
/// stamp is unchanged need not even be read.
pub type FileStamp = (u64, u128);

/// The stamp of the file at `path`, if it exists.
pub fn file_stamp(path: &Path) -> Option<FileStamp> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some((meta.len(), modified.as_nanos()))
}

/// The cache as written: the format tag and every file's entry.
#[derive(serde::Serialize)]
struct CacheOut<'a> {
    /// [`FORMAT`].
    format: &'a str,
    /// Indexed files, their stamps and their entries.
    entries: Vec<(&'a Path, Option<FileStamp>, &'a FileEntry)>,
}

/// The cache as read back.
#[derive(serde::Deserialize)]
struct CacheIn {
    /// Format tag of the writer.
    format: String,
    /// Indexed files, their stamps and their entries.
    entries: Vec<(PathBuf, Option<FileStamp>, FileEntry)>,
}

/// Writes every entry of `index` to `path` (through a temporary file, so a
/// crash never leaves a truncated cache).
///
/// # Errors
/// Returns the I/O or encoding error.
pub fn save_cache(index: &Index, path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let out = CacheOut {
        format: FORMAT,
        entries: index
            .entries()
            .map(|(p, e)| (p, file_stamp(p), e))
            .collect(),
    };
    let file = BufWriter::new(std::fs::File::create(&tmp)?);
    bincode::serialize_into(file, &out).map_err(std::io::Error::other)?;
    std::fs::rename(tmp, path)
}

/// Entries saved by [`save_cache`], each with the stamp its file had then;
/// `None` when the cache is missing, unreadable or from another engine
/// version.
pub fn load_cache(path: &Path) -> Option<Vec<(PathBuf, Option<FileStamp>, FileEntry)>> {
    let file = BufReader::new(std::fs::File::open(path).ok()?);
    let cache: CacheIn = bincode::deserialize_from(file).ok()?;
    (cache.format == FORMAT).then_some(cache.entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_should_round_trip_entries() {
        let mut index = Index::default();
        let path = PathBuf::from("/p/User.php");
        index.update(
            path,
            index_source("<?php class User { public function name(): string {} }"),
        );
        let file =
            std::env::temp_dir().join(format!("phpthunder-cache-{}.bin", std::process::id()));
        save_cache(&index, &file).unwrap();
        let entries = load_cache(&file).unwrap();
        let _ = std::fs::remove_file(&file);
        let mut restored = Index::default();
        for (p, stamp, e) in entries {
            assert_eq!(stamp, None, "the test path does not exist");
            restored.update(p, e);
        }
        assert_eq!(restored.classes("User").len(), 1);
        assert_eq!(
            restored
                .find_member("User", crate::symbol::MemberTag::Method, "name")
                .len(),
            1
        );
    }
}
