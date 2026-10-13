//! Live templates: abbreviations expanded with Tab (`fore` -> a `foreach`
//! loop) and a `⌘J` list to insert them by name.

mod expand;
mod model;
mod postfix;

pub use expand::*;
pub use model::*;
pub use postfix::*;
