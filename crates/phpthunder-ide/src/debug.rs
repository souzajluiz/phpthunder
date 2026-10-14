//! Step debugging with Xdebug over the DBGp protocol: the test at the
//! caret or the current script runs under Xdebug (or web requests connect
//! while the IDE listens); breakpoints, stepping, the call stack, variables
//! and evaluation follow. Also Xdebug's profiler and a configuration check.

mod dbgp;
mod model;
mod profile;
mod session;
mod xml;

pub use dbgp::*;
pub use model::*;
pub use profile::*;
pub use session::*;
pub(crate) use xml::*;
