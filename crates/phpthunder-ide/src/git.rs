//! Git integration through the `git` command line: working tree status,
//! staging, commits, push and pull, branches, and line-level changes
//! against `HEAD` for the editor gutter.

mod blame;
mod command;
mod diff;
mod model;
mod status;

pub use blame::*;
pub use command::*;
pub use diff::*;
pub use model::*;
pub use status::*;
