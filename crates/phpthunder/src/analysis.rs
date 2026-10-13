//! Code intelligence over one file and the index: resolution, type
//! inference, find usages and completion.

mod completion;
mod keywords;
mod matcher;
mod model;
mod references;
mod resolve;
mod sort;

pub use completion::*;
pub use keywords::*;
pub use matcher::*;
pub use model::*;
pub use references::*;
pub use resolve::*;
pub use sort::*;
