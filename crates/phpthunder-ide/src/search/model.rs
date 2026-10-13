//! Search state: the find bar, the find-in-path dialog, options and results.

use regex::{Regex, RegexBuilder};
use std::path::PathBuf;

/// Widget id of the find bar's query input, focused and selected when the bar opens.
pub const FIND_INPUT: &str = "find-input";

/// Widget id of the find bar's replacement input.
pub const REPLACE_INPUT: &str = "replace-input";

/// Widget id of the find-in-path dialog's query input, focused when the dialog opens.
pub const SEARCH_INPUT: &str = "search-input";

/// Cap on matches collected in one file so huge files keep the find bar responsive.
const MAX_MATCHES: usize = 10_000;

/// Option buttons shown in the find bar and find-in-path dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    /// Interpret the query as a regular expression.
    Regex,
    /// Match case.
    Case,
    /// Match whole words only.
    Word,
    /// Also search `vendor/` files (find in path only; ignored by `Options`).
    Vendor,
}

/// Matching options shared by the find bar and find in path.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Query is a regex rather than literal text.
    pub regex: bool,
    /// Case-sensitive matching.
    pub case_sensitive: bool,
    /// Wrap the pattern in word boundaries.
    pub whole_word: bool,
}

impl Options {
    /// Flips the option for a clicked toggle button.
    ///
    /// `Toggle::Vendor` is a no-op here; the app flips `SearchDialog::include_vendor` itself.
    pub fn toggle(&mut self, t: Toggle) {
        match t {
            Toggle::Regex => self.regex = !self.regex,
            Toggle::Case => self.case_sensitive = !self.case_sensitive,
            Toggle::Word => self.whole_word = !self.whole_word,
            Toggle::Vendor => {}
        }
    }

    /// Compiles `query` into a multi-line regex honoring these options.
    ///
    /// Literal queries are escaped; whole-word wraps the pattern in `\b` boundaries.
    ///
    /// # Errors
    ///
    /// Returns the last line of the regex compiler's message when the pattern is
    /// invalid, short enough to show inline in the find bar or dialog.
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

/// State of the in-file find/replace bar, held by the app while the bar is open.
#[derive(Default)]
pub struct FindBar {
    /// Text typed in the find input.
    pub query: String,
    /// Text typed in the replace input.
    pub replacement: String,
    /// Regex, case and whole-word options.
    pub options: Options,
    /// Whether the replace row is visible (opened via Replace).
    pub show_replace: bool,
    /// Byte ranges of all non-empty matches in the active document, in order.
    pub matches: Vec<(usize, usize)>,
    /// Index into `matches` of the highlighted match.
    pub current: Option<usize>,
    /// Pattern compile error shown instead of a match count.
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

    /// Advances to the next (or previous) match, wrapping around, and returns its range.
    ///
    /// Called by Find Next / Find Previous; returns `None` when there are no matches.
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

/// Find-in-path dialog state; kept after a search so it reopens with the same settings.
#[derive(Default, Clone)]
pub struct SearchDialog {
    /// Text to search for.
    pub query: String,
    /// Comma-separated file-name mask such as `*.php, *.js`; empty matches all files.
    pub mask: String,
    /// Regex, case and whole-word options.
    pub options: Options,
    /// Whether `vendor/` files are searched too.
    pub include_vendor: bool,
    /// Pattern compile error from the last submit, shown in the dialog.
    pub error: Option<String>,
}

/// One find-in-path match, shown as a row in the bottom panel.
#[derive(Debug, Clone)]
pub struct Hit {
    /// Zero-based line number of the match.
    pub line: usize,
    /// Byte offset where the match starts within the line.
    pub start: usize,
    /// Byte offset where the match ends within the line.
    pub end: usize,
    /// Line text from the first non-blank character, capped at 200 characters.
    pub preview: String,
    /// Byte offset of the preview inside the original line (leading
    /// whitespace is trimmed).
    pub preview_shift: usize,
}

/// All hits in one file, grouped under the file's header in the results panel.
#[derive(Debug, Clone)]
pub struct FileHits {
    /// File containing the hits.
    pub path: PathBuf,
    /// Hits in line order.
    pub hits: Vec<Hit>,
}

/// Outcome of a find-in-path run, shown in the bottom panel.
#[derive(Debug, Clone)]
pub struct SearchResults {
    /// Query that produced these results, shown in the panel header.
    pub query: String,
    /// Files with at least one hit, in the order they were searched.
    pub files: Vec<FileHits>,
    /// Total number of hits across all files.
    pub total: usize,
    /// Whether the search stopped early at `MAX_PROJECT_HITS`.
    pub truncated: bool,
}
