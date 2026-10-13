//! Import cleanup: remove unused `use` statements and sort import blocks.

use super::*;
use crate::index::Index;
use crate::inspection::{Code, Edit, diagnostics};

/// "Optimize imports": removes unused `use` statements and sorts each
/// contiguous block of single-line imports alphabetically.
pub fn optimize_imports(index: &Index, src: &str) -> String {
    let unused: Vec<Edit> = diagnostics(index, src)
        .into_iter()
        .filter(|d| d.code == Code::UnusedImport)
        .flat_map(|d| crate::inspection::quick_fixes(index, src, &d, |_| None))
        .flat_map(|f| f.edits)
        .collect();
    let cleaned = apply_edits(src, &unused);
    sort_import_blocks(&cleaned)
}

/// Sorts runs of consecutive `use X;` lines (case-insensitive).
fn sort_import_blocks(src: &str) -> String {
    let mut lines: Vec<&str> = src.split('\n').collect();
    let is_use = |l: &str| {
        let t = l.trim_start();
        t.starts_with("use ") && t.ends_with(';') && !t.contains('{')
    };
    let mut i = 0;
    while i < lines.len() {
        if !is_use(lines[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < lines.len() && is_use(lines[i]) {
            i += 1;
        }
        let block = &mut lines[start..i];
        // `use function` and `use const` stay after class imports, as PSR-12 groups them.
        block.sort_by_key(|l| {
            let t = l.trim_start();
            let group = if t.starts_with("use function ") {
                1
            } else if t.starts_with("use const ") {
                2
            } else {
                0
            };
            (group, t.to_ascii_lowercase())
        });
    }
    lines.join("\n")
}
