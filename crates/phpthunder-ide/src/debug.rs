//! Step debugging with Xdebug over the DBGp protocol, centred on Pest: the
//! test at the caret runs under Xdebug, which connects back to the IDE;
//! breakpoints, stepping, the call stack, variables and evaluation follow.

mod dbgp;
mod model;
mod session;
mod xml;

pub use dbgp::*;
pub use model::*;
pub use session::*;
pub(crate) use xml::*;
