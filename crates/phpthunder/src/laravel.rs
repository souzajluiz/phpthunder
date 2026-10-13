//! Laravel support beyond plain PHP: the project's route names, views and
//! configuration keys, and the string arguments that refer to them
//! (`route('users.show')`, `view('users.index')`, `config('app.name')`,
//! `@include('partials.nav')`), for completion and go to declaration.
//! Eloquent conventions live with the symbol builder.

mod key;
mod model;
mod project;

pub use key::*;
pub use model::*;
pub use project::*;
