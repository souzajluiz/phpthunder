//! Find in path: line-by-line search over project files.

use super::*;
use std::path::{Path, PathBuf};

/// Cap on total find-in-path hits; results are marked truncated beyond it.
const MAX_PROJECT_HITS: usize = 5_000;

/// Files larger than this (2 MiB) are skipped by find in path.
const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

/// Whether `path`'s file name matches the comma-separated `mask`.
///
/// `*.ext` entries match by extension; other entries match as a substring of the name
/// (surrounding `*` ignored). An empty mask matches everything.
fn mask_matches(mask: &str, path: &Path) -> bool {
    let mask = mask.trim();
    if mask.is_empty() {
        return true;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    mask.split(',')
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .any(|m| match m.strip_prefix("*.") {
            Some(ext) => name.ends_with(&format!(".{ext}")),
            None => name.contains(m.trim_matches('*')),
        })
}

/// Searches `files` line by line. Runs on a blocking thread.
pub fn search_files(
    files: &[PathBuf],
    query: &str,
    options: &Options,
    mask: &str,
) -> Result<SearchResults, String> {
    let re = options.build(query)?;
    let mut out = SearchResults {
        query: query.to_string(),
        files: Vec::new(),
        total: 0,
        truncated: false,
    };
    for path in files {
        if !mask_matches(mask, path) {
            continue;
        }
        if std::fs::metadata(path)
            .map(|m| m.len() > MAX_FILE_SIZE)
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        if text.contains('\0') {
            continue;
        }
        let mut hits = Vec::new();
        for (line_no, line) in text.lines().enumerate() {
            for m in re.find_iter(line).filter(|m| !m.is_empty()) {
                let shift = line.len() - line.trim_start().len();
                let shift = shift.min(m.start());
                let preview: String = line[shift..].chars().take(200).collect();
                hits.push(Hit {
                    line: line_no,
                    start: m.start(),
                    end: m.end(),
                    preview,
                    preview_shift: shift,
                });
                out.total += 1;
                if out.total >= MAX_PROJECT_HITS {
                    out.truncated = true;
                    break;
                }
            }
            if out.truncated {
                break;
            }
        }
        if !hits.is_empty() {
            out.files.push(FileHits {
                path: path.clone(),
                hits,
            });
        }
        if out.truncated {
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_bar_matches_and_replaces() {
        let text = "$user->name; $User->age; $username";
        let mut bar = FindBar {
            query: "user".into(),
            ..Default::default()
        };
        bar.recompute(text, 0);
        assert_eq!(bar.matches.len(), 3);
        bar.options.case_sensitive = true;
        bar.recompute(text, 0);
        assert_eq!(bar.matches.len(), 2);
        bar.options.whole_word = true;
        bar.recompute(text, 0);
        assert_eq!(bar.matches, vec![(1, 5)]);
        bar.options = Options {
            regex: true,
            ..Default::default()
        };
        bar.query = r"\$(\w+)->".into();
        bar.replacement = "$$this->$1->".into();
        let (out, n) = bar.replace_all(text).unwrap();
        assert_eq!(n, 2);
        assert_eq!(out, "$this->user->name; $this->User->age; $username");
        bar.query = "(".into();
        bar.recompute(text, 0);
        assert!(bar.error.is_some());
    }

    #[test]
    fn masks() {
        assert!(mask_matches("*.php", Path::new("/a/User.php")));
        assert!(!mask_matches("*.php", Path::new("/a/app.js")));
        assert!(mask_matches("*.js, *.php", Path::new("/a/app.js")));
        assert!(mask_matches("", Path::new("/a/app.js")));
    }
}
