//! Local history: a copy of a file's text kept on every save, independent
//! of Git, so earlier versions can be compared and restored. Versions live
//! in the user cache folder, one folder per file path, newest kept.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Most versions kept per file; older ones are deleted on save.
const KEEP: usize = 50;

/// The cache folder holding every file's history: `phpthunder/history`
/// under the user cache folder (`~/Library/Caches` on macOS).
pub fn base_dir() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let base = if cfg!(target_os = "macos") {
        home.join("Library/Caches")
    } else {
        std::env::var_os("XDG_CACHE_HOME").map_or_else(|| home.join(".cache"), PathBuf::from)
    };
    Some(base.join("phpthunder").join("history"))
}

/// The folder of `file`'s versions inside `base`, named by a hash of its
/// path.
fn dir_for(base: &Path, file: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    file.hash(&mut hasher);
    base.join(format!("{:016x}", hasher.finish()))
}

/// One stored version: when it was saved (Unix milliseconds) and its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// Save time in Unix milliseconds.
    pub saved_at: u128,
    /// The stored copy.
    pub path: PathBuf,
}

/// Versions of `file` in `base`, newest first.
pub fn versions(base: &Path, file: &Path) -> Vec<Version> {
    let Ok(entries) = std::fs::read_dir(dir_for(base, file)) else {
        return Vec::new();
    };
    let mut out: Vec<Version> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let saved_at = path.file_stem()?.to_str()?.parse().ok()?;
            Some(Version { saved_at, path })
        })
        .collect();
    out.sort_by_key(|v| std::cmp::Reverse(v.saved_at));
    out
}

/// Stores `text` as the newest version of `file` in `base`, unless it
/// equals the newest one, and drops versions beyond the limit. Failures
/// are ignored: history is a convenience and must never block a save.
pub fn record(base: &Path, file: &Path, text: &str) {
    let existing = versions(base, file);
    if existing
        .first()
        .and_then(|v| std::fs::read_to_string(&v.path).ok())
        .is_some_and(|latest| latest == text)
    {
        return;
    }
    let dir = dir_for(base, file);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let _ = std::fs::write(dir.join(format!("{now}.txt")), text);
    for old in existing.iter().skip(KEEP - 1) {
        let _ = std::fs::remove_file(&old.path);
    }
}

/// How long ago `saved_at` (Unix milliseconds) was, as `5 s`, `12 min`,
/// `3 h` or `2 d`.
pub fn age(saved_at: u128) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let secs = now.saturating_sub(saved_at) / 1000;
    match secs {
        0..60 => format!("{secs} s"),
        60..3600 => format!("{} min", secs / 60),
        3600..86_400 => format!("{} h", secs / 3600),
        _ => format!("{} d", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_base(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("phpthunder-history-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn record_should_skip_a_copy_equal_to_the_newest() {
        let base = temp_base("dup");
        let file = Path::new("/p/a.php");
        record(&base, file, "one");
        record(&base, file, "one");
        assert_eq!(versions(&base, file).len(), 1);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn versions_should_list_newest_first() {
        let base = temp_base("order");
        let file = Path::new("/p/b.php");
        record(&base, file, "one");
        std::thread::sleep(std::time::Duration::from_millis(5));
        record(&base, file, "two");
        let newest = versions(&base, file)
            .first()
            .and_then(|v| std::fs::read_to_string(&v.path).ok());
        assert_eq!(newest.as_deref(), Some("two"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
