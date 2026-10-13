//! Where tool panels live: each one is docked below or right of the
//! editor, both areas can be open at once (Claude on the right while the
//! terminal runs below), and the user moves a panel with the button in its
//! header. The right-docked set is kept in the session.

use super::*;
use iced::Task;

impl App {
    /// The side `panel` is docked to.
    pub(crate) fn dock_of(&self, panel: Bottom) -> Dock {
        if self.session.right_panels.contains(&panel) {
            Dock::Right
        } else {
            Dock::Bottom
        }
    }

    /// Shows `panel` in the area of its side, replacing the tab shown there.
    pub(crate) fn show_panel(&mut self, panel: Bottom) {
        match self.dock_of(panel) {
            Dock::Bottom => self.bottom = Some(panel),
            Dock::Right => self.right = Some(panel),
        }
        if matches!(panel, Bottom::Terminal | Bottom::Claude) {
            self.console = panel;
        }
    }

    /// Hides `panel` if it is shown, closing its area.
    pub(crate) fn hide_panel(&mut self, panel: Bottom) {
        if self.bottom == Some(panel) {
            self.bottom = None;
        }
        if self.right == Some(panel) {
            self.right = None;
        }
    }

    /// Whether `panel` is on screen, in either area.
    pub(crate) fn is_shown(&self, panel: Bottom) -> bool {
        self.bottom == Some(panel) || self.right == Some(panel)
    }

    /// The panel shown on `dock`'s side.
    pub(crate) fn shown_at(&self, dock: Dock) -> Option<Bottom> {
        match dock {
            Dock::Bottom => self.bottom,
            Dock::Right => self.right,
        }
    }

    /// Hides the panel area on `dock`'s side.
    pub(crate) fn close_panels(&mut self, dock: Dock) {
        match dock {
            Dock::Bottom => self.bottom = None,
            Dock::Right => self.right = None,
        }
        if !self.is_shown(self.console) {
            self.terminal_focused = false;
        }
    }

    /// Moves `panel` to the other side and shows it there; remembered in
    /// the session.
    pub(crate) fn move_panel(&mut self, panel: Bottom) -> Task<Message> {
        let shown = self.is_shown(panel);
        self.hide_panel(panel);
        match self.dock_of(panel) {
            Dock::Bottom => self.session.right_panels.push(panel),
            Dock::Right => self.session.right_panels.retain(|p| *p != panel),
        }
        crate::settings::save_session(&self.session);
        if shown {
            self.show_panel(panel);
        }
        Task::none()
    }
}
