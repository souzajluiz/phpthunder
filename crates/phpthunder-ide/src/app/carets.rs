//! Multiple carets in the editor: routing edits to every caret, adding
//! carets (⌥⌘↑/↓, ⌥-click, ⌃G next occurrence) and leaving the mode.

use super::*;
use crate::editor::MultiEdit;
use iced::Task;
use iced::widget::text_editor::{Action, Edit};

impl App {
    /// Handles `message` for a document with several carets, returning
    /// `None` when it is not an edit that needs every caret (the normal
    /// handler then runs). Typing, paste, backspace, delete and Enter go to
    /// every caret; arrows and Home/End move them all; a plain click or
    /// selection drag returns to one caret.
    pub(crate) fn multi_caret_message(&mut self, message: &Message) -> Option<Task<Message>> {
        let alt = self.modifiers.alt();
        let doc = self.doc_mut().filter(|d| d.has_extra_carets())?;
        let edit = match message {
            Message::TypeChar(c) => MultiEdit::Insert(c.to_string()),
            Message::Backspace => MultiEdit::Backspace,
            Message::SmartEnter => MultiEdit::Insert("\n".into()),
            Message::Indent(false) => MultiEdit::Insert("    ".into()),
            Message::Edit(Action::Edit(Edit::Insert(c))) => MultiEdit::Insert(c.to_string()),
            Message::Edit(Action::Edit(Edit::Enter)) => MultiEdit::Insert("\n".into()),
            Message::Edit(Action::Edit(Edit::Backspace)) => MultiEdit::Backspace,
            Message::Edit(Action::Edit(Edit::Delete)) => MultiEdit::Delete,
            Message::Edit(Action::Edit(Edit::Paste(text))) => {
                let lines: Vec<String> = text.lines().map(str::to_string).collect();
                if lines.len() > 1 {
                    MultiEdit::InsertEach(lines)
                } else {
                    MultiEdit::Insert(text.to_string())
                }
            }
            Message::Edit(Action::Move(motion)) => {
                doc.move_extra_carets(*motion);
                return None;
            }
            Message::Edit(
                Action::Click(_)
                | Action::Select(_)
                | Action::SelectWord
                | Action::SelectLine
                | Action::SelectAll,
            ) => {
                if !alt {
                    doc.clear_extra_carets();
                }
                return None;
            }
            _ => return None,
        };
        doc.multi_edit(edit);
        self.after_change(true);
        self.completion = None;
        Some(self.reveal_cursor())
    }

    /// Adds a caret where the text editor was ⌥-clicked (`point` relative
    /// to the editor widget); `false` when the click is not one.
    pub(crate) fn alt_click_caret(&mut self, action: &Action) -> bool {
        let Action::Click(point) = action else {
            return false;
        };
        if !self.modifiers.alt() {
            return false;
        }
        let Some(layout) = self.doc().map(|d| self.wrap_layout(d)) else {
            return false;
        };
        let row = ((point.y - crate::ui::EDITOR_PAD_TOP) / crate::ui::CODE_LINE)
            .floor()
            .max(0.0) as usize;
        let col = ((point.x - crate::ui::EDITOR_PAD_LEFT) / crate::ui::CHAR_WIDTH)
            .round()
            .max(0.0) as usize;
        let (line, base) = layout.line_at_row(row);
        let Some(doc) = self.doc_mut() else {
            return false;
        };
        let position = doc.position_at_visual(line, base + col);
        let offset = doc.offset(position);
        doc.add_caret_at(offset);
        true
    }

    /// Adds a caret above or below (`⌥⌘↑`/`⌥⌘↓`).
    pub(crate) fn add_caret_vertical(&mut self, up: bool) -> Task<Message> {
        if let Some(doc) = self.doc_mut() {
            doc.add_caret_vertical(up);
        }
        self.reveal_cursor()
    }

    /// Selects the next occurrence of the selection as another caret (`⌃G`).
    pub(crate) fn select_next_occurrence(&mut self) -> Task<Message> {
        if let Some(doc) = self.doc_mut() {
            doc.select_next_occurrence();
        }
        self.reveal_cursor()
    }
}
