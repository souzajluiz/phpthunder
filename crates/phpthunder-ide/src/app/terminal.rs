//! Driving the integrated terminal and the Claude panel: starting the shell
//! or the `claude` CLI, routing keys and clipboard to the session of the
//! panel on screen, and keeping both sizes in sync with the panel.

use super::*;
use crate::terminal::{self, Session};
use crate::ui::{TERMINAL_CHAR_WIDTH, TERMINAL_LINE, TERMINAL_PADDING};
use iced::keyboard::{self, Key, Modifiers};
use iced::widget::{Id, operation};
use iced::{Size, Task};

/// Widget id of the invisible input that holds keyboard focus while the
/// terminal is active (taking focus away from the code editor).
pub const TERMINAL_SINK: &str = "terminal-sink";

/// Screen size used before the panel has been measured.
const DEFAULT_GRID: (u16, u16) = (24, 100);

impl App {
    /// Shows the terminal panel, starting a shell in the project root if none
    /// is running, and focuses it; hides the panel when it is already shown
    /// and focused (`⌥F12`).
    pub(crate) fn toggle_terminal(&mut self) -> Task<Message> {
        if self.is_shown(Bottom::Terminal)
            && self.terminal_focused
            && self.console == Bottom::Terminal
        {
            self.hide_panel(Bottom::Terminal);
            self.terminal_focused = false;
            return operation::focus(Id::new(EDITOR_ID));
        }
        self.show_panel(Bottom::Terminal);
        if self.terminal.as_ref().is_none_or(Session::has_exited) {
            self.start_terminal();
        }
        self.focus_terminal()
    }

    /// Starts a shell in the project root (or the home directory).
    pub(crate) fn start_terminal(&mut self) {
        let (rows, cols) = self.terminal_grid.unwrap_or(DEFAULT_GRID);
        match Session::spawn(&self.console_cwd(), rows, cols) {
            Ok(session) => self.terminal = Some(session),
            Err(error) => self.notice = Some(error),
        }
    }

    /// Working directory for new console sessions: the project root, else
    /// the home directory.
    fn console_cwd(&self) -> std::path::PathBuf {
        self.tree
            .root
            .clone()
            .or_else(std::env::home_dir)
            .unwrap_or_else(|| ".".into())
    }

    /// Shows the Claude panel, starting the `claude` CLI in the project root
    /// if it is not running, and focuses it; hides the panel when it is
    /// already shown and focused (`⌥⌘A`).
    pub(crate) fn toggle_claude(&mut self) -> Task<Message> {
        if self.is_shown(Bottom::Claude) && self.terminal_focused && self.console == Bottom::Claude
        {
            self.hide_panel(Bottom::Claude);
            self.terminal_focused = false;
            return operation::focus(Id::new(EDITOR_ID));
        }
        self.show_panel(Bottom::Claude);
        if self.claude.as_ref().is_none_or(Session::has_exited) {
            self.start_claude();
        }
        self.focus_terminal()
    }

    /// Starts the `claude` CLI in the project root through the login shell.
    /// When it is not installed, the shell's "command not found" shows in
    /// the panel above the exited banner, whose Start button retries.
    pub(crate) fn start_claude(&mut self) {
        let (rows, cols) = self
            .claude_grid
            .or(self.terminal_grid)
            .unwrap_or(DEFAULT_GRID);
        match Session::spawn_program(
            &self.console_cwd(),
            rows,
            cols,
            &crate::settings::settings().claude_command,
        ) {
            Ok(session) => {
                self.claude = Some(session);
                self.notice = None;
            }
            Err(error) => self.notice = Some(error),
        }
    }

    /// The session keys and pastes go to: Claude's when the Claude panel
    /// was focused last, else the terminal's.
    pub(crate) fn console_session(&self) -> Option<&Session> {
        self.session_of(self.console)
    }

    /// The session behind console panel `panel`.
    pub(crate) fn session_of(&self, panel: Bottom) -> Option<&Session> {
        match panel {
            Bottom::Claude => self.claude.as_ref(),
            _ => self.terminal.as_ref(),
        }
    }

    /// Mutable session behind console panel `panel`.
    pub(crate) fn session_of_mut(&mut self, panel: Bottom) -> Option<&mut Session> {
        match panel {
            Bottom::Claude => self.claude.as_mut(),
            _ => self.terminal.as_mut(),
        }
    }

    /// Mutable [`Self::console_session`].
    pub(crate) fn console_session_mut(&mut self) -> Option<&mut Session> {
        self.session_of_mut(self.console)
    }

    /// Gives keyboard focus to the terminal.
    pub(crate) fn focus_terminal(&mut self) -> Task<Message> {
        self.terminal_focused = true;
        self.close_overlays();
        operation::focus(Id::new(TERMINAL_SINK))
    }

    /// Writes `bytes` to the session on screen and scrolls back to the live
    /// screen.
    pub(crate) fn terminal_input(&mut self, bytes: &[u8]) {
        if let Some(session) = self.console_session_mut() {
            session.scroll(i32::MIN / 2);
            session.write(bytes);
        }
    }

    /// Recomputes rows and columns of console panel `panel` for a new size;
    /// each console keeps its own grid, since the two may sit in different
    /// areas.
    pub(crate) fn terminal_resized(&mut self, panel: Bottom, size: Size) {
        let cols = ((size.width - 2.0 * TERMINAL_PADDING) / TERMINAL_CHAR_WIDTH)
            .floor()
            .max(10.0) as u16;
        let rows = ((size.height - 2.0 * TERMINAL_PADDING) / TERMINAL_LINE)
            .floor()
            .max(2.0) as u16;
        match panel {
            Bottom::Claude => self.claude_grid = Some((rows, cols)),
            _ => self.terminal_grid = Some((rows, cols)),
        }
        if let Some(session) = self.session_of_mut(panel) {
            session.resize(rows, cols);
        }
    }

    /// Routes a key press to the focused terminal. Returns `None` when the
    /// key is not the terminal's (IDE shortcuts), so normal handling runs.
    pub(crate) fn terminal_key(
        &mut self,
        key: &Key,
        modifiers: Modifiers,
        text: Option<&str>,
    ) -> Option<Task<Message>> {
        if modifiers.command() {
            return match key.as_ref() {
                Key::Character(c) if c.eq_ignore_ascii_case("v") => {
                    Some(iced::clipboard::read().map(Message::TerminalPaste))
                }
                Key::Character(c) if c.eq_ignore_ascii_case("k") => {
                    // ⌘K clears the screen like most macOS terminals.
                    self.terminal_input(b"\x0c");
                    Some(Task::none())
                }
                _ => None,
            };
        }
        let app_cursor = self
            .console_session()
            .is_some_and(|s| s.with_screen(vt100::Screen::application_cursor));
        let bytes = terminal::encode(key, modifiers, text, app_cursor)?;
        self.terminal_input(&bytes);
        Some(Task::none())
    }

    /// Pastes clipboard text, bracketed when the program asked for it.
    pub(crate) fn terminal_paste(&mut self, text: Option<String>) {
        let Some(text) = text else { return };
        let bracketed = self
            .console_session()
            .is_some_and(|s| s.with_screen(vt100::Screen::bracketed_paste));
        self.terminal_input(&terminal::paste(&text, bracketed));
    }

    /// Runs a command line in the terminal (starting it if needed), as if
    /// typed. Used by the Composer and test actions.
    pub(crate) fn run_in_terminal(&mut self, command: &str) -> Task<Message> {
        self.show_panel(Bottom::Terminal);
        if self.terminal.as_ref().is_none_or(Session::has_exited) {
            self.start_terminal();
        }
        self.terminal_input(format!("{command}\r").as_bytes());
        self.focus_terminal()
    }

    /// Key handling while the terminal has focus; returns `None` to fall
    /// through to the IDE's own shortcuts.
    pub(crate) fn terminal_keyboard(&mut self, event: &keyboard::Event) -> Option<Task<Message>> {
        if !self.terminal_focused || !self.is_shown(self.console) {
            return None;
        }
        let keyboard::Event::KeyPressed {
            key,
            modifiers,
            text,
            ..
        } = event
        else {
            return None;
        };
        self.terminal_key(key, *modifiers, text.as_deref())
    }
}
