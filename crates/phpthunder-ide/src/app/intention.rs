//! The `⌥↵` menu: quick fixes for problems at the caret plus context
//! actions, and applying their edits across files.

use super::*;
use crate::tr;
use iced::Task;
use phpthunder::inspection::{Edit, Fix};
use std::collections::BTreeMap;
use std::path::PathBuf;

impl App {
    /// Opens the `⌥↵` menu at the caret: fixes for diagnostics under it,
    /// context actions (see `phpthunder::intention`), "Implement methods"
    /// inside a class that lacks implementations, and "Optimize imports".
    /// Shows a notice when nothing applies.
    pub(crate) fn open_intentions(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let blade = doc.file_type == crate::filetype::FileType::Blade;
        if blade {
            // Templates get the Laravel actions only.
            let items: Vec<Intention> =
                phpthunder::laravel::laravel_actions(&doc.text, offset, true)
                    .into_iter()
                    .map(Intention::Fix)
                    .collect();
            return self.show_intentions(items);
        }
        if !doc.is_php() {
            return Task::none();
        }
        let open = self.open_texts();
        let mut items: Vec<Intention> = self
            .diagnostics_at(offset)
            .iter()
            .flat_map(|d| self.lang.quick_fixes(&doc.text, d, &open))
            .map(Intention::Fix)
            .collect();
        let (start, end) = doc.selection().unwrap_or((offset, offset));
        for action in self.lang.intentions(&doc.text, start, end) {
            // A quick fix may already make the same change.
            let duplicate = items
                .iter()
                .any(|i| matches!(i, Intention::Fix(f) if f.edits == action.edits));
            if !duplicate {
                items.push(Intention::Fix(action));
            }
        }
        if let Some(class) = phpthunder::refactor::enclosing_class_fqn(&doc.text, offset)
            && !self.lang.missing_implementations(&class).is_empty()
        {
            items.push(Intention::ImplementMethods { class });
        }
        items.extend(
            phpthunder::laravel::laravel_actions(&doc.text, offset, false)
                .into_iter()
                .map(Intention::Fix),
        );
        items.extend(self.refactor_intentions());
        items.push(Intention::OptimizeImports);
        self.show_intentions(items)
    }

    /// Opens the `⌥↵` menu with `items` at the caret; a notice instead when
    /// there are none.
    fn show_intentions(&mut self, items: Vec<Intention>) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        if items.is_empty() {
            self.notice = Some(tr!("notice-no-intentions"));
            return Task::none();
        }
        let pos = doc.cursor();
        let (line, col) = (pos.line, doc.visual_column(pos));
        self.completion = None;
        self.hover = None;
        self.intentions = Some(Intentions {
            items,
            selected: 0,
            line,
            col,
        });
        Task::none()
    }

    /// Applies the highlighted menu entry and closes the menu.
    pub(crate) fn apply_selected_intention(&mut self) -> Task<Message> {
        let Some(menu) = self.intentions.take() else {
            return Task::none();
        };
        let Some(item) = menu.items.into_iter().nth(menu.selected) else {
            return Task::none();
        };
        match item {
            Intention::Fix(fix) => self.apply_fix(fix),
            Intention::ImplementMethods { class } => self.implement_methods(&class),
            Intention::OptimizeImports => self.optimize_imports(),
            Intention::Refactor(action) => self.start_refactor(action),
            Intention::InlineVariable => self.inline_variable(),
            Intention::Member(kind) => self.member_refactoring(kind),
        }
    }

    /// Applies a fix whose edits may touch other files.
    pub(crate) fn apply_fix(&mut self, fix: Fix) -> Task<Message> {
        let current = self.doc().map(|d| d.path.clone());
        let mut by_file: BTreeMap<Option<PathBuf>, Vec<Edit>> = BTreeMap::new();
        for edit in fix.edits {
            let path = edit.path.clone().filter(|p| Some(p) != current.as_ref());
            by_file.entry(path).or_default().push(edit);
        }
        for (path, edits) in by_file {
            match path {
                None => {
                    if let Some(doc) = self.doc_mut() {
                        doc.apply_edits(&edits);
                    }
                    self.after_change(true);
                }
                Some(path) => self.edit_other_file(&path, &edits),
            }
        }
        self.reveal_cursor()
    }

    /// Applies edits to a file that is not the active one: through its open
    /// tab when there is one (so they can be undone), else on disk.
    pub(crate) fn edit_other_file(&mut self, path: &std::path::Path, edits: &[Edit]) {
        if let Some(doc) = self.docs.iter_mut().find(|d| d.path == path) {
            doc.apply_edits(edits);
            let (p, text) = (doc.path.clone(), doc.text.clone());
            self.lang.update_file(&p, &text);
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            self.notice = Some(tr!("notice-read-failed", path = path.display().to_string()));
            return;
        };
        let updated = phpthunder::refactor::apply_edits(&text, edits);
        match std::fs::write(path, &updated) {
            Ok(()) => self.lang.update_file(path, &updated),
            Err(e) => self.fail(tr!(
                "notice-write-failed",
                path = path.display().to_string(),
                error = e.to_string()
            )),
        }
    }

    /// Inserts stubs for every method `class` must implement, before the
    /// closing brace of the class body in the active document.
    pub(crate) fn implement_methods(&mut self, class: &str) -> Task<Message> {
        let missing = self.lang.missing_implementations(class);
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let Some(close) = class_body_end(&doc.text, doc.offset(doc.cursor())) else {
            return Task::none();
        };
        let text: String = missing
            .iter()
            .map(phpthunder::refactor::implementation_stub)
            .collect();
        let edit = Edit {
            path: None,
            start: close,
            end: close,
            text,
        };
        if let Some(doc) = self.doc_mut() {
            doc.apply_edits(&[edit]);
        }
        self.after_change(true);
        self.notice = Some(tr!("notice-implemented", count = missing.len()));
        self.reveal_cursor()
    }

    /// Removes unused imports and sorts the import blocks of the active file.
    pub(crate) fn optimize_imports(&mut self) -> Task<Message> {
        let Some(doc) = self.doc().filter(|d| d.is_php()) else {
            return Task::none();
        };
        let optimized = self.lang.optimize_imports(&doc.text);
        if optimized == doc.text {
            self.notice = Some(tr!("notice-imports-optimized"));
            return Task::none();
        }
        let cursor = doc.cursor();
        if let Some(doc) = self.doc_mut() {
            doc.set_text(optimized, cursor);
        }
        self.after_change(true);
        Task::none()
    }
}

/// Byte offset of the closing `}` of the class body around `offset`.
fn class_body_end(text: &str, offset: usize) -> Option<usize> {
    let tree = phpthunder::syntax::parse(text);
    let mut node = tree.root_node().descendant_for_byte_range(offset, offset)?;
    loop {
        if phpthunder::syntax::is_class_like(node.kind()) {
            let body = node.child_by_field_name("body")?;
            return body.end_byte().checked_sub(1);
        }
        node = node.parent()?;
    }
}
