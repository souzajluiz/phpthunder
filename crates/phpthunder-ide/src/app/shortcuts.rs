//! Handlers for the smaller keyboard commands: numbered and mnemonic
//! bookmarks, method and change navigation, tool window toggles and sizes,
//! window state, usage highlighting, and file helpers (copy path, reveal in
//! Finder, settings, scratch files, Save As).

use super::*;
use crate::palette::{Item, Target};
use crate::tr;
use iced::Task;
use iced::widget::text_editor::Position;
use phpthunder::symbol::Kind;
use std::path::PathBuf;

/// Tool layout hidden by `⇧⌘F12`, restored by the next press: sidebar
/// shown, bottom panel, right panel.
pub type HiddenTools = (bool, Option<Bottom>, Option<Bottom>);

/// Pixels one `⌃⌥` arrow press grows or shrinks a panel by.
const STRETCH: f32 = 40.0;

/// Where `⌃⌥` arrow keys stretch a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// `⌃⌥←`.
    Left,
    /// `⌃⌥→`.
    Right,
    /// `⌃⌥↑`.
    Up,
    /// `⌃⌥↓`.
    Down,
}

impl App {
    /// `⌃⇧0`–`⌃⇧9` (with `set`) puts numbered bookmark `digit` on the caret
    /// line, or takes it off when it is already there; `⌃0`–`⌃9` jumps to it.
    pub(crate) fn numbered_bookmark(&mut self, digit: u8, set: bool) -> Task<Message> {
        let key = char::from(b'0' + digit.min(9));
        if set {
            return self.toggle_mnemonic(key);
        }
        match self.numbered_bookmarks.get(&key).cloned() {
            Some((path, line)) => {
                self.record_nav();
                self.open_at_line(path, line)
            }
            None => {
                self.notice = Some(tr!("notice-no-numbered-bookmark", digit = digit));
                Task::none()
            }
        }
    }

    /// Puts mnemonic bookmark `key` (a digit or letter) on the caret line,
    /// moving it from wherever it was, or takes it off when it is already
    /// there.
    pub(crate) fn toggle_mnemonic(&mut self, key: char) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let mark = (doc.path.clone(), doc.cursor().line);
        if self.numbered_bookmarks.get(&key) == Some(&mark) {
            self.numbered_bookmarks.remove(&key);
        } else {
            self.numbered_bookmarks.insert(key, mark);
        }
        Task::none()
    }

    /// `⌥F3`: picks the digit or letter for a mnemonic bookmark on the
    /// caret line; keys in use show where they point.
    pub(crate) fn choose_mnemonic(&mut self) -> Task<Message> {
        if self.doc().is_none() {
            return Task::none();
        }
        let items = ('0'..='9')
            .chain('A'..='Z')
            .map(|key| Item {
                badge: "◆",
                title: key.to_string(),
                matched: Vec::new(),
                detail: self
                    .numbered_bookmarks
                    .get(&key)
                    .map(|(path, line)| format!("{}:{}", self.relative(path), line + 1))
                    .unwrap_or_default(),
                target: Target::Mnemonic(key),
            })
            .collect();
        self.open_choice(tr!("action-mnemonic-bookmark"), items)
    }

    /// `⌃⇧↓` / `⌃⇧↑`: moves the caret to the next (or previous) function or
    /// method declared in the file, from the index's file symbols.
    pub(crate) fn goto_method(&mut self, forward: bool) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let caret = doc.cursor().line;
        let mut lines: Vec<usize> = self
            .lang
            .file_symbols(&doc.path)
            .iter()
            .filter(|s| matches!(s.kind, Kind::Method | Kind::Function) && !s.is_virtual)
            .map(|s| s.name_span.start.line as usize)
            .collect();
        lines.sort_unstable();
        lines.dedup();
        let target = if forward {
            lines.into_iter().find(|l| *l > caret)
        } else {
            lines.into_iter().rev().find(|l| *l < caret)
        };
        match target {
            Some(line) => self.place_cursor(Position { line, column: 0 }),
            None => Task::none(),
        }
    }

    /// `⌃⌥⇧↓` / `⌃⌥⇧↑`: moves the caret to the next (or previous) range
    /// changed since the last commit, from the gutter's git marks.
    pub(crate) fn goto_change(&mut self, forward: bool) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let caret = doc.cursor().line;
        let starts: Vec<usize> = self
            .git_marks
            .get(&doc.path)
            .map(|marks| marks.iter().map(|m| m.line).collect())
            .unwrap_or_default();
        let target = if forward {
            starts.iter().find(|l| **l > caret).or(starts.first())
        } else {
            starts.iter().rev().find(|l| **l < caret).or(starts.last())
        };
        match target {
            Some(&line) => self.place_cursor(Position { line, column: 0 }),
            None => {
                self.notice = Some(tr!("notice-no-changes"));
                Task::none()
            }
        }
    }

    /// `⌘F1`: the messages of the problems at the caret, as a notice.
    pub(crate) fn error_description(&mut self) -> Task<Message> {
        let Some(offset) = self.doc().map(|d| d.offset(d.cursor())) else {
            return Task::none();
        };
        let messages: Vec<String> = self
            .diagnostics_at(offset)
            .into_iter()
            .map(|d| d.message)
            .collect();
        self.notice = Some(if messages.is_empty() {
            tr!("notice-no-problem-at-caret")
        } else {
            messages.join(" · ")
        });
        Task::none()
    }

    /// `⇧F1`: opens the PHP manual for the identifier at the caret.
    pub(crate) fn external_doc(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let word = identifier_at(&doc.text, doc.offset(doc.cursor()));
        if word.is_empty() {
            return Task::none();
        }
        crate::packages::open_in_browser(&format!(
            "https://www.php.net/manual-lookup.php?pattern={word}&scope=quickref"
        ));
        Task::none()
    }

    /// `⇧⌘C`: copies the active file's absolute path.
    pub(crate) fn copy_path(&mut self) -> Task<Message> {
        let Some(path) = self.doc().map(|d| d.path.display().to_string()) else {
            return Task::none();
        };
        self.notice = Some(tr!("notice-copied", text = path.as_str()));
        iced::clipboard::write(path)
    }

    /// `⌥⌘F12`: shows the active file in the system file manager.
    pub(crate) fn reveal_in_file_manager(&mut self) -> Task<Message> {
        let Some(path) = self.doc().map(|d| d.path.clone()) else {
            return Task::none();
        };
        let mut command = if cfg!(target_os = "macos") {
            let mut c = std::process::Command::new("open");
            c.arg("-R").arg(&path);
            c
        } else if cfg!(target_os = "windows") {
            let mut c = std::process::Command::new("explorer");
            c.arg(format!("/select,{}", path.display()));
            c
        } else {
            let mut c = std::process::Command::new("xdg-open");
            c.arg(path.parent().unwrap_or(&path));
            c
        };
        // A missing file manager only means nothing opens.
        let _ = command.spawn();
        Task::none()
    }

    /// `⌥F1`: shows the project tree with the active file selected.
    pub(crate) fn select_in_tree(&mut self) -> Task<Message> {
        let Some(path) = self.doc().map(|d| d.path.clone()) else {
            return Task::none();
        };
        self.sidebar = true;
        self.tree.reveal(&path);
        Task::none()
    }

    /// `⌘1`-style toggle for `panel`: hides it when shown, shows it otherwise.
    pub(crate) fn toggle_panel(&mut self, panel: Bottom) -> Task<Message> {
        if self.is_shown(panel) {
            self.hide_panel(panel);
        } else {
            self.show_panel(panel);
        }
        Task::none()
    }

    /// `⇧⎋`: closes the bottom panel, or the right one when there is none.
    pub(crate) fn hide_tool_window(&mut self) -> Task<Message> {
        if self.bottom.is_some() {
            self.close_panels(Dock::Bottom);
        } else {
            self.close_panels(Dock::Right);
        }
        Task::none()
    }

    /// `⇧⌘F12`: hides the sidebar and both panel areas, or brings back what
    /// the previous press hid.
    pub(crate) fn toggle_all_tool_windows(&mut self) -> Task<Message> {
        if let Some((sidebar, bottom, right)) = self.hidden_tools.take() {
            self.sidebar = sidebar;
            self.bottom = bottom;
            self.right = right;
        } else if self.sidebar || self.bottom.is_some() || self.right.is_some() {
            self.hidden_tools = Some((self.sidebar, self.bottom, self.right));
            self.sidebar = false;
            self.close_panels(Dock::Bottom);
            self.close_panels(Dock::Right);
        }
        Task::none()
    }

    /// `⌃L`: scrolls so the caret line sits in the middle of the editor.
    pub(crate) fn scroll_to_center(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let line = doc.cursor().line;
        let row_top = crate::app::row_top(&self.wrap_layout(doc), line, 0);
        let y = (row_top - self.viewport_height / 2.0).max(0.0);
        if let Some(doc) = self.doc_mut() {
            doc.scroll_y = y;
        }
        crate::app::scroll_to(y)
    }

    /// `⌃⇧G`: drops the occurrence `⌃G` added last, back to the previous one.
    pub(crate) fn unselect_occurrence(&mut self) -> Task<Message> {
        if let Some(doc) = self.doc_mut() {
            doc.unselect_occurrence();
        }
        self.reveal_cursor()
    }

    /// `⌥F8`: evaluates the selection in the paused script, or opens the
    /// debugger panel to type an expression.
    pub(crate) fn evaluate_expression(&mut self) -> Task<Message> {
        let selected = self
            .doc()
            .and_then(|d| d.selected_text())
            .map(str::to_string);
        if let (Some(expression), Some(view)) = (selected, self.debug.as_mut()) {
            view.eval = expression;
            return self.debug_evaluate();
        }
        self.show_panel(Bottom::Debug);
        Task::none()
    }

    /// `⇧⌘F8`: every line breakpoint, by file and line.
    pub(crate) fn view_breakpoints(&mut self) -> Task<Message> {
        let marks: Vec<(PathBuf, usize)> = self
            .breakpoints
            .iter()
            .flat_map(|(p, lines)| lines.iter().map(move |l| (p.clone(), *l)))
            .collect();
        if marks.is_empty() {
            self.notice = Some(tr!("notice-no-breakpoints"));
            return Task::none();
        }
        let items = marks
            .into_iter()
            .map(|(path, line)| Item {
                badge: "●",
                title: self.line_preview(&path, line),
                matched: Vec::new(),
                detail: format!("{}:{}", self.relative(&path), line + 1),
                target: Target::Location(path, line),
            })
            .collect();
        self.open_choice(tr!("action-breakpoints"), items)
    }

    /// `⌘,`: opens the user settings file in the editor.
    pub(crate) fn open_settings(&mut self) -> Task<Message> {
        let Some(dir) = crate::settings::user_config_dir() else {
            return Task::none();
        };
        let json = dir.join("settings.json");
        let path = if json.is_file() {
            json
        } else {
            dir.join("settings.toml")
        };
        self.open(path, None)
    }

    /// `⌥⌘Y`: reloads open files from disk (unmodified tabs take the disk
    /// text) and refreshes the git state.
    pub(crate) fn reload_from_disk(&mut self) -> Task<Message> {
        let paths: Vec<PathBuf> = self.docs.iter().map(|d| d.path.clone()).collect();
        self.update(Message::FilesChanged(paths))
    }

    /// `⌃⌘F`: switches the window in and out of full screen.
    pub(crate) fn toggle_full_screen(&mut self) -> Task<Message> {
        use iced::window;
        window::latest().then(|id| match id {
            Some(id) => window::mode(id).then(move |mode| {
                window::set_mode(
                    id,
                    if mode == window::Mode::Fullscreen {
                        window::Mode::Windowed
                    } else {
                        window::Mode::Fullscreen
                    },
                )
            }),
            None => Task::none(),
        })
    }

    /// `⌃⌥` arrows: grows or shrinks the open panel toward `toward`: the
    /// right panel (else the sidebar) left and right, the bottom panel up
    /// and down. Sizes are saved like after a drag.
    pub(crate) fn stretch_tool_window(&mut self, toward: Direction) -> Task<Message> {
        let (splitter, delta) = match toward {
            Direction::Up | Direction::Down if self.bottom.is_some() => (
                Splitter::Bottom,
                if toward == Direction::Up {
                    STRETCH
                } else {
                    -STRETCH
                },
            ),
            Direction::Left | Direction::Right if self.right.is_some() => (
                Splitter::Right,
                if toward == Direction::Left {
                    STRETCH
                } else {
                    -STRETCH
                },
            ),
            Direction::Left | Direction::Right if self.sidebar => (
                Splitter::Sidebar,
                if toward == Direction::Right {
                    STRETCH
                } else {
                    -STRETCH
                },
            ),
            _ => return Task::none(),
        };
        let room = self.room_beside(splitter);
        self.panes.nudge(splitter, delta, room);
        self.maximized_tool = None;
        SavedLayout::from(self.panes.sizes).save();
        Task::none()
    }

    /// `⇧⌘'`: gives the bottom panel (else the right one) all the room the
    /// editor can spare, or puts back its size from before.
    pub(crate) fn toggle_maximize_tool_window(&mut self) -> Task<Message> {
        if let Some((splitter, size)) = self.maximized_tool.take() {
            self.panes.set(splitter, size);
            return Task::none();
        }
        let splitter = if self.bottom.is_some() {
            Splitter::Bottom
        } else if self.right.is_some() {
            Splitter::Right
        } else {
            return Task::none();
        };
        let before = self.panes.size(splitter);
        let room = self.room_beside(splitter);
        self.panes.nudge(splitter, f32::INFINITY, room);
        self.maximized_tool = Some((splitter, before));
        Task::none()
    }

    /// Pixels the editor has along `splitter`'s axis, for pane limits.
    fn room_beside(&self, splitter: Splitter) -> f32 {
        match splitter {
            Splitter::Sidebar | Splitter::Right => self.editor_width,
            Splitter::Bottom => self.viewport_height,
        }
    }

    /// `⇧⌘F7`: keeps every usage of the word at the caret highlighted in
    /// this file until Escape, or stops when pressed on the same word.
    pub(crate) fn highlight_usages_in_file(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let Some((start, end)) = doc.identifier_at_caret() else {
            self.highlighted_usages = None;
            return Task::none();
        };
        let word = doc.text[start..end].to_string();
        let mark = (doc.path.clone(), word);
        if self.highlighted_usages.as_ref() == Some(&mark) {
            self.highlighted_usages = None;
            return Task::none();
        }
        let count = doc.occurrences_in(&mark.1, 0, doc.text.len()).len();
        self.notice = Some(tr!("notice-usages-highlighted", count = count));
        self.highlighted_usages = Some(mark);
        Task::none()
    }

    /// `⇧⌘N`: creates the next free `scratch_N.php` in the user config
    /// folder and opens it; scratch files live outside every project.
    pub(crate) fn new_scratch_file(&mut self) -> Task<Message> {
        let Some(dir) = crate::settings::user_config_dir().map(|d| d.join("scratches")) else {
            return Task::none();
        };
        let path = (1..)
            .map(|n| dir.join(format!("scratch_{n}.php")))
            .find(|p| !p.exists())
            .unwrap_or_else(|| dir.join("scratch.php"));
        let created =
            std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, "<?php\n\n"));
        if let Err(error) = created {
            self.fail(tr!("notice-scratch-failed", error = error.to_string()));
            return Task::none();
        }
        self.open(path, Some(phpthunder::syntax::Pos { line: 2, col: 0 }))
    }

    /// `⇧⌘S`: asks where to save a copy of the active file
    /// ([`Message::SaveAsPicked`]).
    pub(crate) fn save_as(&mut self) -> Task<Message> {
        if self.preview_active() {
            return Task::none();
        }
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let mut dialog =
            rfd::AsyncFileDialog::new().set_file_name(crate::app::file_name(&doc.path));
        if let Some(dir) = doc.path.parent() {
            dialog = dialog.set_directory(dir);
        }
        Task::perform(
            async move { dialog.save_file().await.map(|h| h.path().to_path_buf()) },
            Message::SaveAsPicked,
        )
    }

    /// Writes the active buffer to `path` (EditorConfig save rules applied)
    /// and opens it there with the same caret; the original tab is left
    /// as it was.
    pub(crate) fn save_as_picked(&mut self, path: Option<PathBuf>) -> Task<Message> {
        let (Some(path), Some(doc)) = (path, self.doc()) else {
            return Task::none();
        };
        let (_, disk) = doc.text_for_save();
        let caret = doc.utf16_pos(doc.cursor());
        if let Err(error) = std::fs::write(&path, disk) {
            self.fail(tr!(
                "notice-save-failed",
                file = crate::app::file_name(&path),
                error = error.to_string()
            ));
            return Task::none();
        }
        Task::batch([self.open(path, Some(caret)), self.refresh_git()])
    }

    /// `⌘M`: minimizes the window.
    pub(crate) fn minimize_window(&mut self) -> Task<Message> {
        use iced::window;
        window::latest().then(|id| match id {
            Some(id) => window::minimize(id, true),
            None => Task::none(),
        })
    }
}

/// The PHP identifier (letters, digits, `_`, `\`) around byte `offset` of
/// `text`, without a leading `$` or namespace; empty when there is none.
fn identifier_at(text: &str, offset: usize) -> &str {
    let offset = offset.min(text.len());
    let is_part = |c: char| is_ident(c) || c == '\\';
    let start = text[..offset]
        .rfind(|c: char| !is_part(c))
        .map_or(0, |i| i + 1);
    let end = text[offset..]
        .find(|c: char| !is_part(c))
        .map_or(text.len(), |i| offset + i);
    let word = &text[start..end];
    word.rsplit('\\').next().unwrap_or(word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_at_should_take_the_word_around_the_caret() {
        assert_eq!(identifier_at("echo strlen($x);", 8), "strlen");
    }

    #[test]
    fn identifier_at_should_drop_the_namespace() {
        assert_eq!(identifier_at("\\Foo\\array_map(", 7), "array_map");
    }

    #[test]
    fn identifier_at_should_be_empty_between_words() {
        assert_eq!(identifier_at("a  b", 2), "");
    }
}
