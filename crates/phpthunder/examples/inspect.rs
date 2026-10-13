//! Indexes a PHP project and prints inspection results for its own files
//! (not `vendor/`), grouped by message kind. Read-only.
//!
//! Usage: `cargo run --release --example inspect -- /path/to/project [--all]`

use parking_lot::RwLock;
use phpthunder::index::{BUILTINS_SOURCE, Index, index_files, index_source, php_files};
use phpthunder::inspection::{Code, diagnostics};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

/// Groups a diagnostic for the summary table: its kind without the name.
fn kind(code: &Code) -> &'static str {
    match code {
        Code::UndefinedClass(_) => "undefined class",
        Code::UndefinedFunction(_) => "undefined function",
        Code::UndefinedMethod { .. } => "undefined method",
        Code::UndefinedProperty { .. } => "undefined property",
        Code::UnusedImport(_) => "unused import",
        Code::UndefinedVariable(_) => "undefined variable",
        Code::UnusedVariable(_) => "unused variable",
        Code::MissingReturn(_) => "missing return",
        Code::ReturnTypeMismatch { .. } => "return type mismatch",
        Code::External(_) => "external",
    }
}

/// Indexes the project, inspects every non-vendor PHP file and prints a
/// summary plus up to 15 examples per kind (all with `--all`).
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: inspect <project> [--all]")?,
    );
    let show_all = std::env::args().any(|a| a == "--all");
    let started = Instant::now();
    let index = RwLock::new(Index::default());
    index
        .write()
        .update(PathBuf::from("<builtins>"), index_source(BUILTINS_SOURCE));
    let files = php_files(&root);
    index_files(&index, &files, |_, _| {});
    let index = index.into_inner();
    println!("indexed {} files in {:.2?}", files.len(), started.elapsed());

    let started = Instant::now();
    let mut by_kind: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let own = files
        .iter()
        .filter(|p| !p.components().any(|c| c.as_os_str() == "vendor"));
    let mut inspected = 0;
    for path in own {
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        inspected += 1;
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        for d in diagnostics(&index, &src) {
            by_kind.entry(kind(&d.code)).or_default().push(format!(
                "{rel}:{} {}",
                d.span.start.line + 1,
                d.message
            ));
        }
    }
    println!(
        "inspected {inspected} project files in {:.2?}",
        started.elapsed()
    );
    for (kind, items) in &by_kind {
        println!("\n{kind}: {}", items.len());
        for item in items.iter().take(if show_all { usize::MAX } else { 15 }) {
            println!("  {item}");
        }
    }
    Ok(())
}
