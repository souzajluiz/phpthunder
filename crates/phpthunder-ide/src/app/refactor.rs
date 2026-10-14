//! Parameter info while typing calls, and rename across the project.

use super::*;
use crate::tr;
use iced::Task;
use iced::widget::{Id, operation};
use phpthunder::refactor::RenamePlan;

/// Widget id of the rename dialog's input, focused when it opens.
pub const RENAME_INPUT: &str = "rename-input";

impl App {
    /// Recomputes parameter info for the caret. Typing `(` or `,` and every
    /// edit while the popup is open call it; `explicit` (`⌘P`) reports when
    /// the caret is not inside a known call.
    pub(crate) fn refresh_signature(&mut self, explicit: bool) {
        if self.doc().is_some_and(|d| d.is_php()) {
            // Computed in the background (see `schedule_signature`).
            self.signature_request = Some(explicit);
        }
    }

    /// Starts computing parameter info for a pending request on a
    /// blocking thread; the newest request wins. Called by `App::update`
    /// after every message.
    pub(crate) fn schedule_signature(&mut self) -> Task<Message> {
        let Some(explicit) = self.signature_request.take() else {
            return Task::none();
        };
        let Some((version, offset, text)) = self
            .doc()
            .map(|d| (d.version(), d.offset(d.cursor()), d.text.clone()))
        else {
            return Task::none();
        };
        self.signature_generation += 1;
        let generation = self.signature_generation;
        let lang = self.lang.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || lang.signature_help(&text, offset))
                    .await
                    .ok()
                    .flatten()
            },
            move |help| Message::SignatureReady {
                generation,
                version,
                offset,
                explicit,
                help,
            },
        )
    }

    /// Shows background parameter info if it is the latest asked for and
    /// the document and caret have not moved since.
    pub(crate) fn signature_ready(
        &mut self,
        generation: u64,
        version: u64,
        offset: usize,
        explicit: bool,
        help: Option<phpthunder::refactor::SignatureHelp>,
    ) {
        let Some(doc) = self
            .doc()
            .filter(|d| d.version() == version && d.offset(d.cursor()) == offset)
        else {
            return;
        };
        if generation != self.signature_generation {
            return;
        }
        match help {
            Some(help) => {
                let pos = doc.cursor();
                let (line, col) = (pos.line, doc.visual_column(pos));
                self.signature = Some(SignatureView { help, line, col });
            }
            None => {
                self.signature = None;
                if explicit {
                    self.notice = Some(tr!("notice-no-parameter-info"));
                }
            }
        }
    }

    /// Opens the rename dialog for the identifier at the caret (`⇧F6`).
    pub(crate) fn open_rename(&mut self) -> Task<Message> {
        let Some(doc) = self.doc().filter(|d| d.is_php()) else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let start = phpthunder::analysis::ident_start(&doc.text, offset);
        let end = doc.text[offset..]
            .find(|c: char| !is_ident(c))
            .map_or(doc.text.len(), |i| offset + i);
        let name = doc.text[start..end].to_string();
        if name.is_empty() {
            self.notice = Some(tr!("notice-caret-on-name"));
            return Task::none();
        }
        self.close_overlays();
        self.rename = Some(RenameDialog {
            original: name.clone(),
            name,
            offset: start,
            error: None,
        });
        Task::batch([
            operation::focus(Id::new(RENAME_INPUT)),
            operation::select_all(Id::new(RENAME_INPUT)),
        ])
    }

    /// Plans the rename in the background; the plan comes back as
    /// [`Message::RenamePlanned`].
    pub(crate) fn submit_rename(&mut self) -> Task<Message> {
        let (Some(dialog), Some(doc)) = (self.rename.as_ref(), self.doc()) else {
            return Task::none();
        };
        if dialog.name == dialog.original {
            self.rename = None;
            return operation::focus(Id::new(EDITOR_ID));
        }
        let open = self.open_texts();
        let task = self.lang.clone().rename(
            doc.path.clone(),
            doc.text.clone(),
            dialog.offset,
            dialog.name.clone(),
            open,
        );
        Task::perform(task, Message::RenamePlanned)
    }

    /// Applies a rename plan: edits open tabs (undoable) and files on disk,
    /// then renames the class file when the plan asks for it.
    pub(crate) fn apply_rename(&mut self, plan: Result<RenamePlan, String>) -> Task<Message> {
        let plan = match plan {
            Ok(plan) => plan,
            Err(error) => {
                if let Some(dialog) = &mut self.rename {
                    dialog.error = Some(error);
                }
                return Task::none();
            }
        };
        self.rename = None;
        let current = self.doc().map(|d| d.path.clone());
        let files = plan.edits.len();
        let occurrences: usize = plan.edits.values().map(Vec::len).sum();
        for (path, edits) in &plan.edits {
            if Some(path) == current.as_ref() {
                if let Some(doc) = self.doc_mut() {
                    doc.apply_edits(edits);
                }
                self.after_change(true);
            } else {
                self.edit_other_file(path, edits);
            }
        }
        if let Some((from, to)) = &plan.file_rename {
            self.rename_file(from, to);
        }
        for (path, content) in &plan.created {
            match super::files::create_file(path, content) {
                Ok(_) => {
                    self.lang.update_file(path, content);
                    if let Some(dir) = path.parent() {
                        self.tree.refresh(dir);
                    }
                    if let Err(at) = self.project_files.binary_search(path) {
                        self.project_files.insert(at, path.clone());
                    }
                }
                Err(error) => self.fail(error),
            }
        }
        for path in &plan.deleted {
            match super::files::move_to_trash(path) {
                Ok(()) => self.after_file_moved(path, None),
                Err(e) => self.fail(tr!(
                    "notice-write-failed",
                    path = path.display().to_string(),
                    error = e.to_string()
                )),
            }
        }
        self.notice = Some(tr!(
            "notice-renamed",
            occurrences = occurrences,
            files = files
        ));
        Task::batch([self.reveal_cursor(), operation::focus(Id::new(EDITOR_ID))])
    }

    /// Moves a file on disk and keeps tabs, index, tree and file lists in
    /// sync. Refuses to overwrite an existing file.
    fn rename_file(&mut self, from: &std::path::Path, to: &std::path::Path) {
        if to.exists() {
            self.notice = Some(tr!("notice-rename-exists", path = to.display().to_string()));
            return;
        }
        // Unsaved edits of the renamed file are written first, so the move
        // does not lose them.
        if let Some(doc) = self.docs.iter_mut().find(|d| d.path == from)
            && std::fs::write(from, &doc.text).is_ok()
        {
            doc.mark_saved();
        }
        if let Some(dir) = to.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::rename(from, to) {
            self.fail(tr!(
                "notice-rename-failed",
                path = from.display().to_string(),
                error = e.to_string()
            ));
            return;
        }
        if let Some(doc) = self.docs.iter_mut().find(|d| d.path == from) {
            doc.path = to.to_path_buf();
        }
        self.lang.remove_file(from);
        if let Ok(text) = std::fs::read_to_string(to) {
            self.lang.update_file(to, &text);
        }
        for list in [&mut self.project_files, &mut self.recent] {
            for p in list.iter_mut().filter(|p| p.as_path() == from) {
                *p = to.to_path_buf();
            }
        }
        if let Some(dir) = to.parent() {
            self.tree.refresh(dir);
        }
    }
}
