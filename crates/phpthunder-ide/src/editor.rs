//! The code editor: open documents with undo history, and syntax highlighting.

mod blade;
mod document;
mod highlight;

pub use blade::*;
pub use document::*;
pub use highlight::*;
