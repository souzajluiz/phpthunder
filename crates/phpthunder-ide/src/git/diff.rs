//! Line diffs between `HEAD` and the current text.

use super::*;
use similar::{DiffOp, TextDiff};
use std::time::Duration;

/// Longest a diff may run before it settles for an approximate result, so
/// typing in a huge file never stalls.
const DIFF_TIMEOUT: Duration = Duration::from_millis(40);

/// Context lines around each hunk in the diff view.
const CONTEXT: usize = 3;

/// Diff operations between two texts, line by line.
fn line_ops<'a>(old: &'a str, new: &'a str) -> TextDiff<'a, 'a, str> {
    TextDiff::configure()
        .timeout(DIFF_TIMEOUT)
        .diff_lines(old, new)
}

/// Changed line ranges of `current` compared with `base` (the `HEAD`
/// version), for the gutter.
pub fn line_changes(base: &str, current: &str) -> Vec<LineChange> {
    line_ops(base, current)
        .ops()
        .iter()
        .filter_map(|op| match *op {
            DiffOp::Equal { .. } => None,
            DiffOp::Insert {
                new_index, new_len, ..
            } => Some(LineChange {
                line: new_index,
                count: new_len,
                mark: LineMark::Added,
            }),
            DiffOp::Delete { new_index, .. } => Some(LineChange {
                line: new_index,
                count: 0,
                mark: LineMark::Deleted,
            }),
            DiffOp::Replace {
                new_index, new_len, ..
            } => Some(LineChange {
                line: new_index,
                count: new_len,
                mark: LineMark::Modified,
            }),
        })
        .collect()
}

/// `current` with the change touching 0-based `line` put back as `base`
/// has it (`⌥⌘Z`, roll back changed lines); a deletion counts for the line
/// right below it. `None` when no change touches `line`.
pub fn revert_change_at(base: &str, current: &str, line: usize) -> Option<String> {
    let diff = line_ops(base, current);
    let op = diff.ops().iter().find(|op| {
        let new = op.new_range();
        match op {
            DiffOp::Equal { .. } => false,
            DiffOp::Delete { .. } => new.start == line,
            _ => new.contains(&line),
        }
    })?;
    let (old, new) = (op.old_range(), op.new_range());
    let current_lines: Vec<&str> = current.split_inclusive('\n').collect();
    let base_lines: Vec<&str> = base.split_inclusive('\n').collect();
    let mut out: String = current_lines.get(..new.start)?.concat();
    out.push_str(&base_lines.get(old)?.concat());
    out.push_str(&current_lines.get(new.end..)?.concat());
    Some(out)
}

/// Hunks of a diff from `old` to `new` with a few context lines, for the
/// Commit panel's diff view.
pub fn diff_lines(old: &str, new: &str) -> Vec<DiffLine> {
    let diff = line_ops(old, new);
    let clean = |s: &str| s.trim_end_matches(['\n', '\r']).to_string();
    let mut out = Vec::new();
    for group in diff.grouped_ops(CONTEXT) {
        let (Some(first), Some(last)) = (group.first(), group.last()) else {
            continue;
        };
        let old_start = first.old_range().start;
        let new_start = first.new_range().start;
        out.push(DiffLine {
            kind: DiffKind::Hunk,
            text: format!(
                "@@ -{},{} +{},{} @@",
                old_start + 1,
                last.old_range().end - old_start,
                new_start + 1,
                last.new_range().end - new_start
            ),
            new_line: None,
        });
        for op in &group {
            for i in op.old_range() {
                if !matches!(op, DiffOp::Equal { .. }) {
                    out.push(DiffLine {
                        kind: DiffKind::Removed,
                        text: clean(diff.old_slice(i).unwrap_or_default()),
                        new_line: None,
                    });
                }
            }
            for i in op.new_range() {
                let kind = if matches!(op, DiffOp::Equal { .. }) {
                    DiffKind::Context
                } else {
                    DiffKind::Added
                };
                out.push(DiffLine {
                    kind,
                    text: clean(diff.new_slice(i).unwrap_or_default()),
                    new_line: Some(i + 1),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revert_change_at_should_restore_a_modified_line() {
        let reverted = revert_change_at("a\nb\nc\n", "a\nB\nc\nd\n", 1);
        assert_eq!(reverted.as_deref(), Some("a\nb\nc\nd\n"));
    }

    #[test]
    fn revert_change_at_should_bring_back_deleted_lines() {
        let reverted = revert_change_at("a\nb\nc\n", "a\nc\n", 1);
        assert_eq!(reverted.as_deref(), Some("a\nb\nc\n"));
    }

    #[test]
    fn revert_change_at_should_drop_added_lines() {
        let reverted = revert_change_at("a\nc\n", "a\nb\nc\n", 1);
        assert_eq!(reverted.as_deref(), Some("a\nc\n"));
    }

    #[test]
    fn revert_change_at_should_skip_unchanged_lines() {
        assert_eq!(revert_change_at("a\nb\n", "a\nB\n", 0), None);
    }

    #[test]
    fn line_changes_should_mark_added_modified_and_deleted_lines() {
        let base = "a\nb\nc\nd\n";
        let current = "a\nB\nc\nnew\n";
        let changes = line_changes(base, current);
        assert_eq!(
            changes,
            [
                LineChange {
                    line: 1,
                    count: 1,
                    mark: LineMark::Modified
                },
                LineChange {
                    line: 3,
                    count: 1,
                    mark: LineMark::Modified
                },
            ]
        );
        assert_eq!(
            line_changes("a\nb\n", "a\n"),
            [LineChange {
                line: 1,
                count: 0,
                mark: LineMark::Deleted
            }]
        );
        assert_eq!(
            line_changes("a\n", "a\nb\n"),
            [LineChange {
                line: 1,
                count: 1,
                mark: LineMark::Added
            }]
        );
    }

    #[test]
    fn diff_lines_should_show_hunks_with_context() {
        let lines = diff_lines("a\nb\nc\n", "a\nx\nc\n");
        let kinds: Vec<DiffKind> = lines.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            [
                DiffKind::Hunk,
                DiffKind::Context,
                DiffKind::Removed,
                DiffKind::Added,
                DiffKind::Context
            ]
        );
        assert_eq!(lines[0].text, "@@ -1,3 +1,3 @@");
        assert_eq!(lines[3].new_line, Some(2));
    }
}
