//! Front-end code next to PHP: JavaScript completion for scripts,
//! Alpine.js support and Tailwind CSS classes in templates.

mod alpine;
mod javascript;
mod tailwind;

pub use alpine::*;
pub use javascript::*;
pub use tailwind::*;
