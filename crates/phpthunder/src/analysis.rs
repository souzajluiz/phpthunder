//! Code intelligence over one file and the index: resolution, type
//! inference, find usages and completion.

mod completion;
mod model;
mod references;
mod resolve;

pub use completion::*;
pub use model::*;
pub use references::*;
pub use resolve::*;
