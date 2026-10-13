//! Prints the keys (routes, views, config keys, middleware, …) found in a
//! Laravel project; with `--check`, also every unknown key its PHP and
//! Blade files name. Read-only.
//!
//! Usage: `cargo run --example laravel -- /path/to/project [--check]`

use phpthunder::laravel::{KeyKind, key_diagnostics, scan_project};
use std::path::PathBuf;

/// Scans the project given as first argument and prints a summary.
fn main() {
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let Some(project) = scan_project(&root) else {
        eprintln!("{} is not a Laravel project", root.display());
        return;
    };
    for &kind in KeyKind::ALL {
        let keys = project.keys(kind);
        println!("{kind:?}: {}", keys.len());
        for key in keys.iter().take(3) {
            println!("  {}  ({})", key.name, key.detail);
        }
    }
    if std::env::args().any(|a| a == "--check") {
        for dir in ["app", "routes", "resources/views", "config", "database"] {
            for entry in walkdir::WalkDir::new(root.join(dir)).into_iter().flatten() {
                let path = entry.path();
                if !path.to_string_lossy().ends_with(".php") {
                    continue;
                }
                let Ok(src) = std::fs::read_to_string(path) else {
                    continue;
                };
                for d in key_diagnostics(&project, &src) {
                    println!(
                        "{}:{}  {}",
                        path.display(),
                        d.span.start.line + 1,
                        d.message
                    );
                }
            }
        }
    }
}
