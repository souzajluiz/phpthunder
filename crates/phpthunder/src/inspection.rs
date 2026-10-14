//! Inspections: problems found in a file, and quick fixes for them.

mod bugs;
mod fix;
mod flow;
mod inspect;
mod model;
mod phpunit;
mod psr;
mod rewrite;
mod rules;
mod semantic;
mod simplify;
mod style;

pub(crate) use bugs::*;
pub use fix::*;
pub(crate) use flow::*;
pub use inspect::*;
pub use model::*;
pub(crate) use phpunit::*;
pub(crate) use psr::*;
pub(crate) use rewrite::*;
pub use rules::Rule;
pub(crate) use rules::*;
pub(crate) use semantic::*;
pub(crate) use simplify::*;
pub(crate) use style::*;
