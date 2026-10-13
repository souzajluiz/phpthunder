//! Laravel support beyond plain PHP: the project's route names, views and
//! configuration keys, and the string arguments that refer to them
//! (`route('users.show')`, `view('users.index')`, `config('app.name')`,
//! `@include('partials.nav')`), for completion and go to declaration.
//! Eloquent conventions live with the symbol builder; the forwarding of
//! calls between models and their query builder lives here.

mod blade;
mod eloquent;
mod key;
mod model;
mod project;

pub use blade::*;
pub use eloquent::*;
pub use key::*;
pub use model::*;
pub use project::*;
