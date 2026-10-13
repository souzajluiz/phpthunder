//! Text editing actions beyond typing: line joins and splits, case and
//! line-order transforms, block comments, bracket navigation, structural
//! selection, statement completion, word completion, and surrounding or
//! unwrapping code with a block. All are plain text operations driven by
//! brackets and the highlighter's string/comment spans, so they work in
//! every language the editor highlights.

use super::document::leading_ws;
use crate::editor::{Document, Token};
use iced::widget::text_editor::{Cursor, Position};

/// Bracket pairs the navigation and selection actions understand.
const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

/// A word completion (`⌥/`) in progress: where the completed word starts,
/// the candidates in the order offered, and which one is inserted now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordCompletion {
    /// Byte offset where the word being completed starts.
    start: usize,
    /// The prefix typed before the first `⌥/`.
    prefix: String,
    /// Distinct words starting with `prefix`, nearest to the caret first.
    candidates: Vec<String>,
    /// Index of the candidate currently in the text.
    index: usize,
}

/// A block construct to surround the selected lines with (`⌥⌘T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surround {
    /// `if () { … }`.
    If,
    /// `if () { … } else { }`.
    IfElse,
    /// `while () { … }`.
    While,
    /// `foreach ( as ) { … }`.
    Foreach,
    /// `for (;;) { … }`.
    For,
    /// `try { … } catch (\Exception $e) { }`.
    TryCatch,
    /// `try { … } finally { }`.
    TryFinally,
    /// `function () { … }`.
    Closure,
    /// `{ … }`.
    Braces,
    /// `( … )` around the selection itself.
    Parentheses,
    /// `" … "` around the selection itself.
    Quotes,
}

impl Surround {
    /// Every construct, in menu order.
    pub const ALL: [Surround; 11] = [
        Surround::If,
        Surround::IfElse,
        Surround::While,
        Surround::Foreach,
        Surround::For,
        Surround::TryCatch,
        Surround::TryFinally,
        Surround::Closure,
        Surround::Braces,
        Surround::Parentheses,
        Surround::Quotes,
    ];

    /// The code shown in the menu, which is also what gets inserted.
    pub fn label(self) -> &'static str {
        match self {
            Surround::If => "if () { }",
            Surround::IfElse => "if () { } else { }",
            Surround::While => "while () { }",
            Surround::Foreach => "foreach ( as ) { }",
            Surround::For => "for (;;) { }",
            Surround::TryCatch => "try { } catch () { }",
            Surround::TryFinally => "try { } finally { }",
            Surround::Closure => "function () { }",
            Surround::Braces => "{ }",
            Surround::Parentheses => "( )",
            Surround::Quotes => "\" \"",
        }
    }

    /// Header line before the block, and the text after its closing brace;
    /// `None` for the inline wrappers.
    fn block(self) -> Option<(&'static str, &'static str)> {
        Some(match self {
            Surround::If => ("if () {", "}"),
            Surround::IfElse => ("if () {", "} else {\n}"),
            Surround::While => ("while () {", "}"),
            Surround::Foreach => ("foreach ( as ) {", "}"),
            Surround::For => ("for (;;) {", "}"),
            Surround::TryCatch => ("try {", "} catch (\\Exception $e) {\n}"),
            Surround::TryFinally => ("try {", "} finally {\n}"),
            Surround::Closure => ("function () {", "};"),
            Surround::Braces => ("{", "}"),
            Surround::Parentheses | Surround::Quotes => return None,
        })
    }
}

/// Whether `c` belongs to a word for word completion and case toggling.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Document {
    /// Whether byte `offset` lies in a string or comment, where brackets
    /// are text rather than structure.
    fn in_literal(&self, offset: usize) -> bool {
        matches!(
            self.token_at(offset),
            Some(Token::String | Token::Comment | Token::DocTag)
        )
    }

    /// The bracket matching the one at byte `at` (opening or closing),
    /// skipping brackets inside strings and comments.
    pub fn matching_bracket(&self, at: usize) -> Option<usize> {
        let c = self.text[at..].chars().next()?;
        if self.in_literal(at) {
            return None;
        }
        if let Some(&(open, close)) = PAIRS.iter().find(|(o, _)| *o == c) {
            let mut depth = 0usize;
            for (i, ch) in self.text[at..].char_indices() {
                let i = at + i;
                if (ch == open || ch == close) && !self.in_literal(i) {
                    if ch == open {
                        depth += 1;
                    } else {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i);
                        }
                    }
                }
            }
            return None;
        }
        let &(open, close) = PAIRS.iter().find(|(_, cl)| *cl == c)?;
        let mut depth = 0usize;
        for (i, ch) in self.text[..=at].char_indices().rev() {
            if (ch == open || ch == close) && !self.in_literal(i) {
                if ch == close {
                    depth += 1;
                } else {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
            }
        }
        None
    }

    /// The bracket pair touching the caret: the bracket right after it,
    /// else the one right before it, with its match. Used to highlight the
    /// pair and by `⌃M`.
    pub fn bracket_pair_at_caret(&self) -> Option<(usize, usize)> {
        let caret = self.offset(self.cursor());
        let is_bracket = |c: char| PAIRS.iter().any(|(o, cl)| *o == c || *cl == c);
        let at = match (self.char_after(caret), self.char_before(caret)) {
            (Some(c), _) if is_bracket(c) => caret,
            (_, Some(c)) if is_bracket(c) => caret - c.len_utf8(),
            _ => return None,
        };
        let other = self.matching_bracket(at)?;
        Some((at.min(other), at.max(other)))
    }

    /// The innermost bracket pair strictly enclosing the range `lo..hi`,
    /// as the offsets of its opening and closing brackets.
    pub fn enclosing_pair(
        &self,
        lo: usize,
        hi: usize,
        only: Option<char>,
    ) -> Option<(usize, usize)> {
        let mut depth: Vec<char> = Vec::new();
        for (i, ch) in self.text[..lo].char_indices().rev() {
            if self.in_literal(i) {
                continue;
            }
            if let Some(&(open, _)) = PAIRS.iter().find(|(_, cl)| *cl == ch) {
                depth.push(open);
            } else if PAIRS.iter().any(|(o, _)| *o == ch) {
                if depth.last() == Some(&ch) {
                    depth.pop();
                } else if only.is_none_or(|o| o == ch) {
                    let close = self.matching_bracket(i)?;
                    if close >= hi {
                        return Some((i, close));
                    }
                }
            }
        }
        None
    }

    /// The identifier at the caret (a `$variable` with its `$`), unless it
    /// is inside a string or comment; used to highlight its occurrences.
    pub fn identifier_at_caret(&self) -> Option<(usize, usize)> {
        let caret = self.offset(self.cursor());
        let (mut start, end) = self.word_at(caret);
        if start == end || self.in_literal(start) {
            return None;
        }
        if self.char_before(start) == Some('$') {
            start -= 1;
        }
        let first = self.text[start..].chars().next()?;
        (first == '$' || first.is_alphabetic() || first == '_').then_some((start, end))
    }

    /// Start offsets of whole-word occurrences of `word` in `from..to`,
    /// outside strings and comments.
    pub fn occurrences_in(&self, word: &str, from: usize, to: usize) -> Vec<usize> {
        let to = to.min(self.text.len());
        let mut out = Vec::new();
        let mut at = from;
        while let Some(i) = self.text[at..to].find(word) {
            let start = at + i;
            let end = start + word.len();
            let bounded = self
                .char_before(start)
                .is_none_or(|c| !is_word(c) && c != '$')
                && self.char_after(end).is_none_or(|c| !is_word(c));
            if bounded && !self.in_literal(start) {
                out.push(start);
            }
            at = end;
        }
        out
    }

    /// Lines heading the `{` blocks that enclose the start of `line` and
    /// begin above it, outermost first, at most `limit`: what sticky lines
    /// pin to the top of the editor. A brace alone on its line is headed by
    /// the line before it.
    pub fn enclosing_headers(&self, line: usize, limit: usize) -> Vec<usize> {
        let offset = self.line_start(line);
        let mut headers = Vec::new();
        let (mut lo, mut hi) = (offset, offset);
        while let Some((open, close)) = self.enclosing_pair(lo, hi, Some('{')) {
            let mut header = self.position_of(open).line;
            if self.line_text(header).trim() == "{" && header > 0 {
                header -= 1;
            }
            if header < line {
                headers.push(header);
            }
            (lo, hi) = (open, close + 1);
        }
        headers.reverse();
        headers.truncate(limit);
        headers
    }

    /// `⌃M`: moves the caret to the bracket matching the one at the caret.
    pub fn jump_to_matching_bracket(&mut self) {
        let caret = self.offset(self.cursor());
        if let Some((a, b)) = self.bracket_pair_at_caret() {
            let target = if caret <= a { b } else { a };
            let pos = self.position_of(target);
            self.move_to(pos);
        }
    }

    /// `⌥⌘[` / `⌥⌘]`: moves the caret to the `{` or `}` of the enclosing
    /// code block.
    pub fn jump_to_block_edge(&mut self, end: bool) {
        let caret = self.offset(self.cursor());
        if let Some((open, close)) = self.enclosing_pair(caret, caret, Some('{')) {
            let pos = self.position_of(if end { close } else { open });
            self.move_to(pos);
        }
    }

    /// `⌥↑`: grows the selection one structural step: the word, the inside
    /// of the enclosing string or brackets, the brackets themselves, then
    /// the whole lines they span, and so on out to the whole text.
    pub fn extend_selection(&mut self) {
        let caret = self.offset(self.cursor());
        let (lo, hi) = self.selection().unwrap_or((caret, caret));
        if self.selection_stack.last() != Some(&(lo, hi)) {
            self.selection_stack.clear();
            self.selection_stack.push((lo, hi));
        }
        if let Some((a, b)) = self.grown(lo, hi) {
            self.selection_stack.push((a, b));
            self.select(a, b);
        }
    }

    /// `⌥↓`: steps the selection back to what `⌥↑` grew it from.
    pub fn shrink_selection(&mut self) {
        let current = self.selection();
        if current.is_some() && self.selection_stack.last().copied() == current {
            self.selection_stack.pop();
            if let Some(&(a, b)) = self.selection_stack.last() {
                if a == b {
                    let pos = self.position_of(a);
                    self.move_to(pos);
                    self.selection_stack.clear();
                } else {
                    self.select(a, b);
                }
            }
        }
    }

    /// The next larger structural range around `lo..hi`, if any.
    fn grown(&self, lo: usize, hi: usize) -> Option<(usize, usize)> {
        let text = &self.text;
        // A word around an empty or partial-word selection.
        let start = text[..lo]
            .char_indices()
            .rev()
            .take_while(|(_, c)| is_word(*c) || *c == '$')
            .last()
            .map_or(lo, |(i, _)| i);
        let end = text[hi..]
            .char_indices()
            .take_while(|(_, c)| is_word(*c))
            .last()
            .map_or(hi, |(i, c)| hi + i + c.len_utf8());
        if (start, end) != (lo, hi) && start < end {
            return Some((start, end));
        }
        // The string literal around the selection: inside, then with quotes.
        if self.token_at(lo) == Some(Token::String) && lo > 0 {
            let (s, e) = self.literal_span(lo);
            let inner = (s + 1, e.saturating_sub(1));
            if inner.0 <= lo && hi <= inner.1 && inner != (lo, hi) && inner.0 < inner.1 {
                return Some(inner);
            }
            if (s, e) != (lo, hi) && s <= lo && hi <= e {
                return Some((s, e));
            }
        }
        // The enclosing brackets: their inside, trimmed, then with them.
        if let Some((open, close)) = self.enclosing_pair(lo, hi, None) {
            let inner = (open + 1, close);
            let trimmed = trim_range(text, inner);
            if trimmed.0 <= lo && hi <= trimmed.1 && trimmed != (lo, hi) {
                return Some(trimmed);
            }
            // Whole lines when the block spans several.
            let lines = self.full_lines(open, close + 1);
            if (open, close + 1) != (lo, hi)
                && !(lines.0 <= lo && hi <= lines.1 && lines != (open, close + 1))
            {
                return Some((open, close + 1));
            }
            if lines != (lo, hi) {
                return Some(lines);
            }
        }
        let lines = self.full_lines(lo, hi);
        if lines != (lo, hi) {
            return Some(lines);
        }
        ((0, text.len()) != (lo, hi)).then_some((0, text.len()))
    }

    /// The string or comment span containing byte `offset`, from the
    /// highlighter's spans on that line.
    fn literal_span(&self, offset: usize) -> (usize, usize) {
        let pos = self.position_of(offset);
        let start = self.line_start(pos.line);
        let col = offset - start;
        self.highlights
            .get(pos.line)
            .and_then(|spans| spans.iter().find(|(r, _)| r.start <= col && col < r.end))
            .map_or((offset, offset), |(r, _)| (start + r.start, start + r.end))
    }

    /// `lo..hi` widened to whole lines, from the first non-blank character
    /// of the first line to the end of the last.
    fn full_lines(&self, lo: usize, hi: usize) -> (usize, usize) {
        let first = self.position_of(lo).line;
        let last = self.position_of(hi).line;
        let start = self.line_start(first) + leading_ws(self.line_text(first)).len();
        (start.min(lo), self.line_end(last).max(hi))
    }

    /// `⌃⇧J`: joins the next line (or every selected line) onto the current
    /// one, dropping the indentation in between; a space separates them
    /// except after `(`/`[` or before `)`/`]`/`,`/`;`.
    pub fn join_lines(&mut self) {
        let (lo, hi) = self.selected_lines();
        let hi = hi.max(lo + 1).min(self.line_count().saturating_sub(1));
        if hi == lo {
            return;
        }
        let mut joined = self.line_text(lo).trim_end().to_string();
        for line in lo + 1..=hi {
            let next = self.line_text(line).trim();
            let next = next
                .strip_prefix("// ")
                .filter(|_| joined.trim_start().starts_with("//"))
                .unwrap_or(next);
            let tight = joined.ends_with(['(', '['])
                || next.starts_with([')', ']', ',', ';'])
                || next.is_empty()
                || joined.is_empty();
            if !tight {
                joined.push(' ');
            }
            joined.push_str(next);
        }
        let (start, end) = (self.line_start(lo), self.line_end(hi));
        let caret = self.line_text(lo).trim_end().len();
        let mut text = self.text.clone();
        text.replace_range(start..end, &joined);
        self.rewrite(
            text,
            Cursor {
                position: Position {
                    line: lo,
                    column: caret,
                },
                selection: None,
            },
        );
    }

    /// `⌘↵`: breaks the line at the caret, keeping the caret where it was.
    pub fn split_line(&mut self) {
        let pos = self.cursor();
        let offset = self.offset(pos);
        let indent = leading_ws(self.line_text(pos.line)).to_string();
        self.checkpoint();
        self.replace(offset, offset, &format!("\n{indent}"));
        self.move_to(pos);
    }

    /// `⇧↵` (below) / `⌥⌘↵` (above): opens a new line with the current
    /// indentation, plus one level below a line ending in `{`.
    pub fn start_new_line(&mut self, above: bool) {
        let line = self.cursor().line;
        let content = self.line_text(line).to_string();
        let mut indent = leading_ws(&content).to_string();
        self.checkpoint();
        if above {
            let start = self.line_start(line);
            self.replace(start, start, &format!("{indent}\n"));
            self.move_to(Position {
                line,
                column: indent.len(),
            });
        } else {
            if content.trim_end().ends_with(['{', '(', '[']) {
                indent.push_str(&self.indent_unit);
            }
            let end = self.line_end(line);
            self.replace(end, end, &format!("\n{indent}"));
        }
    }

    /// `⇧⌘U`: upper-cases the selection (or the word at the caret), or
    /// lower-cases it when it has no lowercase letters.
    pub fn toggle_case(&mut self) {
        let caret = self.offset(self.cursor());
        let (a, b) = self.selection().unwrap_or_else(|| self.word_at(caret));
        if a == b {
            return;
        }
        let chunk = &self.text[a..b];
        let swapped = if chunk.chars().any(char::is_lowercase) {
            chunk.to_uppercase()
        } else {
            chunk.to_lowercase()
        };
        self.checkpoint();
        self.replace(a, b, &swapped);
        self.select(a, a + swapped.len());
    }

    /// The word around byte `offset`.
    fn word_at(&self, offset: usize) -> (usize, usize) {
        let start = self.text[..offset]
            .char_indices()
            .rev()
            .take_while(|(_, c)| is_word(*c))
            .last()
            .map_or(offset, |(i, _)| i);
        let end = self.text[offset..]
            .char_indices()
            .take_while(|(_, c)| is_word(*c))
            .last()
            .map_or(offset, |(i, c)| offset + i + c.len_utf8());
        (start, end)
    }

    /// Sorts, reverses or de-duplicates the selected lines (every line
    /// without a selection) with `transform`.
    pub fn transform_lines(&mut self, transform: LineTransform) {
        let (lo, hi) = if self.selection().is_some() {
            self.selected_lines()
        } else {
            (0, self.line_count().saturating_sub(1))
        };
        let (start, end) = (self.line_start(lo), self.line_end(hi));
        let mut lines: Vec<&str> = self.text[start..end].split('\n').collect();
        match transform {
            LineTransform::Sort => lines.sort_unstable(),
            LineTransform::Reverse => lines.reverse(),
            LineTransform::Unique => {
                let mut seen = std::collections::HashSet::new();
                lines.retain(|l| seen.insert(*l));
            }
        }
        let replaced = lines.join("\n");
        let mut text = self.text.clone();
        text.replace_range(start..end, &replaced);
        let cursor = Cursor {
            position: Position {
                line: lo,
                column: 0,
            },
            selection: None,
        };
        self.rewrite(text, cursor);
        let end = start + replaced.len();
        self.select(start, end);
    }

    /// `⌃T`: swaps the characters on both sides of the caret (the two
    /// before it at the end of a line).
    pub fn transpose(&mut self) {
        let mut caret = self.offset(self.cursor());
        if self.char_after(caret).is_none_or(|c| c == '\n') {
            match self.char_before(caret) {
                Some(c) if c != '\n' => caret -= c.len_utf8(),
                _ => return,
            }
        }
        let (Some(before), Some(after)) = (self.char_before(caret), self.char_after(caret)) else {
            return;
        };
        if before == '\n' || after == '\n' {
            return;
        }
        let start = caret - before.len_utf8();
        let end = caret + after.len_utf8();
        self.checkpoint();
        self.replace(start, end, &format!("{after}{before}"));
    }

    /// `⌥⇧⌘/`: wraps the selection (or the current line) in `/* */`, or
    /// removes the block comment the selection starts and ends with.
    pub fn toggle_block_comment(&mut self) {
        let (a, b) = self.selection().unwrap_or_else(|| {
            let line = self.cursor().line;
            let start = self.line_start(line) + leading_ws(self.line_text(line)).len();
            (start, self.line_end(line))
        });
        let chunk = self.text[a..b].to_string();
        self.checkpoint();
        if let Some(inner) = chunk.strip_prefix("/*").and_then(|c| c.strip_suffix("*/")) {
            let inner = inner.strip_prefix(' ').unwrap_or(inner);
            let inner = inner.strip_suffix(' ').unwrap_or(inner).to_string();
            self.replace(a, b, &inner);
            self.select(a, a + inner.len());
        } else {
            let wrapped = format!("/* {chunk} */");
            self.replace(a, b, &wrapped);
            self.select(a, a + wrapped.len());
        }
    }

    /// `⇧⌘↵`: finishes the statement on the caret line and moves to a new
    /// line: control headers (`if`, `foreach`, `function`, `class`, …) get
    /// their closing parenthesis and a `{ }` block, other code gets its
    /// missing closing brackets and `;`.
    pub fn complete_statement(&mut self) {
        let line = self.cursor().line;
        let content = self.line_text(line).to_string();
        let indent = leading_ws(&content).to_string();
        let trimmed = content.trim_end();
        let body = trimmed.trim_start();
        if body.is_empty() {
            return;
        }
        let mut fixed = trimmed.to_string();
        // Close brackets left open on this line, innermost first.
        let mut open: Vec<char> = Vec::new();
        for c in body.chars() {
            match c {
                '(' | '[' => open.push(c),
                ')' | ']' => {
                    open.pop();
                }
                _ => {}
            }
        }
        for c in open.iter().rev() {
            fixed.push(if *c == '(' { ')' } else { ']' });
        }
        let word = body
            .split(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap_or("");
        let header = matches!(
            word,
            "if" | "elseif"
                | "else"
                | "for"
                | "foreach"
                | "while"
                | "switch"
                | "try"
                | "catch"
                | "finally"
                | "function"
                | "class"
                | "interface"
                | "trait"
                | "enum"
                | "do"
                | "match"
        ) || body.starts_with("} else")
            || body.contains(" function ")
            || body.starts_with("public ") && body.contains("function")
            || body.starts_with("private ") && body.contains("function")
            || body.starts_with("protected ") && body.contains("function");
        let ends_block = fixed.ends_with(['{', '}', ';', ',', ':']) || fixed.ends_with("*/");
        self.checkpoint();
        let (start, end) = (self.line_start(line), self.line_end(line));
        if header && !fixed.ends_with('{') && !fixed.ends_with(';') {
            let inner = format!("{indent}{}", self.indent_unit);
            let replaced = format!("{fixed} {{\n{inner}\n{indent}}}");
            self.replace(start, end, &replaced);
            self.move_to(Position {
                line: line + 1,
                column: inner.len(),
            });
            return;
        }
        if !ends_block
            && !body.starts_with("//")
            && !body.starts_with('#')
            && !body.starts_with('*')
        {
            fixed.push(';');
        }
        let extra = if fixed.ends_with('{') {
            self.indent_unit.clone()
        } else {
            String::new()
        };
        let replaced = format!("{fixed}\n{indent}{extra}");
        self.replace(start, end, &replaced);
    }

    /// `⌥/`: completes the word before the caret from other words in the
    /// file, nearest first; pressing again cycles through the candidates.
    pub fn complete_word(&mut self) {
        let caret = self.offset(self.cursor());
        let continuing = self.word_completion.as_ref().is_some_and(|w| {
            let current = &w.candidates[w.index];
            caret == w.start + current.len() && self.text.get(w.start..caret) == Some(current)
        });
        if continuing {
            let Some(w) = self.word_completion.as_mut() else {
                return;
            };
            let old_end = w.start + w.candidates[w.index].len();
            w.index = (w.index + 1) % w.candidates.len();
            let (start, next) = (w.start, w.candidates[w.index].clone());
            self.checkpoint();
            self.replace(start, old_end, &next);
            return;
        }
        let (start, _) = self.word_at(caret);
        let prefix = self.text[start..caret].to_string();
        if prefix.is_empty() {
            return;
        }
        // Nearest first: words before the caret backwards, then after it.
        let mut candidates: Vec<String> = Vec::new();
        let mut consider = |word: &str| {
            if word.len() > prefix.len()
                && word.starts_with(prefix.as_str())
                && !candidates.iter().any(|c| c == word)
            {
                candidates.push(word.to_string());
            }
        };
        let words = |s: &str| -> Vec<String> {
            s.split(|c: char| !is_word(c))
                .filter(|w| !w.is_empty())
                .map(str::to_string)
                .collect()
        };
        for w in words(&self.text[..start]).iter().rev() {
            consider(w);
        }
        for w in words(&self.text[caret..]) {
            consider(&w);
        }
        if candidates.is_empty() {
            self.word_completion = None;
            return;
        }
        let first = candidates[0].clone();
        self.word_completion = Some(WordCompletion {
            start,
            prefix,
            candidates,
            index: 0,
        });
        self.checkpoint();
        self.replace(start, caret, &first);
    }

    /// `⇧⌘⌦`: removes the block construct around the caret (`if`,
    /// `foreach`, `try`, a bare `{ }`, …), keeping its body one level less
    /// indented; `else`/`catch`/`finally` branches go with it.
    pub fn unwrap_block(&mut self) {
        let caret = self.offset(self.cursor());
        let Some((open, close)) = self.enclosing_pair(caret, caret, Some('{')) else {
            return;
        };
        let header_line = self.position_of(open).line;
        let close_line = self.position_of(close).line;
        if close_line <= header_line {
            return;
        }
        // Branches chained after the closing brace (`} else { … }`).
        let mut end = close + 1;
        loop {
            let rest = &self.text[end..];
            let trimmed = rest.trim_start();
            let skipped = rest.len() - trimmed.len();
            let chained = ["else", "elseif", "catch", "finally"]
                .iter()
                .any(|k| trimmed.starts_with(k));
            if !chained {
                break;
            }
            let Some(brace) = self.text[end + skipped..]
                .find('{')
                .map(|i| end + skipped + i)
            else {
                break;
            };
            match self.matching_bracket(brace) {
                Some(c) => end = c + 1,
                None => break,
            }
        }
        let unit = self.indent_unit.len();
        let body: Vec<String> = (header_line + 1..close_line)
            .map(|l| {
                let line = self.line_text(l);
                let ws = leading_ws(line).len().min(unit);
                line[ws..].to_string()
            })
            .collect();
        let start = self.line_start(header_line);
        let end_line = self.position_of(end).line;
        let stop = self.line_end(end_line);
        let mut text = self.text.clone();
        text.replace_range(start..stop, &body.join("\n"));
        self.rewrite(
            text,
            Cursor {
                position: Position {
                    line: header_line,
                    column: 0,
                },
                selection: None,
            },
        );
    }

    /// `⌥⌘T`: surrounds the selected lines (or the current line) with
    /// `construct`, re-indenting them one level, and puts the caret in the
    /// construct's header; inline wrappers surround the selection itself.
    pub fn surround_with(&mut self, construct: Surround) {
        let Some((header, footer)) = construct.block() else {
            let caret = self.offset(self.cursor());
            let (a, b) = self.selection().unwrap_or_else(|| self.word_at(caret));
            let (l, r) = if construct == Surround::Quotes {
                ("\"", "\"")
            } else {
                ("(", ")")
            };
            let inner = self.text[a..b].to_string();
            self.checkpoint();
            self.replace(a, b, &format!("{l}{inner}{r}"));
            self.select(a + 1, a + 1 + inner.len());
            return;
        };
        let (lo, hi) = self.selected_lines();
        let indent = leading_ws(self.line_text(lo)).to_string();
        let unit = self.indent_unit.clone();
        let body: Vec<String> = (lo..=hi)
            .map(|l| {
                let line = self.line_text(l);
                if line.trim().is_empty() {
                    String::new()
                } else {
                    format!("{unit}{line}")
                }
            })
            .collect();
        let footer = footer.replace('\n', &format!("\n{indent}"));
        let block = format!("{indent}{header}\n{}\n{indent}{footer}", body.join("\n"));
        let (start, end) = (self.line_start(lo), self.line_end(hi));
        let mut text = self.text.clone();
        text.replace_range(start..end, &block);
        // Caret inside the header's parentheses, where the condition goes.
        let column = indent.len() + header.find('(').map_or(header.len() - 1, |i| i + 1);
        self.rewrite(
            text,
            Cursor {
                position: Position { line: lo, column },
                selection: None,
            },
        );
    }
}

/// A whole-line transform for [`Document::transform_lines`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineTransform {
    /// Sort lines in byte order.
    Sort,
    /// Reverse line order.
    Reverse,
    /// Drop repeated lines, keeping the first of each.
    Unique,
}

/// `range` without leading and trailing whitespace (unchanged when blank).
fn trim_range(text: &str, (a, b): (usize, usize)) -> (usize, usize) {
    let s = &text[a..b];
    let lead = s.len() - s.trim_start().len();
    let trail = s.len() - s.trim_end().len();
    if lead + trail >= s.len() {
        (a, b)
    } else {
        (a + lead, b - trail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn doc(text: &str) -> Document {
        Document::new(PathBuf::from("/t.php"), text.to_string())
    }

    fn at(d: &mut Document, offset: usize) {
        let pos = d.position_of(offset);
        d.move_to(pos);
    }

    #[test]
    fn matching_bracket_should_skip_brackets_in_strings() {
        let d = doc("<?php f('(', $a);");
        assert_eq!(d.matching_bracket(7), Some(15));
    }

    #[test]
    fn extend_selection_should_grow_word_then_brackets_inside() {
        let mut d = doc("<?php foo($bar, $baz);");
        at(&mut d, 12);
        d.extend_selection();
        assert_eq!(d.selected_text(), Some("$bar"));
        d.extend_selection();
        assert_eq!(d.selected_text(), Some("$bar, $baz"));
    }

    #[test]
    fn shrink_selection_should_step_back() {
        let mut d = doc("<?php foo($bar, $baz);");
        at(&mut d, 12);
        d.extend_selection();
        d.extend_selection();
        d.shrink_selection();
        assert_eq!(d.selected_text(), Some("$bar"));
    }

    #[test]
    fn join_lines_should_drop_indentation() {
        let mut d = doc("$a = [\n    1,\n];");
        d.join_lines();
        assert_eq!(d.text, "$a = [1,\n];");
    }

    #[test]
    fn toggle_case_should_upper_then_lower() {
        let mut d = doc("abc");
        d.toggle_case();
        assert_eq!(d.text, "ABC");
    }

    #[test]
    fn transform_lines_should_sort_all_lines_without_selection() {
        let mut d = doc("b\na\nc");
        d.transform_lines(LineTransform::Sort);
        assert_eq!(d.text, "a\nb\nc");
    }

    #[test]
    fn transform_lines_should_drop_duplicates() {
        let mut d = doc("a\nb\na");
        d.transform_lines(LineTransform::Unique);
        assert_eq!(d.text, "a\nb");
    }

    #[test]
    fn transpose_should_swap_around_caret() {
        let mut d = doc("ab");
        at(&mut d, 1);
        d.transpose();
        assert_eq!(d.text, "ba");
    }

    #[test]
    fn block_comment_should_round_trip() {
        let mut d = doc("$a = 1;");
        d.toggle_block_comment();
        d.toggle_block_comment();
        assert_eq!(d.text, "$a = 1;");
    }

    #[test]
    fn complete_statement_should_add_semicolon() {
        let mut d = doc("    $a = f(1");
        d.complete_statement();
        assert_eq!(d.text, "    $a = f(1);\n    ");
    }

    #[test]
    fn complete_statement_should_open_a_block_after_a_header() {
        let mut d = doc("if ($a");
        d.complete_statement();
        assert_eq!(d.text, "if ($a) {\n    \n}");
    }

    #[test]
    fn complete_word_should_cycle_nearest_first() {
        let mut d = doc("apple apricot ap");
        at(&mut d, 16);
        d.complete_word();
        assert_eq!(d.text, "apple apricot apricot");
    }

    #[test]
    fn complete_word_should_cycle_on_repeat() {
        let mut d = doc("apple apricot ap");
        at(&mut d, 16);
        d.complete_word();
        d.complete_word();
        assert_eq!(d.text, "apple apricot apple");
    }

    #[test]
    fn unwrap_block_should_keep_the_body_dedented() {
        let mut d = doc("if ($a) {\n    f();\n}\n");
        at(&mut d, 14);
        d.unwrap_block();
        assert_eq!(d.text, "f();\n");
    }

    #[test]
    fn surround_with_if_should_indent_the_line() {
        let mut d = doc("f();");
        d.surround_with(Surround::If);
        assert_eq!(d.text, "if () {\n    f();\n}");
    }

    #[test]
    fn jump_to_block_edge_should_reach_the_closing_brace() {
        let mut d = doc("{\n  a();\n}");
        at(&mut d, 4);
        d.jump_to_block_edge(true);
        assert_eq!(d.offset(d.cursor()), 9);
    }

    #[test]
    fn occurrences_in_should_match_whole_words_only() {
        let d = doc("<?php $a = $ab + $a; // $a");
        assert_eq!(d.occurrences_in("$a", 0, d.text.len()), [6, 17]);
    }

    #[test]
    fn enclosing_headers_should_list_outer_blocks_first() {
        let d = doc("class A\n{\n    function f() {\n        x();\n    }\n}");
        assert_eq!(d.enclosing_headers(3, 3), [0, 2]);
    }

    #[test]
    fn start_new_line_should_indent_after_a_brace() {
        let mut d = doc("if (1) {");
        d.start_new_line(false);
        assert_eq!(d.text, "if (1) {\n    ");
    }
}
