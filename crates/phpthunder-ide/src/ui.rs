//! Look and layout: the Thunder Purple theme, icons, and the workbench view.

mod console;
mod debug;
mod file_icon;
mod git;
mod icons;
mod panel;
mod popup;
mod theme;
mod workbench;

pub use console::*;
pub(crate) use file_icon::*;
pub(crate) use git::*;
pub use icons::*;
pub(crate) use panel::*;
pub(crate) use popup::*;
pub use theme::*;
pub use workbench::*;
