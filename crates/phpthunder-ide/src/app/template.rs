//! Live templates in the editor: Tab expansion and the `⌘J` list.

use super::*;
use crate::palette::{Item, Target};
use crate::template::{self, Expansion, TEMPLATES};
use crate::tr;
use iced::Task;

impl App {
    /// Applies a template expansion to the active document as one undo
    /// step and moves the caret to the template's caret mark.
    fn apply_expansion(&mut self, e: Expansion) {
        let Some(doc) = self.doc_mut() else {
            return;
        };
        doc.checkpoint();
        doc.replace(e.start, e.end, &e.text);
        let position = doc.position_of(e.caret);
        doc.move_to(position);
        self.after_change(true);
        self.completion = None;
    }

    /// Tab in a PHP document without a selection: expands the template
    /// abbreviation before the caret. Returns false when there is none, so
    /// Tab indents as usual.
    pub(crate) fn expand_template(&mut self) -> bool {
        let Some(doc) = self.doc().filter(|d| d.is_php() && d.selection().is_none()) else {
            return false;
        };
        let Some(expansion) = template::expand_at(&doc.text, doc.offset(doc.cursor())) else {
            return false;
        };
        self.apply_expansion(expansion);
        true
    }

    /// Opens the `⌘J` list of live templates.
    pub(crate) fn open_templates(&mut self) -> Task<Message> {
        if self.doc().is_none_or(|d| !d.is_php()) {
            return Task::none();
        }
        let items = TEMPLATES
            .iter()
            .map(|t| Item {
                badge: "⇥",
                title: t.abbreviation.to_string(),
                matched: Vec::new(),
                detail: t.description(),
                target: Target::Template(t.abbreviation),
            })
            .collect();
        self.open_choice(tr!("templates-title"), items)
    }

    /// Inserts the template `abbreviation` at the caret (from the `⌘J` list).
    pub(crate) fn insert_template(&mut self, abbreviation: &str) -> Task<Message> {
        let (Some(t), Some(doc)) = (template::find_template(abbreviation), self.doc()) else {
            return Task::none();
        };
        let at = doc.offset(doc.cursor());
        let expansion = template::render(t, &doc.text, at, at);
        self.apply_expansion(expansion);
        self.reveal_cursor()
    }
}
