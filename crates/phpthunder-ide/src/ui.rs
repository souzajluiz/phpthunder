//! Look and layout: the Thunder Purple theme, icons, and the workbench view.

mod compare;
mod console;
mod debug;
mod editor_marks;
mod file_icon;
mod git;
mod icons;
mod markdown;
mod panel;
mod popup;
mod preview;
mod theme;
mod welcome;
mod workbench;

pub use console::*;
pub(crate) use file_icon::*;
pub(crate) use git::*;
pub use icons::*;
pub(crate) use markdown::*;
pub(crate) use panel::*;
pub(crate) use popup::*;
pub use preview::*;
pub use theme::*;
pub use workbench::*;
