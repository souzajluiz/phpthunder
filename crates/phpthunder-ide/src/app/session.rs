//! Remembering the work session: the last project and, per project, the open
//! tabs with their caret and the active tab, restored on the next start.

use super::*;
use iced::Task;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One open tab as saved: file, caret line and column.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SavedTab {
    /// The file.
    pub path: PathBuf,
    /// Caret line (0-based).
    pub line: usize,
    /// Caret column (0-based, in characters).
    pub column: usize,
}

/// Tabs of one project as saved.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SavedProject {
    /// Open tabs in order.
    pub tabs: Vec<SavedTab>,
    /// Index of the active tab.
    pub active: Option<usize>,
}

/// Everything remembered between runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SavedSession {
    /// Project opened last.
    pub last_project: Option<PathBuf>,
    /// Tabs per project root.
    pub projects: HashMap<PathBuf, SavedProject>,
}

/// Where the session is kept; `None` without a home folder. Disabled in
/// development snapshots so they never change the user's session.
fn session_path() -> Option<PathBuf> {
    if std::env::var_os("PHPTHUNDER_SNAPSHOT").is_some() {
        return None;
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/phpthunder/session.json"))
}

/// The saved session; empty when missing or unreadable.
pub(crate) fn load_session() -> SavedSession {
    session_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

impl App {
    /// The tabs of the open project as they should be saved.
    fn current_project(&self) -> SavedProject {
        SavedProject {
            tabs: self
                .docs
                .iter()
                .map(|d| {
                    let caret = d.cursor();
                    SavedTab {
                        path: d.path.clone(),
                        line: caret.line,
                        column: caret.column,
                    }
                })
                .collect(),
            active: self.active,
        }
    }

    /// Writes the session when the open project or its tabs changed since
    /// the last write. Called after every update; cheap when nothing moved.
    pub(crate) fn save_session_if_changed(&mut self) {
        let Some(root) = self.tree.root.clone() else {
            return;
        };
        let project = self.current_project();
        let unchanged = self.session.last_project.as_ref() == Some(&root)
            && self.session.projects.get(&root) == Some(&project);
        if unchanged || self.restoring {
            return;
        }
        self.session.last_project = Some(root.clone());
        self.session.projects.insert(root, project);
        let Some(path) = session_path() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.session) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Reads the saved tabs of `root` in the background (answered by
    /// [`Message::SessionRestored`]); nothing when none were saved.
    pub(crate) fn restore_tabs(&mut self, root: &Path) -> Task<Message> {
        let Some(saved) = self.session.projects.get(root).cloned() else {
            return Task::none();
        };
        if saved.tabs.is_empty() {
            return Task::none();
        }
        self.restoring = true;
        Task::perform(
            async move {
                let mut loaded = Vec::new();
                for tab in saved.tabs {
                    if let Ok(text) = tokio::fs::read_to_string(&tab.path).await {
                        loaded.push((tab, text));
                    }
                }
                (loaded, saved.active)
            },
            |(tabs, active)| Message::SessionRestored(tabs, active),
        )
    }

    /// Opens the restored tabs in their saved order with their carets, and
    /// activates the saved tab.
    pub(crate) fn session_restored(
        &mut self,
        tabs: Vec<(SavedTab, String)>,
        active: Option<usize>,
    ) -> Task<Message> {
        self.restoring = false;
        let mut tasks = Vec::new();
        for (tab, text) in tabs {
            if self.docs.iter().any(|d| d.path == tab.path) {
                continue;
            }
            let mut doc = crate::editor::Document::new(tab.path.clone(), text);
            let position = iced::widget::text_editor::Position {
                line: tab.line.min(doc.line_count().saturating_sub(1)),
                column: tab.column,
            };
            doc.move_to(position);
            if doc.is_php() {
                self.lang.update_file(&doc.path, &doc.text);
            }
            tasks.push(self.load_git_base(tab.path));
            self.docs.push(doc);
        }
        if self.docs.is_empty() {
            return Task::batch(tasks);
        }
        let index = active.unwrap_or(0).min(self.docs.len() - 1);
        tasks.push(self.select(index));
        self.inspect_pending = true;
        Task::batch(tasks)
    }
}
