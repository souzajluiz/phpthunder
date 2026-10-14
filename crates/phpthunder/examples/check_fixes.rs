//! Applies every quick fix and every context action offered in the given
//! PHP files (actions at every fifth byte) and reports, per kind, the ones
//! whose result no longer parses. Files that already have syntax errors
//! are skipped. Read-only.
//!
//! Usage: `cargo run --release --example check_fixes -- file.php …`

use phpthunder::index::Index;
use phpthunder::inspection::FixKind;
use std::collections::BTreeMap;
/// Runs the check over the files named on the command line.
fn main() {
    let index = Index::default();
    let mut offered = 0;
    let mut broken: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in std::env::args().skip(1) {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        if phpthunder::syntax::parse(&src).root_node().has_error() {
            continue;
        }
        for d in phpthunder::inspection::diagnostics(&index, &src) {
            for fix in phpthunder::inspection::quick_fixes(&index, &src, &d, |_| None) {
                offered += 1;
                let out = phpthunder::refactor::apply_edits(&src, &fix.edits);
                if phpthunder::syntax::parse(&out).root_node().has_error() {
                    let e = &fix.edits[0];
                    broken
                        .entry(format!("fix {:?}", fix.kind))
                        .or_default()
                        .push(format!(
                            "{path}:{} {:?} -> {:?}",
                            src[..e.start].lines().count(),
                            &src[e.start..e.end.min(e.start + 80)],
                            &e.text[..e.text.len().min(80)]
                        ));
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        let mut at = 0;
        while at < src.len() {
            if src.is_char_boundary(at) {
                for fix in phpthunder::intention::intentions(&index, &src, at, at) {
                    if !seen.insert(format!("{:?}", fix.edits)) {
                        continue;
                    }
                    offered += 1;
                    let out = phpthunder::refactor::apply_edits(&src, &fix.edits);
                    if phpthunder::syntax::parse(&out).root_node().has_error() {
                        let FixKind::Intention(a) = fix.kind else {
                            continue;
                        };
                        let e = &fix.edits[0];
                        broken
                            .entry(a.slug().to_string())
                            .or_default()
                            .push(format!(
                                "{path}:{} {:?} -> {:?}",
                                src[..e.start].lines().count(),
                                &src[e.start..e.end.min(e.start + 80)],
                                &e.text[..e.text.len().min(80)]
                            ));
                    }
                }
            }
            at += 5;
        }
    }
    println!("offered {offered}");
    for (k, v) in broken {
        println!("{k}: {}", v.len());
        for x in v.iter().take(3) {
            println!("  {x}");
        }
    }
}
