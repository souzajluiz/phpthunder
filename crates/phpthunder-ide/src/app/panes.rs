//! Resizable panes: the width of the project sidebar and the height of the
//! bottom tool panel, changed by dragging the gap beside them. Pure state
//! with no widget types, so the drag math is testable without a window; the
//! view draws the handles (`ui/workbench.rs`) and `App::update` feeds the
//! cursor positions in.

use iced::Point;

/// Default width of the project sidebar, in pixels.
pub const SIDEBAR_DEFAULT: f32 = 280.0;
/// Narrowest the sidebar can be dragged.
pub const SIDEBAR_MIN: f32 = 180.0;
/// Widest the sidebar can be dragged.
pub const SIDEBAR_MAX: f32 = 640.0;
/// Default height of the bottom panel, in pixels.
pub const BOTTOM_DEFAULT: f32 = 260.0;
/// Lowest the bottom panel can be dragged: its header plus a few rows.
pub const BOTTOM_MIN: f32 = 120.0;
/// Default width of the right panel area, in pixels.
pub const RIGHT_DEFAULT: f32 = 460.0;
/// Narrowest the right panel area can be dragged.
pub const RIGHT_MIN: f32 = 260.0;
/// Room the editor keeps when a pane grows into it.
pub const EDITOR_MIN: f32 = 160.0;

/// A draggable gap between two panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Splitter {
    /// Between the project sidebar and the editor; drags horizontally.
    Sidebar,
    /// Between the editor and the bottom panel; drags vertically.
    Bottom,
    /// Between the editor and the right panel; drags horizontally.
    Right,
}

/// Current pane sizes, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneSizes {
    /// Width of the project sidebar.
    pub sidebar: f32,
    /// Height of the bottom tool panel.
    pub bottom: f32,
    /// Width of the right tool panel.
    pub right: f32,
}

impl Default for PaneSizes {
    fn default() -> Self {
        Self {
            sidebar: SIDEBAR_DEFAULT,
            bottom: BOTTOM_DEFAULT,
            right: RIGHT_DEFAULT,
        }
    }
}

/// Pane sizes as remembered in the session file, rounded to whole pixels so
/// the saved session stays comparable (`Eq`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SavedLayout {
    /// Width of the project sidebar.
    pub sidebar: u32,
    /// Height of the bottom tool panel.
    pub bottom: u32,
    /// Width of the right tool panel; missing in sessions saved before it
    /// existed.
    #[serde(default)]
    pub right: u32,
}

impl SavedLayout {
    /// The sizes from the settings file; `None` until every key is set
    /// (first start, or a settings file older than these keys).
    pub fn from_settings() -> Option<Self> {
        let settings = crate::settings::settings();
        Some(Self {
            sidebar: settings.sidebar_width?,
            bottom: settings.bottom_height?,
            right: settings.right_width?,
        })
    }

    /// Writes the sizes to the settings file (`App::update`, after a drag
    /// ends or a handle is reset).
    pub fn save(self) {
        crate::settings::save_panel_sizes(self.sidebar, self.bottom, self.right);
    }
}

impl From<PaneSizes> for SavedLayout {
    fn from(sizes: PaneSizes) -> Self {
        Self {
            sidebar: sizes.sidebar.round() as u32,
            bottom: sizes.bottom.round() as u32,
            right: sizes.right.round() as u32,
        }
    }
}

impl From<SavedLayout> for PaneSizes {
    /// Clamps on the way in, so a hand-edited session cannot hide a pane.
    fn from(saved: SavedLayout) -> Self {
        Self {
            sidebar: (saved.sidebar as f32).clamp(SIDEBAR_MIN, SIDEBAR_MAX),
            bottom: (saved.bottom as f32).max(BOTTOM_MIN),
            right: if saved.right == 0 {
                RIGHT_DEFAULT
            } else {
                (saved.right as f32).max(RIGHT_MIN)
            },
        }
    }
}

/// A drag in progress.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Drag {
    /// Which gap is being dragged.
    splitter: Splitter,
    /// Cursor position along the drag axis at the first move; `None` until
    /// then, since the press itself carries no position.
    origin: Option<f32>,
    /// Pane size when the drag started.
    start: f32,
    /// Largest size allowed for this drag (keeps the editor visible).
    max: f32,
}

/// Pane sizes plus the drag and hover state of their handles.
#[derive(Debug, Clone, Default)]
pub struct Panes {
    /// Current sizes.
    pub sizes: PaneSizes,
    /// Handle under the mouse, highlighted by the view.
    pub hovered: Option<Splitter>,
    /// Drag in progress, if any.
    drag: Option<Drag>,
}

impl Panes {
    /// Panes at the given sizes, as restored from the session.
    pub fn new(sizes: PaneSizes) -> Self {
        Self {
            sizes,
            ..Self::default()
        }
    }

    /// The splitter being dragged, if any. The view keeps its handle lit and
    /// `App::subscription` listens to the mouse only while this is `Some`.
    pub fn dragging(&self) -> Option<Splitter> {
        self.drag.map(|d| d.splitter)
    }

    /// Starts dragging `splitter`. `room` is how many pixels the editor next
    /// to the pane has right now; the pane may grow into all but
    /// [`EDITOR_MIN`] of it.
    pub fn begin(&mut self, splitter: Splitter, room: f32) {
        let (start, ceiling) = match splitter {
            Splitter::Sidebar => (self.sizes.sidebar, SIDEBAR_MAX),
            Splitter::Bottom => (self.sizes.bottom, f32::INFINITY),
            Splitter::Right => (self.sizes.right, f32::INFINITY),
        };
        // Never below the current size: a cramped editor still lets the
        // pane shrink, it just cannot grow.
        let max = (start + room - EDITOR_MIN)
            .min(ceiling)
            .max(start)
            .max(Self::min(splitter));
        self.drag = Some(Drag {
            splitter,
            origin: None,
            start,
            max,
        });
    }

    /// Follows the cursor during a drag. Returns whether a size changed, so
    /// the caller can skip work when nothing moved.
    pub fn moved(&mut self, cursor: Point) -> bool {
        let Some(drag) = &mut self.drag else {
            return false;
        };
        let along = match drag.splitter {
            Splitter::Sidebar | Splitter::Right => cursor.x,
            Splitter::Bottom => cursor.y,
        };
        let origin = *drag.origin.get_or_insert(along);
        // The bottom panel grows as the cursor moves up, the right one as
        // it moves left.
        let delta = match drag.splitter {
            Splitter::Sidebar => along - origin,
            Splitter::Bottom | Splitter::Right => origin - along,
        };
        let size = (drag.start + delta)
            .clamp(Self::min(drag.splitter), drag.max)
            .round();
        let slot = match drag.splitter {
            Splitter::Sidebar => &mut self.sizes.sidebar,
            Splitter::Bottom => &mut self.sizes.bottom,
            Splitter::Right => &mut self.sizes.right,
        };
        let changed = *slot != size;
        *slot = size;
        changed
    }

    /// Ends the drag (mouse released anywhere in the window). Returns
    /// whether a drag was in progress, so the caller saves the sizes once.
    pub fn end(&mut self) -> bool {
        self.drag.take().is_some()
    }

    /// Puts `splitter`'s pane back to its default size (double-click on the
    /// handle).
    pub fn reset(&mut self, splitter: Splitter) {
        let defaults = PaneSizes::default();
        match splitter {
            Splitter::Sidebar => self.sizes.sidebar = defaults.sidebar,
            Splitter::Bottom => self.sizes.bottom = defaults.bottom,
            Splitter::Right => self.sizes.right = defaults.right,
        }
    }

    /// Grows (positive `delta`) or shrinks `splitter`'s pane from the
    /// keyboard (`⌃⌥` arrows), within the same limits as a drag: `room` is
    /// the editor's current size along the axis. Returns the new size.
    pub fn nudge(&mut self, splitter: Splitter, delta: f32, room: f32) -> f32 {
        self.begin(splitter, room);
        let Some(drag) = self.drag.take() else {
            return 0.0;
        };
        let size = (drag.start + delta)
            .clamp(Self::min(splitter), drag.max)
            .round();
        self.set(splitter, size);
        size
    }

    /// Current size of `splitter`'s pane.
    pub fn size(&self, splitter: Splitter) -> f32 {
        match splitter {
            Splitter::Sidebar => self.sizes.sidebar,
            Splitter::Bottom => self.sizes.bottom,
            Splitter::Right => self.sizes.right,
        }
    }

    /// Sets `splitter`'s pane to `size` as is (restoring a maximized pane).
    pub fn set(&mut self, splitter: Splitter, size: f32) {
        match splitter {
            Splitter::Sidebar => self.sizes.sidebar = size,
            Splitter::Bottom => self.sizes.bottom = size,
            Splitter::Right => self.sizes.right = size,
        }
    }

    /// Smallest size of `splitter`'s pane.
    fn min(splitter: Splitter) -> f32 {
        match splitter {
            Splitter::Sidebar => SIDEBAR_MIN,
            Splitter::Bottom => BOTTOM_MIN,
            Splitter::Right => RIGHT_MIN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dragged(splitter: Splitter, room: f32, from: Point, to: Point) -> PaneSizes {
        let mut panes = Panes::default();
        panes.begin(splitter, room);
        panes.moved(from);
        panes.moved(to);
        panes.sizes
    }

    #[test]
    fn sidebar_should_follow_the_cursor_horizontally() {
        let sizes = dragged(
            Splitter::Sidebar,
            1000.0,
            Point::new(300.0, 50.0),
            Point::new(360.0, 80.0),
        );
        assert_eq!(sizes.sidebar, SIDEBAR_DEFAULT + 60.0);
    }

    #[test]
    fn bottom_panel_should_grow_when_dragged_up() {
        let sizes = dragged(
            Splitter::Bottom,
            1000.0,
            Point::new(0.0, 500.0),
            Point::new(0.0, 400.0),
        );
        assert_eq!(sizes.bottom, BOTTOM_DEFAULT + 100.0);
    }

    #[test]
    fn sidebar_should_stop_at_its_minimum() {
        let sizes = dragged(
            Splitter::Sidebar,
            1000.0,
            Point::new(300.0, 0.0),
            Point::new(0.0, 0.0),
        );
        assert_eq!(sizes.sidebar, SIDEBAR_MIN);
    }

    #[test]
    fn bottom_panel_should_leave_room_for_the_editor() {
        let sizes = dragged(
            Splitter::Bottom,
            300.0,
            Point::new(0.0, 500.0),
            Point::new(0.0, 0.0),
        );
        assert_eq!(sizes.bottom, BOTTOM_DEFAULT + 300.0 - EDITOR_MIN);
    }

    #[test]
    fn right_panel_should_grow_when_dragged_left() {
        let sizes = dragged(
            Splitter::Right,
            1000.0,
            Point::new(800.0, 0.0),
            Point::new(700.0, 0.0),
        );
        assert_eq!(sizes.right, RIGHT_DEFAULT + 100.0);
    }

    #[test]
    fn moving_without_a_drag_should_change_nothing() {
        let mut panes = Panes::default();
        assert!(!panes.moved(Point::new(10.0, 10.0)));
    }

    #[test]
    fn reset_should_restore_the_default_size() {
        let mut panes = Panes::new(PaneSizes {
            sidebar: 500.0,
            bottom: 400.0,
            right: RIGHT_DEFAULT,
        });
        panes.reset(Splitter::Sidebar);
        assert_eq!(panes.sizes.sidebar, SIDEBAR_DEFAULT);
    }

    #[test]
    fn saved_layout_should_clamp_a_hidden_sidebar() {
        let sizes = PaneSizes::from(SavedLayout {
            sidebar: 0,
            bottom: 300,
            right: 0,
        });
        assert_eq!(sizes.sidebar, SIDEBAR_MIN);
    }

    #[test]
    fn nudge_should_grow_the_pane_by_the_step() {
        let mut panes = Panes::default();
        panes.nudge(Splitter::Bottom, 40.0, 1000.0);
        assert_eq!(panes.sizes.bottom, BOTTOM_DEFAULT + 40.0);
    }

    #[test]
    fn nudge_should_stop_at_the_minimum() {
        let mut panes = Panes::default();
        assert_eq!(panes.nudge(Splitter::Sidebar, -1000.0, 1000.0), SIDEBAR_MIN);
    }

    #[test]
    fn nudge_should_leave_the_editor_room() {
        let mut panes = Panes::default();
        let size = panes.nudge(Splitter::Right, 1000.0, 500.0);
        assert_eq!(size, RIGHT_DEFAULT + 500.0 - EDITOR_MIN);
    }
}
