//! Syntax layer: parsing, name resolution context, and source positions.
//!
//! The tree-sitter concrete syntax tree is the code model every other module
//! works on.

mod names;
mod parser;
mod position;

pub use names::*;
pub use parser::*;
pub use position::*;
