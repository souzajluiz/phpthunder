//! Multiple carets: extra carets (and selections) beside the editor's own,
//! edited together as one undo step. The text editor widget has a single
//! cursor, so extra carets are kept by the document and applied here.

use super::*;
use iced::widget::text_editor::{Motion, Position};

/// An edit applied at every caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultiEdit {
    /// Insert this text (replacing selections).
    Insert(String),
    /// Insert one text per caret, in document order (multi-line paste
    /// with one line per caret).
    InsertEach(Vec<String>),
    /// Delete the selection or the character before each caret.
    Backspace,
    /// Delete the selection or the character after each caret.
    Delete,
}

/// Replaces the ranges (sorted, non-overlapping) with `texts`; returns the
/// new text and the caret offset after each replacement.
fn splice(text: &str, ranges: &[(usize, usize)], texts: &[String]) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut carets = Vec::with_capacity(ranges.len());
    let mut at = 0;
    for ((start, end), insert) in ranges.iter().zip(texts) {
        out.push_str(&text[at..*start]);
        out.push_str(insert);
        carets.push(out.len());
        at = *end;
    }
    out.push_str(&text[at..]);
    (out, carets)
}

/// Byte offset of the character boundary before `offset`.
fn prev_boundary(text: &str, offset: usize) -> usize {
    text[..offset]
        .char_indices()
        .next_back()
        .map_or(0, |(i, _)| i)
}

/// Byte offset of the character boundary after `offset`.
fn next_boundary(text: &str, offset: usize) -> usize {
    text[offset..]
        .chars()
        .next()
        .map_or(offset, |c| offset + c.len_utf8())
}

impl Document {
    /// Whether there are carets besides the main one.
    pub fn has_extra_carets(&self) -> bool {
        !self.extra_carets.is_empty()
    }

    /// Drops every extra caret.
    pub fn clear_extra_carets(&mut self) {
        self.extra_carets.clear();
    }

    /// The main caret as `(anchor, head)`.
    fn main_caret(&self) -> (usize, usize) {
        let head = self.offset(self.cursor());
        match self.selection() {
            Some((start, end)) if head == start => (end, start),
            Some((start, end)) => (start, end),
            None => (head, head),
        }
    }

    /// Every caret, the main one included, as ordered `(start, end)` ranges
    /// with overlaps merged.
    pub fn caret_ranges(&self) -> Vec<(usize, usize)> {
        let mut ranges: Vec<(usize, usize)> = std::iter::once(self.main_caret())
            .chain(self.extra_carets.iter().copied())
            .map(|(a, b)| (a.min(b), a.max(b)))
            .collect();
        ranges.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (s, e) in ranges {
            match merged.last_mut() {
                Some(last) if s <= last.1 && (s < last.1 || s == last.0) => last.1 = last.1.max(e),
                _ => merged.push((s, e)),
            }
        }
        merged
    }

    /// Adds a caret at `offset`, keeping the current one.
    pub fn add_caret_at(&mut self, offset: usize) {
        let main = self.main_caret();
        if main.1 != offset && !self.extra_carets.iter().any(|c| c.1 == offset) {
            self.extra_carets.push(main);
            let position = self.position_of(offset);
            self.move_to(position);
        }
    }

    /// Adds a caret on the line above (or below) the topmost (or
    /// bottommost) caret, at the same visual column.
    pub fn add_caret_vertical(&mut self, up: bool) {
        let heads =
            std::iter::once(self.main_caret().1).chain(self.extra_carets.iter().map(|c| c.1));
        let edge = if up { heads.min() } else { heads.max() }.unwrap_or(0);
        let pos = self.position_of(edge);
        let col = self.visual_column(pos);
        let line = if up {
            match pos.line.checked_sub(1) {
                Some(l) => l,
                None => return,
            }
        } else if pos.line + 1 < self.line_count() {
            pos.line + 1
        } else {
            return;
        };
        let target = self.position_at_visual(line, col);
        let offset = self.offset(target);
        self.add_caret_at(offset);
    }

    /// Selects the next occurrence of the main selection (or the word at
    /// the caret when nothing is selected), keeping earlier selections as
    /// extra carets. Searches forward from the last caret and wraps around.
    pub fn select_next_occurrence(&mut self) {
        let (anchor, head) = self.main_caret();
        if anchor == head {
            let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
            let start = self.text[..head]
                .rfind(|c: char| !is_word(c))
                .map_or(0, |i| i + 1);
            let end = self.text[head..]
                .find(|c: char| !is_word(c))
                .map_or(self.text.len(), |i| head + i);
            if start < end {
                self.select(start, end);
            }
            return;
        }
        let (start, end) = (anchor.min(head), anchor.max(head));
        let needle = self.text[start..end].to_string();
        let taken = self.caret_ranges();
        let from = taken.last().map_or(end, |r| r.1);
        let found = self.text[from..]
            .find(&needle)
            .map(|i| from + i)
            .or_else(|| self.text.find(&needle))
            .filter(|s| !taken.iter().any(|r| r.0 == *s));
        if let Some(s) = found {
            self.extra_carets.push((start, end));
            self.select(s, s + needle.len());
        }
    }

    /// Selects every occurrence of the main selection (the word at the
    /// caret first, when nothing is selected), one caret each.
    pub fn select_all_occurrences(&mut self) {
        if self.selection().is_none() {
            self.select_next_occurrence();
        }
        // Each call adds one caret; stop when a pass adds none.
        for _ in 0..10_000 {
            let before = self.extra_carets.len();
            self.select_next_occurrence();
            if self.extra_carets.len() == before {
                break;
            }
        }
    }

    /// Applies `edit` at every caret as one undo step; afterwards each
    /// caret sits after its edit and the last one is the main caret.
    pub fn multi_edit(&mut self, edit: MultiEdit) {
        let ranges = self.caret_ranges();
        let text = self.text.clone();
        let (ranges, texts): (Vec<(usize, usize)>, Vec<String>) = match edit {
            MultiEdit::Insert(s) => (ranges.clone(), vec![s; ranges.len()]),
            MultiEdit::InsertEach(each) if each.len() == ranges.len() => (ranges, each),
            MultiEdit::InsertEach(each) => {
                let joined = each.join("\n");
                (ranges.clone(), vec![joined; ranges.len()])
            }
            MultiEdit::Backspace => (
                ranges
                    .iter()
                    .map(|&(s, e)| {
                        if s == e {
                            (prev_boundary(&text, s), e)
                        } else {
                            (s, e)
                        }
                    })
                    .collect(),
                vec![String::new(); ranges.len()],
            ),
            MultiEdit::Delete => (
                ranges
                    .iter()
                    .map(|&(s, e)| {
                        if s == e {
                            (s, next_boundary(&text, e))
                        } else {
                            (s, e)
                        }
                    })
                    .collect(),
                vec![String::new(); ranges.len()],
            ),
        };
        // Deleting across carets can make ranges touch; keep them ordered.
        let mut ranges = ranges;
        for i in 1..ranges.len() {
            if ranges[i].0 < ranges[i - 1].1 {
                ranges[i].0 = ranges[i - 1].1;
            }
        }
        let (new_text, carets) = splice(&text, &ranges, &texts);
        let main = *carets.last().unwrap_or(&0);
        self.checkpoint();
        let position = {
            let line = new_text[..main].matches('\n').count();
            let line_start = new_text[..main].rfind('\n').map_or(0, |i| i + 1);
            Position {
                line,
                column: main - line_start,
            }
        };
        self.set_text(new_text, position);
        self.extra_carets = carets[..carets.len().saturating_sub(1)]
            .iter()
            .map(|&c| (c, c))
            .collect();
    }

    /// Moves every extra caret like the main caret moves for `motion`
    /// (left, right, up, down, line start/end); other motions drop them.
    pub fn move_extra_carets(&mut self, motion: Motion) {
        let moved: Option<Vec<(usize, usize)>> = self
            .extra_carets
            .iter()
            .map(|&(_, head)| {
                let pos = self.position_of(head);
                let to = match motion {
                    Motion::Left => prev_boundary(&self.text, head),
                    Motion::Right => next_boundary(&self.text, head),
                    Motion::Home => self.line_start(pos.line),
                    Motion::End => self.line_end(pos.line),
                    Motion::Up | Motion::Down => {
                        let col = self.visual_column(pos);
                        let line = if motion == Motion::Up {
                            pos.line.checked_sub(1)?
                        } else {
                            pos.line + 1
                        };
                        if line >= self.line_count() {
                            return None;
                        }
                        self.offset(self.position_at_visual(line, col))
                    }
                    _ => return None,
                };
                Some((to, to))
            })
            .collect();
        match moved {
            Some(carets) => self.extra_carets = carets,
            None => self.extra_carets.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn doc(text: &str) -> Document {
        Document::new(PathBuf::from("/t.php"), text.to_string())
    }

    #[test]
    fn typing_should_go_to_every_caret_as_one_undo_step() {
        let mut d = doc("a\nb\nc\n");
        d.move_to(Position { line: 0, column: 1 });
        d.add_caret_vertical(false);
        d.add_caret_vertical(false);
        assert_eq!(d.caret_ranges().len(), 3);
        d.multi_edit(MultiEdit::Insert(";".into()));
        assert_eq!(d.text, "a;\nb;\nc;\n");
        d.multi_edit(MultiEdit::Backspace);
        assert_eq!(d.text, "a\nb\nc\n");
        d.undo();
        assert_eq!(d.text, "a;\nb;\nc;\n");
    }

    #[test]
    fn select_next_occurrence_should_add_selections() {
        let mut d = doc("foo bar foo baz foo");
        d.move_to(Position { line: 0, column: 1 });
        d.select_next_occurrence();
        d.select_next_occurrence();
        d.select_next_occurrence();
        assert_eq!(d.caret_ranges(), vec![(0, 3), (8, 11), (16, 19)]);
        d.multi_edit(MultiEdit::Insert("x".into()));
        assert_eq!(d.text, "x bar x baz x");
    }

    #[test]
    fn paste_should_spread_lines_over_carets() {
        let mut d = doc("a\nb\n");
        d.move_to(Position { line: 0, column: 1 });
        d.add_caret_vertical(false);
        d.multi_edit(MultiEdit::InsertEach(vec!["1".into(), "2".into()]));
        assert_eq!(d.text, "a1\nb2\n");
    }
}
