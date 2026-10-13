//! Prints the route names, views and config keys found in a Laravel
//! project. Read-only.
//!
//! Usage: `cargo run --example laravel -- /path/to/project`

use phpthunder::laravel::{KeyKind, scan_project};
use std::path::PathBuf;

/// Scans the project given as first argument and prints a summary.
fn main() {
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let Some(project) = scan_project(&root) else {
        eprintln!("{} is not a Laravel project", root.display());
        return;
    };
    for kind in [KeyKind::Route, KeyKind::View, KeyKind::Config] {
        let keys = project.keys(kind);
        println!("{kind:?}: {}", keys.len());
        for key in keys.iter().take(5) {
            println!("  {}  ({})", key.name, key.detail);
        }
    }
}
