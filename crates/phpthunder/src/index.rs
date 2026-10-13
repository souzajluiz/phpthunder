//! The project index: declarations and identifiers of every PHP file, the
//! project scan that fills it, its on-disk cache, and the embedded PHP
//! built-ins.

mod builtins;
mod cache;
mod model;
mod scan;
mod store;

pub use builtins::*;
pub use cache::*;
pub use model::*;
pub use scan::*;
pub use store::*;
