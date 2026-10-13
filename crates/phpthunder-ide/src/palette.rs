//! Navigation popup: go to file / class / symbol / line, recent files and
//! search everywhere, with fuzzy matching.

mod fuzzy;
mod model;
mod view;

pub use fuzzy::*;
pub use model::*;
