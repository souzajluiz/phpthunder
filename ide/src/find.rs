//! Find and replace: in the current file (find bar) and across the project
//! (find in path, results in the bottom panel).

use regex::{Regex, RegexBuilder};
use std::path::{Path, PathBuf};

pub const FIND_INPUT: &str = "find-input";
pub const REPLACE_INPUT: &str = "replace-input";
pub const SEARCH_INPUT: &str = "search-input";
const MAX_MATCHES: usize = 10_000;
const MAX_PROJECT_HITS: usize = 5_000;
const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    Regex,
    Case,
    Word,
    Vendor,
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
}

impl Options {
    pub fn toggle(&mut self, t: Toggle) {
        match t {
            Toggle::Regex => self.regex = !self.regex,
            Toggle::Case => self.case_sensitive = !self.case_sensitive,
            Toggle::Word => self.whole_word = !self.whole_word,
            Toggle::Vendor => {}
        }
    }

    pub fn build(&self, query: &str) -> Result<Regex, String> {
        let mut pattern = if self.regex {
            query.to_string()
        } else {
            regex::escape(query)
        };
        if self.whole_word {
            pattern = format!(r"\b(?:{pattern})\b");
        }
        RegexBuilder::new(&pattern)
            .case_insensitive(!self.case_sensitive)
            .multi_line(true)
            .build()
            .map_err(|e| {
                e.to_string()
                    .lines()
                    .last()
                    .unwrap_or("invalid pattern")
                    .trim()
                    .to_string()
            })
    }
}

#[derive(Default)]
pub struct FindBar {
    pub query: String,
    pub replacement: String,
    pub options: Options,
    pub show_replace: bool,
    pub matches: Vec<(usize, usize)>,
    pub current: Option<usize>,
    pub error: Option<String>,
}

impl FindBar {
    /// Recomputes matches for `text`; the current match becomes the first at
    /// or after `from`.
    pub fn recompute(&mut self, text: &str, from: usize) {
        self.matches.clear();
        self.current = None;
        self.error = None;
        if self.query.is_empty() {
            return;
        }
        match self.options.build(&self.query) {
            Ok(re) => {
                self.matches = re
                    .find_iter(text)
                    .filter(|m| !m.is_empty())
                    .take(MAX_MATCHES)
                    .map(|m| (m.start(), m.end()))
                    .collect();
                if !self.matches.is_empty() {
                    let i = self.matches.partition_point(|&(s, _)| s < from);
                    self.current = Some(if i < self.matches.len() { i } else { 0 });
                }
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn step(&mut self, forward: bool) -> Option<(usize, usize)> {
        let n = self.matches.len();
        if n == 0 {
            return None;
        }
        let next = match self.current {
            None => 0,
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
        };
        self.current = Some(next);
        Some(self.matches[next])
    }

    /// Replacement text for the match `m` (expands `$1` in regex mode).
    pub fn replacement_for(&self, text: &str, m: (usize, usize)) -> Option<String> {
        let re = self.options.build(&self.query).ok()?;
        if !self.options.regex {
            return Some(self.replacement.clone());
        }
        let caps = re.captures_at(text, m.0)?;
        let mut out = String::new();
        caps.expand(&self.replacement, &mut out);
        Some(out)
    }

    /// The whole text with every match replaced.
    pub fn replace_all(&self, text: &str) -> Option<(String, usize)> {
        let re = self.options.build(&self.query).ok()?;
        let count = re.find_iter(text).filter(|m| !m.is_empty()).count();
        let replaced = if self.options.regex {
            re.replace_all(text, self.replacement.as_str()).into_owned()
        } else {
            re.replace_all(text, regex::NoExpand(&self.replacement))
                .into_owned()
        };
        Some((replaced, count))
    }
}

// ----- find in path ------------------------------------------------------

#[derive(Default, Clone)]
pub struct SearchDialog {
    pub query: String,
    pub mask: String,
    pub options: Options,
    pub include_vendor: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Hit {
    /// Zero-based line and byte range of the match within that line.
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub preview: String,
    /// Byte offset of the preview inside the original line (leading
    /// whitespace is trimmed).
    pub preview_shift: usize,
}

#[derive(Debug, Clone)]
pub struct FileHits {
    pub path: PathBuf,
    pub hits: Vec<Hit>,
}

#[derive(Debug, Clone)]
pub struct SearchResults {
    pub query: String,
    pub files: Vec<FileHits>,
    pub total: usize,
    pub truncated: bool,
}

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
