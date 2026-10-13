//! The project index: declarations and identifiers of every PHP file, the
//! project scan that fills it, and the embedded PHP built-ins.

mod builtins;
mod model;
mod scan;
mod store;

pub use builtins::*;
pub use model::*;
pub use scan::*;
pub use store::*;
