//! The code editor: open documents with undo history, and syntax highlighting.

mod blade;
mod document;
mod highlight;
mod lexer;

pub use blade::*;
pub use document::*;
pub use highlight::*;
pub use lexer::*;
