//! An open file: editor content, cached text, highlighting, cursor math.

use crate::highlight::{self, LineSpans};
use iced::widget::text_editor::{Action, Content, Cursor, Edit, Position};
use phpthunder::text::Pos;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Document {
    pub path: PathBuf,
    pub content: Content,
    /// Mirror of `content` as one string, refreshed after every edit.
    pub text: String,
    pub dirty: bool,
    pub highlights: Arc<LineSpans>,
    pub highlight_version: u64,
    /// Vertical scroll offset of the editor, restored when switching tabs.
    pub scroll_y: f32,
}

impl Document {
    pub fn new(path: PathBuf, text: String) -> Self {
        let mut doc = Self {
            path,
            content: Content::with_text(&text),
            text,
            dirty: false,
            highlights: Arc::default(),
            highlight_version: 0,
            scroll_y: 0.0,
        };
        doc.refresh_highlights();
        doc
    }

    pub fn title(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    pub fn is_php(&self) -> bool {
        self.path.extension().is_some_and(|e| e == "php")
    }

    fn refresh_highlights(&mut self) {
        self.highlights = Arc::new(if self.is_php() { highlight::compute(&self.text) } else { Vec::new() });
        self.highlight_version += 1;
    }

    pub fn highlight_settings(&self) -> highlight::Settings {
        highlight::Settings { version: self.highlight_version, lines: self.highlights.clone() }
    }

    /// Applies an editor action; returns whether the text changed.
    pub fn perform(&mut self, action: Action) -> bool {
        let is_edit = action.is_edit();
        self.content.perform(action);
        if is_edit {
            self.sync_text();
        }
        is_edit
    }

    fn sync_text(&mut self) {
        self.text = self.content.text();
        self.dirty = true;
        self.refresh_highlights();
    }

    pub fn cursor(&self) -> Position {
        self.content.cursor().position
    }

    fn line_start(&self, line: usize) -> usize {
        if line == 0 {
            return 0;
        }
        self.text.match_indices('\n').nth(line - 1).map(|(i, _)| i + 1).unwrap_or(self.text.len())
    }

    fn line_text(&self, line: usize) -> &str {
        let start = self.line_start(line);
        let end = self.text[start..].find('\n').map(|i| start + i).unwrap_or(self.text.len());
        self.text[start..end].trim_end_matches('\r')
    }

    /// Byte offset of an editor position (columns are byte indices in the line).
    pub fn offset(&self, pos: Position) -> usize {
        let start = self.line_start(pos.line);
        (start + pos.column).min(self.text.len())
    }

    pub fn position_of(&self, offset: usize) -> Position {
        let offset = offset.min(self.text.len());
        let line = self.text[..offset].matches('\n').count();
        Position { line, column: offset - self.line_start(line) }
    }

    /// Visual column (characters) of an editor position, for pixel math.
    pub fn visual_column(&self, pos: Position) -> usize {
        let line = self.line_text(pos.line);
        line.get(..pos.column.min(line.len())).map(|s| s.chars().count()).unwrap_or(0)
    }

    /// Editor position from a visual (line, character column).
    pub fn position_at_visual(&self, line: usize, col: usize) -> Position {
        let text = self.line_text(line);
        let column = text.char_indices().nth(col).map(|(i, _)| i).unwrap_or(text.len());
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
        Position { line: pos.line as usize, column }
    }

    pub fn line_count(&self) -> usize {
        self.content.line_count()
    }

    pub fn move_to(&mut self, position: Position) {
        self.content.move_to(Cursor { position, selection: None });
    }

    /// Replaces the byte range with `with` and leaves the cursor after it.
    pub fn replace(&mut self, start: usize, end: usize, with: &str) {
        let anchor = self.position_of(start);
        let head = self.position_of(end);
        if start == end {
            self.content.move_to(Cursor { position: anchor, selection: None });
        } else {
            self.content.move_to(Cursor { position: head, selection: Some(anchor) });
        }
        self.content.perform(Action::Edit(Edit::Paste(Arc::new(with.to_string()))));
        self.sync_text();
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }
}
