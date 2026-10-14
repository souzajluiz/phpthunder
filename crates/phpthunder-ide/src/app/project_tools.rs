//! Project upkeep commands: copyright headers, file watchers run on save,
//! and the log file picker.

use super::*;
use crate::filetype::FileType;
use crate::interpreter::project_config;
use crate::palette::{Item, Target};
use crate::tr;
use iced::Task;
use std::path::{Path, PathBuf};

/// Folders, relative to the project, where frameworks write their logs.
const LOG_DIRS: &[&str] = &["storage/logs", "var/log", "logs", "log", "."];

/// The project's copyright notice for `path`, variables filled in; `None`
/// when the project file sets none.
fn notice_for(root: &Path, path: &Path) -> Option<String> {
    let template = project_config(root).copyright?;
    let project = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Some(crate::copyright::render(
        &template,
        crate::copyright::current_year(),
        &project,
        &file,
    ))
}

impl App {
    /// Writes or refreshes the copyright header of the active file, as one
    /// undo step. Without a notice in the project file, opens the project
    /// file so one can be added.
    pub(crate) fn update_copyright(&mut self) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let Some(path) = self.doc().map(|d| d.path.clone()) else {
            return Task::none();
        };
        let Some(notice) = notice_for(&root, &path) else {
            self.notice = Some(tr!("copyright-missing"));
            return self.edit_project_settings();
        };
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        let Some(text) = crate::copyright::apply(&doc.text, doc.file_type, &notice) else {
            self.notice = Some(tr!("copyright-current"));
            return Task::none();
        };
        let cursor = doc.cursor();
        doc.checkpoint();
        doc.set_text(text, cursor);
        self.after_change(true);
        Task::none()
    }

    /// Writes or refreshes the copyright header of every project file that
    /// takes one (vendor and open files with unsaved changes left alone),
    /// then reports how many changed. Open tabs follow through the file
    /// watcher.
    pub(crate) fn update_copyright_everywhere(&mut self) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        if project_config(&root).copyright.is_none() {
            self.notice = Some(tr!("copyright-missing"));
            return self.edit_project_settings();
        }
        let dirty: Vec<PathBuf> = self
            .docs
            .iter()
            .filter(|d| d.dirty)
            .map(|d| d.path.clone())
            .collect();
        let mut changed = 0;
        for path in &self.project_files {
            if dirty.contains(path) || path.components().any(|c| c.as_os_str() == "vendor") {
                continue;
            }
            let (Some(notice), Ok(text)) = (notice_for(&root, path), std::fs::read_to_string(path))
            else {
                continue;
            };
            let file_type = FileType::detect(path, text.lines().next());
            if let Some(updated) = crate::copyright::apply(&text, file_type, &notice)
                && std::fs::write(path, updated).is_ok()
            {
                changed += 1;
            }
        }
        self.notice = Some(tr!("copyright-updated", count = changed));
        Task::none()
    }

    /// Starts the project's file watchers that match the just-saved `path`;
    /// each reports through [`Message::WatcherDone`].
    pub(crate) fn run_watchers(&self, path: &Path) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let config = project_config(&root);
        Task::batch(
            crate::watchers::matching(&config.watcher, &root, path)
                .into_iter()
                .map(|w| {
                    let line = crate::watchers::command_line(w, &root, path);
                    Task::perform(
                        crate::watchers::run(w.name.clone(), root.clone(), line),
                        Message::WatcherDone,
                    )
                }),
        )
    }

    /// Lists the project's log files, newest first, from the folders
    /// frameworks write them to; the choice opens in a tab that follows the
    /// file while its caret stays on the last line.
    pub(crate) fn open_log_picker(&mut self) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let mut logs: Vec<(std::time::SystemTime, PathBuf)> = LOG_DIRS
            .iter()
            .filter_map(|dir| std::fs::read_dir(root.join(dir)).ok())
            .flat_map(|entries| entries.flatten())
            .map(|e| e.path())
            .filter(|p| FileType::detect(p, None) == FileType::Log)
            .filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p)))
            .collect();
        logs.sort_by_key(|(time, _)| std::cmp::Reverse(*time));
        logs.dedup_by(|a, b| a.1 == b.1);
        if logs.is_empty() {
            self.notice = Some(tr!("log-none"));
            return Task::none();
        }
        let items = logs
            .into_iter()
            .map(|(_, path)| {
                let rel = path.strip_prefix(&root).unwrap_or(&path);
                let size = path.metadata().map_or(0, |m| m.len());
                Item {
                    badge: "≡",
                    title: path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    matched: Vec::new(),
                    detail: format!("{}  ·  {} KB", rel.display(), size.div_ceil(1024)),
                    target: Target::File(path),
                }
            })
            .collect();
        self.open_choice(tr!("log-title"), items)
    }
}
