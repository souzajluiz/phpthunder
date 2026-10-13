//! phpthunder: a fast PHP language server.
//!
//! Pipeline: parse (tree-sitter) -> per-file declaration stubs -> project
//! index (declarations + word index) -> resolve, find usages, completion.

pub mod analysis;
pub mod index;
pub mod phpdoc;
pub mod server;
pub mod stubs;
pub mod syntax;
pub mod text;
pub mod workspace;
