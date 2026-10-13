//! An open file: editor content, cached text, highlighting, undo history,
//! cursor math, and line-oriented editing operations.

use crate::highlight::{self, LineSpans};
use iced::widget::text_editor::{Action, Content, Cursor, Edit, Motion, Position};
use phpthunder::text::Pos;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const UNDO_LIMIT: usize = 300;
const TYPING_GROUP: Duration = Duration::from_millis(1200);
pub const INDENT: &str = "    ";

#[derive(Clone)]
struct Snapshot {
    text: String,
    cursor: Cursor,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Typing,
    Deleting,
    Other,
}

pub struct Document {
    pub path: PathBuf,
    pub content: Content,
    /// Mirror of `content` as one string, refreshed after every edit.
    pub text: String,
    saved_text: String,
    pub dirty: bool,
    pub highlights: Arc<LineSpans>,
    pub highlight_version: u64,
    /// Vertical scroll offset of the editor, restored when switching tabs.
    pub scroll_y: f32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: Option<(EditKind, Instant)>,
}

fn leading_ws(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

impl Document {
    pub fn new(path: PathBuf, text: String) -> Self {
        let mut doc = Self {
            path,
            content: Content::with_text(&text),
            saved_text: text.clone(),
            text,
            dirty: false,
            highlights: Arc::default(),
            highlight_version: 0,
            scroll_y: 0.0,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
        };
        doc.refresh_highlights();
        doc
    }

    pub fn title(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    pub fn is_php(&self) -> bool {
        self.path.extension().is_some_and(|e| e == "php")
    }

    fn refresh_highlights(&mut self) {
        self.highlights = Arc::new(if self.is_php() {
            highlight::compute(&self.text)
        } else {
            Vec::new()
        });
        self.highlight_version += 1;
    }

    pub fn highlight_settings(&self) -> highlight::Settings {
        highlight::Settings {
            version: self.highlight_version,
            lines: self.highlights.clone(),
        }
    }

    // ----- history -------------------------------------------------------

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            cursor: self.content.cursor(),
        }
    }

    /// Records the current state as an undo step (call before a change).
    pub fn checkpoint(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.last_edit = None;
    }

    fn record(&mut self, kind: EditKind) {
        let now = Instant::now();
        let continues = matches!(self.last_edit, Some((k, at)) if k == kind && kind != EditKind::Other && now - at < TYPING_GROUP);
        if !continues {
            self.checkpoint();
        }
        self.last_edit = Some((kind, now));
    }

    fn restore(&mut self, snap: Snapshot) {
        self.content = Content::with_text(&snap.text);
        self.text = snap.text;
        self.set_cursor(snap.cursor);
        self.after_change();
    }

    pub fn undo(&mut self) -> bool {
        let Some(snap) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(snap);
        self.last_edit = None;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(snap) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(snap);
        self.last_edit = None;
        true
    }

    // ----- editing -------------------------------------------------------

    /// Applies an editor action; returns whether the text changed.
    pub fn perform(&mut self, action: Action) -> bool {
        let kind = match &action {
            Action::Edit(Edit::Insert(c)) if !c.is_whitespace() => Some(EditKind::Typing),
            Action::Edit(Edit::Backspace | Edit::Delete) => Some(EditKind::Deleting),
            Action::Edit(_) => Some(EditKind::Other),
            _ => None,
        };
        if let Some(kind) = kind {
            self.record(kind);
        } else {
            self.last_edit = None;
        }
        self.content.perform(action);
        if kind.is_some() {
            self.sync_text();
        }
        kind.is_some()
    }

    fn after_change(&mut self) {
        self.dirty = self.text != self.saved_text;
        self.refresh_highlights();
    }

    fn sync_text(&mut self) {
        self.text = self.content.text();
        self.after_change();
    }

    /// Replaces the whole text in one undo step, then places the cursor.
    fn rewrite(&mut self, text: String, cursor: Cursor) {
        self.checkpoint();
        self.content = Content::with_text(&text);
        self.text = text;
        self.set_cursor(cursor);
        self.after_change();
    }

    /// Replaces the byte range with `with` and leaves the cursor after it.
    /// Several calls between two `checkpoint`s undo together.
    pub fn replace(&mut self, start: usize, end: usize, with: &str) {
        let anchor = self.position_of(start);
        let head = self.position_of(end);
        if start == end {
            self.set_cursor(Cursor {
                position: anchor,
                selection: None,
            });
        } else {
            self.content.move_to(Cursor {
                position: head,
                selection: Some(anchor),
            });
        }
        self.content
            .perform(Action::Edit(Edit::Paste(Arc::new(with.to_string()))));
        self.sync_text();
    }

    /// Replaces the whole text as one undo step, keeping the cursor nearby.
    pub fn set_text(&mut self, text: String, cursor: Position) {
        let line = cursor.line.min(text.matches('\n').count());
        self.rewrite(
            text,
            Cursor {
                position: Position { line, column: 0 },
                selection: None,
            },
        );
        let column = cursor.column.min(self.line_text(line).len());
        self.move_to(Position { line, column });
    }

    /// LSP-style position (UTF-16 column) of an editor position.
    pub fn utf16_pos(&self, pos: Position) -> Pos {
        let line = self.line_text(pos.line);
        let col = line
            .get(..pos.column.min(line.len()))
            .map(|s| s.encode_utf16().count())
            .unwrap_or(0);
        Pos {
            line: pos.line as u32,
            col: col as u32,
        }
    }

    pub fn mark_saved(&mut self) {
        self.saved_text = self.text.clone();
        self.dirty = false;
    }

    // ----- positions -----------------------------------------------------

    pub fn cursor(&self) -> Position {
        self.content.cursor().position
    }

    /// Selected byte range (start <= end), if any.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let c = self.content.cursor();
        let anchor = c.selection?;
        let (a, b) = (self.offset(anchor), self.offset(c.position));
        (a != b).then(|| (a.min(b), a.max(b)))
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.selection().map(|(a, b)| &self.text[a..b])
    }

    pub fn line_start(&self, line: usize) -> usize {
        if line == 0 {
            return 0;
        }
        self.text
            .match_indices('\n')
            .nth(line - 1)
            .map(|(i, _)| i + 1)
            .unwrap_or(self.text.len())
    }

    /// End of the line's text (before `\r\n` / `\n`).
    pub fn line_end(&self, line: usize) -> usize {
        let start = self.line_start(line);
        let end = self.text[start..]
            .find('\n')
            .map(|i| start + i)
            .unwrap_or(self.text.len());
        if end > start && self.text.as_bytes()[end - 1] == b'\r' {
            end - 1
        } else {
            end
        }
    }

    pub fn line_text(&self, line: usize) -> &str {
        &self.text[self.line_start(line)..self.line_end(line)]
    }

    /// Byte offset of an editor position (columns are byte indices in the line).
    pub fn offset(&self, pos: Position) -> usize {
        let start = self.line_start(pos.line);
        (start + pos.column).min(self.line_end(pos.line))
    }

    pub fn position_of(&self, offset: usize) -> Position {
        let offset = offset.min(self.text.len());
        let line = self.text[..offset].matches('\n').count();
        Position {
            line,
            column: offset - self.line_start(line),
        }
    }

    /// Visual column (characters) of an editor position, for pixel math.
    pub fn visual_column(&self, pos: Position) -> usize {
        let line = self.line_text(pos.line);
        line.get(..pos.column.min(line.len()))
            .map(|s| s.chars().count())
            .unwrap_or(0)
    }

    /// Editor position from a visual (line, character column).
    pub fn position_at_visual(&self, line: usize, col: usize) -> Position {
        let text = self.line_text(line);
        let column = text
            .char_indices()
            .nth(col)
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        Position { line, column }
    }

    /// Converts an LSP-style position (UTF-16 column) to an editor position.
    pub fn position_from_utf16(&self, pos: Pos) -> Position {
        let text = self.line_text(pos.line as usize);
        let mut units = 0u32;
        let mut column = text.len();
        for (i, ch) in text.char_indices() {
            if units >= pos.col {
                column = i;
                break;
            }
            units += ch.len_utf16() as u32;
        }
        Position {
            line: pos.line as usize,
            column,
        }
    }

    pub fn line_count(&self) -> usize {
        self.content.line_count()
    }

    pub fn move_to(&mut self, position: Position) {
        self.set_cursor(Cursor {
            position,
            selection: None,
        });
        self.last_edit = None;
    }

    /// iced keeps the previous selection when moving without one; an empty
    /// selection at the cursor clears it.
    fn set_cursor(&mut self, cursor: Cursor) {
        let p = cursor.position;
        if cursor.selection.is_some_and(|s| s != p) {
            self.content.move_to(cursor);
            return;
        }
        // Select from a distinct point to `p`, then collapse with a motion:
        // iced clears the selection and leaves the cursor at that end.
        let origin = Position { line: 0, column: 0 };
        if p != origin {
            self.content.move_to(Cursor {
                position: p,
                selection: Some(origin),
            });
            self.content.perform(Action::Move(Motion::Right));
        } else if !self.text.is_empty() {
            let end = self.position_of(self.text.len());
            self.content.move_to(Cursor {
                position: p,
                selection: Some(end),
            });
            self.content.perform(Action::Move(Motion::Left));
        } else {
            self.content.move_to(Cursor {
                position: p,
                selection: None,
            });
        }
    }

    pub fn select(&mut self, start: usize, end: usize) {
        let (anchor, head) = (self.position_of(start), self.position_of(end));
        self.content.move_to(Cursor {
            position: head,
            selection: Some(anchor),
        });
        self.last_edit = None;
    }

    fn char_after(&self, offset: usize) -> Option<char> {
        self.text[offset..].chars().next()
    }

    fn char_before(&self, offset: usize) -> Option<char> {
        self.text[..offset].chars().next_back()
    }

    /// Lines touched by the cursor or selection (inclusive).
    fn selected_lines(&self) -> (usize, usize) {
        let c = self.content.cursor();
        let a = c.position.line;
        let b = c.selection.map(|s| s.line).unwrap_or(a);
        let (lo, mut hi) = (a.min(b), a.max(b));
        // A selection ending at column 0 does not include that line.
        if let Some(sel) = c.selection {
            let end = if c.position.line > sel.line {
                c.position
            } else {
                sel
            };
            if end.column == 0 && hi > lo {
                hi -= 1;
            }
        }
        (lo, hi)
    }

    /// Range covering whole lines `lo..=hi`, including the final newline if any.
    fn lines_range(&self, lo: usize, hi: usize) -> (usize, usize) {
        let start = self.line_start(lo);
        let end = self.line_start(hi + 1);
        let end = if hi + 1 >= self.line_count() {
            self.text.len()
        } else {
            end
        };
        (start, end)
    }

    // ----- line operations ----------------------------------------------

    /// `⌘/`: comments or uncomments the touched lines with `//`.
    pub fn toggle_comment(&mut self) {
        let (lo, hi) = self.selected_lines();
        let lines: Vec<&str> = (lo..=hi).map(|l| self.line_text(l)).collect();
        let all_commented = lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .all(|l| l.trim_start().starts_with("//"));
        let indent = lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| leading_ws(l).len())
            .min()
            .unwrap_or(0);
        let mut text = self.text.clone();
        let mut delta_cursor: isize = 0;
        let cursor = self.content.cursor();
        for line in (lo..=hi).rev() {
            let start = self.line_start(line);
            let content = self.line_text(line);
            if content.trim().is_empty() {
                continue;
            }
            if all_commented {
                let ws = leading_ws(content).len();
                let rest = &content[ws..];
                let n = if rest.starts_with("// ") { 3 } else { 2 };
                text.replace_range(start + ws..start + ws + n, "");
                if line == cursor.position.line {
                    delta_cursor = -(n as isize);
                }
            } else {
                text.insert_str(start + indent, "// ");
                if line == cursor.position.line {
                    delta_cursor = 3;
                }
            }
        }
        let mut position = cursor.position;
        position.column = (position.column as isize + delta_cursor).max(0) as usize;
        self.rewrite(
            text,
            Cursor {
                position,
                selection: None,
            },
        );
    }

    /// `⌘D`: duplicates the selection, or the current line(s).
    pub fn duplicate(&mut self) {
        if let Some((a, b)) = self.selection() {
            let mut text = self.text.clone();
            let chunk = self.text[a..b].to_string();
            text.insert_str(b, &chunk);
            let position = self.position_of(b);
            self.rewrite(
                text,
                Cursor {
                    position,
                    selection: None,
                },
            );
            let (s, e) = (b, b + chunk.len());
            self.select(s, e);
            return;
        }
        let (lo, hi) = self.selected_lines();
        let (start, end) = self.lines_range(lo, hi);
        let mut chunk = self.text[start..end].to_string();
        if !chunk.ends_with('\n') {
            chunk.insert(0, '\n');
        }
        let mut text = self.text.clone();
        text.insert_str(end, &chunk);
        let mut position = self.cursor();
        position.line += hi - lo + 1;
        self.rewrite(
            text,
            Cursor {
                position,
                selection: None,
            },
        );
    }

    /// `⌘⌫`: deletes the current line(s).
    pub fn delete_lines(&mut self) {
        let (lo, hi) = self.selected_lines();
        let (mut start, end) = self.lines_range(lo, hi);
        // Last line: remove the newline before it instead.
        if end == self.text.len() && start > 0 && !self.text[start..end].ends_with('\n') {
            start -= 1;
        }
        let mut text = self.text.clone();
        text.replace_range(start..end, "");
        let line = lo.min(text.matches('\n').count());
        self.rewrite(
            text,
            Cursor {
                position: Position { line, column: 0 },
                selection: None,
            },
        );
    }

    /// `⌥⇧↑` / `⌥⇧↓`: moves the current line(s) up or down.
    pub fn move_lines(&mut self, down: bool) {
        let (lo, hi) = self.selected_lines();
        let last = self.line_count().saturating_sub(1);
        if (down && hi >= last) || (!down && lo == 0) {
            return;
        }
        let mut lines: Vec<String> = self.text.split('\n').map(str::to_string).collect();
        let block: Vec<String> = lines.drain(lo..=hi).collect();
        let at = if down { lo + 1 } else { lo - 1 };
        for (i, l) in block.into_iter().enumerate() {
            lines.insert(at + i, l);
        }
        let text = lines.join("\n");
        let mut cursor = self.content.cursor();
        let shift = |p: &mut Position| if down { p.line += 1 } else { p.line -= 1 };
        shift(&mut cursor.position);
        if let Some(s) = &mut cursor.selection {
            shift(s);
        }
        self.rewrite(text, cursor);
    }

    /// Enter with auto-indent; between `{}` / `()` / `[]` opens an indented block.
    pub fn smart_enter(&mut self) {
        let offset = self.offset(self.cursor());
        let line = self.line_text(self.cursor().line).to_string();
        let indent = leading_ws(&line).to_string();
        let before = self.char_before(offset).filter(|c| !c.is_whitespace());
        let after = self.char_after(offset);
        let opens = matches!(before, Some('{' | '(' | '['));
        let between = matches!(
            (before, after),
            (Some('{'), Some('}')) | (Some('('), Some(')')) | (Some('['), Some(']'))
        );
        self.checkpoint();
        let (start, end) = self.selection().unwrap_or((offset, offset));
        if between {
            let insert = format!("\n{indent}{INDENT}\n{indent}");
            self.replace(start, end, &insert);
            let pos = Position {
                line: self.cursor().line - 1,
                column: indent.len() + INDENT.len(),
            };
            self.move_to(pos);
        } else {
            let extra = if opens { INDENT } else { "" };
            self.replace(start, end, &format!("\n{indent}{extra}"));
        }
        self.last_edit = None;
    }

    /// Typing a bracket or quote: auto-close, skip over a closing char,
    /// or wrap the selection.
    pub fn type_pair_char(&mut self, c: char) {
        let close = match c {
            '(' => Some(')'),
            '[' => Some(']'),
            '{' => Some('}'),
            '"' => Some('"'),
            '\'' => Some('\''),
            _ => None,
        };
        let offset = self.offset(self.cursor());
        if let Some((a, b)) = self.selection()
            && let Some(close) = close
        {
            self.checkpoint();
            let wrapped = format!("{c}{}{close}", &self.text[a..b]);
            self.replace(a, b, &wrapped);
            self.select(a + c.len_utf8(), b + c.len_utf8());
            return;
        }
        let next = self.char_after(offset);
        let is_closer = matches!(c, ')' | ']' | '}' | '"' | '\'');
        if is_closer && next == Some(c) {
            self.content.perform(Action::Move(Motion::Right));
            self.last_edit = None;
            return;
        }
        let prev = self.char_before(offset);
        let auto_close = match close {
            Some(_) if matches!(c, '"' | '\'') => {
                // Not after a word character (`don't`) or inside a word.
                !prev.is_some_and(|p| p.is_alphanumeric() || p == '_' || p == '\\')
                    && next.is_none_or(|n| n.is_whitespace() || ")]};,".contains(n))
            }
            Some(_) => next.is_none_or(|n| n.is_whitespace() || ")]};,".contains(n)),
            None => false,
        };
        self.perform(Action::Edit(Edit::Insert(c)));
        if let Some(close) = close.filter(|_| auto_close) {
            self.content.perform(Action::Edit(Edit::Insert(close)));
            self.content.perform(Action::Move(Motion::Left));
            self.sync_text();
        }
    }

    /// Backspace between an empty pair removes both characters.
    pub fn backspace(&mut self) {
        let offset = self.offset(self.cursor());
        if self.selection().is_none() {
            let pair = (self.char_before(offset), self.char_after(offset));
            if matches!(
                pair,
                (Some('('), Some(')'))
                    | (Some('['), Some(']'))
                    | (Some('{'), Some('}'))
                    | (Some('"'), Some('"'))
                    | (Some('\''), Some('\''))
            ) {
                self.perform(Action::Edit(Edit::Delete));
            }
        }
        self.perform(Action::Edit(Edit::Backspace));
    }

    /// Typing `}` on a whitespace-only line dedents it.
    pub fn type_close_brace(&mut self) {
        let pos = self.cursor();
        let line = self.line_text(pos.line).to_string();
        let offset = self.offset(pos);
        if self.selection().is_none()
            && line.trim().is_empty()
            && line.len() >= INDENT.len()
            && self.char_after(offset) != Some('}')
        {
            let start = self.line_start(pos.line);
            self.checkpoint();
            self.replace(start + line.len() - INDENT.len(), start + line.len(), "");
        }
        self.type_pair_char('}');
    }

    /// Tab: indent the selected lines, or insert spaces to the next stop.
    pub fn indent(&mut self, outdent: bool) {
        let (lo, hi) = self.selected_lines();
        if self.selection().is_none() && !outdent {
            let col = self.visual_column(self.cursor());
            let n = INDENT.len() - col % INDENT.len();
            self.checkpoint();
            let offset = self.offset(self.cursor());
            self.replace(offset, offset, &" ".repeat(n));
            return;
        }
        let mut text = self.text.clone();
        for line in (lo..=hi).rev() {
            let start = self.line_start(line);
            if outdent {
                let ws = leading_ws(self.line_text(line)).len().min(INDENT.len());
                text.replace_range(start..start + ws, "");
            } else if !self.line_text(line).is_empty() {
                text.insert_str(start, INDENT);
            }
        }
        let cursor = self.content.cursor();
        self.rewrite(
            text,
            Cursor {
                position: Position {
                    line: cursor.position.line,
                    column: 0,
                },
                selection: None,
            },
        );
        // Keep the lines selected so Tab can be repeated.
        let (s, e) = (self.line_start(lo), self.line_end(hi));
        self.select(s, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> Document {
        Document::new(PathBuf::from("/t.php"), text.to_string())
    }

    #[test]
    fn comment_toggle_round_trips() {
        let mut d = doc("<?php\n    $a = 1;\n    $b = 2;\n");
        d.select(d.line_start(1), d.line_end(2));
        d.toggle_comment();
        assert_eq!(d.text, "<?php\n    // $a = 1;\n    // $b = 2;\n");
        d.select(d.line_start(1), d.line_end(2));
        d.toggle_comment();
        assert_eq!(d.text, "<?php\n    $a = 1;\n    $b = 2;\n");
        assert!(d.undo());
        assert_eq!(d.text, "<?php\n    // $a = 1;\n    // $b = 2;\n");
        d.move_to(Position { line: 2, column: 0 });
        d.toggle_comment();
        assert_eq!(d.text, "<?php\n    // $a = 1;\n    $b = 2;\n");
    }

    #[test]
    fn duplicate_move_delete_and_undo() {
        let mut d = doc("a\nb\nc");
        d.move_to(Position { line: 1, column: 0 });
        d.duplicate();
        assert_eq!(d.text, "a\nb\nb\nc");
        d.move_lines(true);
        assert_eq!(d.text, "a\nb\nc\nb");
        d.delete_lines();
        assert_eq!(d.text, "a\nb\nc");
        assert!(d.undo());
        assert!(d.undo());
        assert_eq!(d.text, "a\nb\nb\nc");
        assert!(d.redo());
        assert_eq!(d.text, "a\nb\nc\nb");
    }

    #[test]
    fn smart_enter_and_pairs() {
        let mut d = doc("<?php\nfunction f() ");
        d.move_to(Position {
            line: 1,
            column: 13,
        });
        d.type_pair_char('{');
        assert_eq!(d.text, "<?php\nfunction f() {}");
        d.smart_enter();
        assert_eq!(d.text, "<?php\nfunction f() {\n    \n}");
        assert_eq!(d.cursor(), Position { line: 2, column: 4 });
        d.type_pair_char('(');
        d.type_pair_char(')');
        assert_eq!(d.text, "<?php\nfunction f() {\n    ()\n}");
        assert_eq!(d.cursor(), Position { line: 2, column: 6 });
        d.move_to(Position { line: 2, column: 5 });
        d.backspace();
        assert_eq!(d.text, "<?php\nfunction f() {\n    \n}");
    }

    #[test]
    fn typing_groups_into_one_undo_step() {
        let mut d = doc("");
        for c in "hello".chars() {
            d.perform(Action::Edit(Edit::Insert(c)));
        }
        d.perform(Action::Edit(Edit::Insert(' ')));
        for c in "you".chars() {
            d.perform(Action::Edit(Edit::Insert(c)));
        }
        assert_eq!(d.text, "hello you");
        d.undo();
        assert_eq!(d.text, "hello ");
        d.undo();
        assert_eq!(d.text, "hello");
        d.undo();
        assert_eq!(d.text, "");
        assert!(!d.dirty);
    }
}
