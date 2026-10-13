//! The Compare panel and what feeds it: the active file against the
//! clipboard or one of its local history versions (with Restore), plus
//! Git Annotate (who last changed each line).

use super::*;
use crate::palette::{Item, Target};
use crate::tr;
use iced::Task;
use std::path::PathBuf;

/// What the Compare panel shows.
pub struct Compare {
    /// Panel heading, e.g. "Clipboard ↔ Order.php".
    pub title: String,
    /// The file compared.
    pub path: PathBuf,
    /// Diff from the other text to the file's current text.
    pub diff: Vec<crate::git::DiffLine>,
    /// Text that Restore puts back into the file, for history versions.
    pub restore: Option<String>,
}

impl App {
    /// Copies the active document into local history (on save).
    pub(crate) fn record_local_history(&self) {
        let (Some(doc), Some(base)) = (self.doc(), crate::local_history::base_dir()) else {
            return;
        };
        crate::local_history::record(&base, &doc.path, &doc.text);
    }

    /// Compares the selection (or the whole file) with the clipboard.
    pub(crate) fn compare_with_clipboard(&mut self) -> Task<Message> {
        if self.doc().is_none() {
            return Task::none();
        }
        iced::clipboard::read().map(Message::ClipboardForCompare)
    }

    /// Shows the diff from the clipboard text to the selection or file.
    pub(crate) fn clipboard_arrived(&mut self, clipboard: Option<String>) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let Some(clipboard) = clipboard else {
            self.notice = Some(tr!("notice-clipboard-empty"));
            return Task::none();
        };
        let mine = doc.selected_text().unwrap_or(&doc.text).to_string();
        self.compare = Some(Compare {
            title: tr!("compare-clipboard", file = doc.title()),
            path: doc.path.clone(),
            diff: crate::git::diff_lines(&clipboard, &mine),
            restore: None,
        });
        self.show_panel(Bottom::Compare);
        Task::none()
    }

    /// Lists the local history versions of the active file, newest first.
    pub(crate) fn open_local_history(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let Some(base) = crate::local_history::base_dir() else {
            return Task::none();
        };
        let versions = crate::local_history::versions(&base, &doc.path);
        if versions.is_empty() {
            self.notice = Some(tr!("notice-no-history"));
            return Task::none();
        }
        let items = versions
            .into_iter()
            .map(|v| Item {
                badge: "◷",
                title: tr!("history-ago", age = crate::local_history::age(v.saved_at)),
                matched: Vec::new(),
                detail: String::new(),
                target: Target::LocalVersion(v.path),
            })
            .collect();
        self.open_choice(tr!("action-local-history", file = doc.title()), items)
    }

    /// Compares a stored version with the active file, offering Restore.
    pub(crate) fn open_local_version(&mut self, version: PathBuf) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let Ok(old) = std::fs::read_to_string(&version) else {
            return Task::none();
        };
        self.compare = Some(Compare {
            title: tr!("compare-history", file = doc.title()),
            path: doc.path.clone(),
            diff: crate::git::diff_lines(&old, &doc.text),
            restore: Some(old),
        });
        self.show_panel(Bottom::Compare);
        Task::none()
    }

    /// Replaces the compared file's text with the history version shown.
    pub(crate) fn restore_compared(&mut self) -> Task<Message> {
        let Some((path, text)) = self
            .compare
            .as_ref()
            .and_then(|c| c.restore.clone().map(|t| (c.path.clone(), t)))
        else {
            return Task::none();
        };
        let Some(doc) = self.docs.iter_mut().find(|d| d.path == path) else {
            return Task::none();
        };
        let cursor = doc.cursor();
        doc.set_text(text, cursor);
        self.after_change(true);
        self.compare = None;
        self.hide_panel(Bottom::Compare);
        self.notice = Some(tr!("notice-restored"));
        Task::none()
    }

    /// Shows or hides Git Annotate for the active file.
    pub(crate) fn toggle_annotate(&mut self) -> Task<Message> {
        let (Some(doc), Some(root)) = (self.doc(), self.git_root()) else {
            return Task::none();
        };
        let path = doc.path.clone();
        if self.blame.remove(&path).is_some() {
            return Task::none();
        }
        Task::perform(crate::git::blame(root, path.clone()), move |result| {
            Message::BlameArrived(path, result)
        })
    }

    /// Stores annotations, or reports why Git could not give them.
    pub(crate) fn blame_arrived(
        &mut self,
        path: PathBuf,
        result: Result<Vec<crate::git::LineBlame>, String>,
    ) {
        match result {
            Ok(lines) => {
                self.blame.insert(path, lines);
            }
            Err(error) => self.notice = Some(error),
        }
    }
}
