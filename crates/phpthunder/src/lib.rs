//! phpthunder: PHP code intelligence engine.
//!
//! Pipeline: parse (tree-sitter) -> per-file declaration stubs -> project
//! index (declarations + word index) -> resolution, find usages, completion,
//! inspections and refactorings. Used by the language server and the IDE.

pub mod analysis;
pub mod index;
pub mod inspection;
pub mod laravel;
pub mod refactor;
pub mod symbol;
pub mod syntax;
