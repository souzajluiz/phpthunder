//! The code editor: open documents with undo history, and syntax highlighting.

mod actions;
mod blade;
mod carets;
mod document;
mod highlight;
mod javascript;
mod lexer;
mod wrap;

pub use actions::*;
pub use blade::*;
pub use carets::*;
pub use document::*;
pub use highlight::*;
pub use lexer::*;
pub use wrap::*;
