//! Laravel support beyond plain PHP: the project's route names, views and
//! configuration keys, and the string arguments that refer to them
//! (`route('users.show')`, `view('users.index')`, `config('app.name')`,
//! `@include('partials.nav')`), for completion and go to declaration.
//! Eloquent conventions live with the symbol builder; the forwarding of
//! calls between models and their query builder lives here.

mod actions;
mod blade;
mod catalog;
mod columns;
mod container;
mod diagnose;
mod eloquent;
mod gutter;
mod key;
mod livewire;
mod model;
mod project;
mod rules;

pub use actions::*;
pub use blade::*;
use catalog::*;
pub use columns::*;
pub use container::container_binding;
pub use diagnose::*;
pub use eloquent::*;
pub use gutter::*;
pub use key::*;
pub use model::*;
pub use project::*;
pub use rules::is_validation_rule;
