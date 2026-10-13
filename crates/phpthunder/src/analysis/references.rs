//! Find usages: the word index narrows candidate files, then each occurrence
//! is resolved and kept only if it resolves to the searched declaration.

use super::*;
use crate::index::Index;
use crate::syntax::LineIndex;
use crate::syntax::{parse, text};
use std::collections::HashSet;
use std::path::Path;

/// Find usages of the declarations `defs`.
///
/// `load` returns current file content (open editor buffers win over disk).
/// Only files the word index lists for the name are parsed; each matching
/// identifier is resolved and kept if it lands on one of `defs`. Results are
/// sorted by path, then position. The result is only complete once the whole
/// project is indexed, which is why the server waits for indexing first.
pub fn find_references(
    index: &Index,
    defs: &[Definition],
    load: impl Fn(&Path) -> Option<String>,
) -> Vec<Usage> {
    let Some(first) = defs.first() else {
        return Vec::new();
    };
    let word = first.symbol.name.as_str();
    let wanted: HashSet<(&Path, usize)> = defs.iter().map(Definition::identity).collect();
    let mut out = Vec::new();
    for path in index.files_with_word(word) {
        let Some(src) = load(&path) else { continue };
        let tree = parse(&src);
        let analyzer = Analyzer::new(index, &src, &tree);
        let lines = LineIndex::new(&src);
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            let mut c = node.walk();
            stack.extend(node.named_children(&mut c));
            if node.kind() != "name" || !text(node, &src).eq_ignore_ascii_case(word) {
                continue;
            }
            let Some(target) = analyzer.target_of(node) else {
                continue;
            };
            let hit = resolve(index, &target)
                .iter()
                .any(|d| wanted.contains(&d.identity()));
            if hit {
                out.push(Usage {
                    path: path.clone(),
                    span: lines.span(&src, node.start_byte(), node.end_byte()),
                    is_declaration: wanted.contains(&(path.as_path(), node.start_byte())),
                });
            }
        }
    }
    out.sort_by(|a, b| {
        (&a.path, a.span.start.line, a.span.start.col).cmp(&(
            &b.path,
            b.span.start.line,
            b.span.start.col,
        ))
    });
    out
}
