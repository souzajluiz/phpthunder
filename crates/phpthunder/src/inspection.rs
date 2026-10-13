//! Inspections: problems found in a file, and quick fixes for them.

mod fix;
mod flow;
mod inspect;
mod model;

pub use fix::*;
pub(crate) use flow::*;
pub use inspect::*;
pub use model::*;
