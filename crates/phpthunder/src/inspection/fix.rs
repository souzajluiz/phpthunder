//! Quick fixes offered for diagnostics (`⌥↵`).

use super::*;
use crate::index::Index;
use crate::syntax::{import_insertion_point, is_class_like, parse};
use std::path::Path;

/// Quick fixes for a diagnostic previously returned by [`diagnostics`] for
/// the same `src`.
///
/// `load` reads other files (open editor buffers first) when a fix edits a
/// different file, e.g. adding a method to another class.
pub fn quick_fixes(
    index: &Index,
    src: &str,
    diagnostic: &Diagnostic,
    load: impl Fn(&Path) -> Option<String>,
) -> Vec<Fix> {
    match &diagnostic.code {
        Code::UndefinedClass(fqn) => import_fixes(index, src, diagnostic, fqn),
        Code::UnusedImport(_) => vec![Fix {
            title: "Remove unused import".into(),
            edits: vec![remove_import_edit(src, diagnostic)],
            kind: FixKind::RemoveImport,
        }],
        Code::UndefinedMethod {
            class,
            name,
            is_static,
        } => add_member_fix(
            index,
            class,
            &method_stub(name, *is_static),
            &format!("Add method '{name}()' to {}", short(class)),
            FixKind::AddMethod {
                class: class.clone(),
                name: name.clone(),
            },
            load,
        )
        .into_iter()
        .collect(),
        Code::UndefinedProperty { class, name } => add_member_fix(
            index,
            class,
            &format!("\n    public mixed ${name};\n"),
            &format!("Add property '${name}' to {}", short(class)),
            FixKind::AddProperty {
                class: class.clone(),
                name: name.clone(),
            },
            load,
        )
        .into_iter()
        .collect(),
        Code::UnusedVariable(_) => remove_assignment_fix(src, diagnostic).into_iter().collect(),
        Code::SimplifyOrderBy => {
            let edits = crate::laravel::order_by_fix(src, diagnostic.start);
            if edits.is_empty() {
                Vec::new()
            } else {
                vec![Fix {
                    title: "Use orderByDesc()".into(),
                    edits,
                    kind: FixKind::UseOrderByDesc,
                }]
            }
        }
        Code::LooseNullComparison => {
            let op = &src[diagnostic.start..diagnostic.end];
            vec![Fix {
                title: format!("Use {op}="),
                edits: vec![Edit {
                    path: None,
                    start: diagnostic.end,
                    end: diagnostic.end,
                    text: "=".into(),
                }],
                kind: FixKind::UseStrictComparison,
            }]
        }
        Code::UnreachableCode | Code::SelfAssignment => vec![Fix {
            title: "Remove statement".into(),
            edits: vec![remove_lines_edit(src, diagnostic.start, diagnostic.end)],
            kind: FixKind::RemoveStatement,
        }],
        Code::RedundantElse => vec![Fix {
            title: "Remove redundant else".into(),
            edits: vec![unwrap_else_edit(src, diagnostic.start, diagnostic.end)],
            kind: FixKind::RemoveRedundantElse,
        }],
        Code::Rule(rule, detail) => rule
            .fix_title()
            .zip(rule_fix(
                *rule,
                src,
                diagnostic.start,
                diagnostic.end,
                detail,
            ))
            .map(|(title, edits)| Fix {
                title: title.into(),
                edits,
                kind: FixKind::Rule(*rule),
            })
            .into_iter()
            .collect(),
        Code::UndefinedFunction(_)
        | Code::EmptyBody
        | Code::DuplicateCase(_)
        | Code::DuplicateCondition
        | Code::IdenticalBranches
        | Code::SameOperands
        | Code::UnusedClosureUse(_)
        | Code::DuplicateMember(_)
        | Code::AbstractInConcreteClass
        | Code::ThisInStatic
        | Code::DuplicateArrayKey(_)
        | Code::AssignmentInCondition
        | Code::EmptyCatch
        | Code::UnusedPrivateMember(_)
        | Code::UndefinedVariable(_)
        | Code::MissingReturn(_)
        | Code::ReturnTypeMismatch { .. }
        | Code::UnknownLaravelKey { .. }
        | Code::External(_) => Vec::new(),
    }
}

/// An edit deleting `start..end` together with its line when nothing else
/// is on it (with the statement's `;` when it follows directly).
pub(crate) fn remove_lines_edit(src: &str, start: usize, end: usize) -> Edit {
    let end = if src[end..].starts_with(';') {
        end + 1
    } else {
        end
    };
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[end..].find('\n').map_or(src.len(), |i| end + i + 1);
    let alone = src[line_start..start].trim().is_empty() && src[end..line_end].trim().is_empty();
    if alone {
        Edit {
            path: None,
            start: line_start,
            end: line_end,
            text: String::new(),
        }
    } else {
        Edit {
            path: None,
            start,
            end,
            text: String::new(),
        }
    }
}

/// An edit replacing `else { body }` (at `start..end`) with the body's
/// statements, one level less indented, after the `if`.
fn unwrap_else_edit(src: &str, start: usize, end: usize) -> Edit {
    let clause = &src[start..end];
    let inner = match (clause.find('{'), clause.rfind('}')) {
        (Some(a), Some(b)) if a < b => &clause[a + 1..b],
        _ => "",
    };
    let body: Vec<&str> = inner
        .lines()
        .map(|l| {
            l.strip_prefix("    ")
                .or_else(|| l.strip_prefix('\t'))
                .unwrap_or(l)
        })
        .collect();
    let body = body.join("\n");
    let before = &src[..start];
    let ws = before.len() - before.trim_end().len();
    Edit {
        path: None,
        start: start - ws,
        end,
        text: body.trim_end().to_string(),
    }
}

/// Last segment of a fully qualified name.
fn short(fqn: &str) -> &str {
    fqn.rsplit('\\').next().unwrap_or(fqn)
}

/// "Import class" fixes: one per indexed class whose short name matches the
/// unresolved name.
fn import_fixes(index: &Index, src: &str, diagnostic: &Diagnostic, fqn: &str) -> Vec<Fix> {
    let written = &src[diagnostic.start..diagnostic.end];
    if written.contains('\\') {
        return Vec::new(); // qualified names are not fixed by an import
    }
    let tree = parse(src);
    let at = import_insertion_point(tree.root_node(), diagnostic.start);
    let wanted = short(fqn).to_ascii_lowercase();
    let mut fixes: Vec<Fix> = index
        .top_level_with_prefix(&wanted, 200)
        .into_iter()
        .filter(|(_, s)| s.kind.is_class_like() && short(&s.fqn).eq_ignore_ascii_case(&wanted))
        .map(|(_, s)| Fix {
            title: format!("Import class {}", s.fqn),
            edits: vec![Edit {
                path: None,
                start: at,
                end: at,
                text: format!("\nuse {};", s.fqn),
            }],
            kind: FixKind::ImportClass(s.fqn.clone()),
        })
        .collect();
    fixes.sort_by(|a, b| a.title.cmp(&b.title));
    fixes.dedup_by(|a, b| a.title == b.title);
    fixes
}

/// Deletes the reported `use` statement (with its line) or clause.
fn remove_import_edit(src: &str, diagnostic: &Diagnostic) -> Edit {
    let (mut start, mut end) = (diagnostic.start, diagnostic.end);
    let line_start = src[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = src[end..]
        .find('\n')
        .map(|i| end + i + 1)
        .unwrap_or(src.len());
    if src[line_start..start].trim().is_empty() && src[end..line_end].trim().is_empty() {
        (start, end) = (line_start, line_end);
    } else if src[end..].starts_with(',') {
        end += 1;
        end += src[end..].len() - src[end..].trim_start().len();
    }
    Edit {
        path: None,
        start,
        end,
        text: String::new(),
    }
}

/// Source for a new empty method.
fn method_stub(name: &str, is_static: bool) -> String {
    let modifier = if is_static { "public static" } else { "public" };
    format!("\n    {modifier} function {name}()\n    {{\n    }}\n")
}

/// Inserts `member` before the closing brace of `class`'s body, in the file
/// that declares it.
fn add_member_fix(
    index: &Index,
    class: &str,
    member: &str,
    title: &str,
    kind: FixKind,
    load: impl Fn(&Path) -> Option<String>,
) -> Option<Fix> {
    let (path, symbol) = index.classes(class).into_iter().next()?;
    let path = path.to_path_buf();
    let text = load(&path)?;
    let tree = parse(&text);
    let decl = tree
        .root_node()
        .descendant_for_byte_range(symbol.name_start, symbol.name_start)?
        .parent()?;
    if !is_class_like(decl.kind()) {
        return None;
    }
    let body = decl.child_by_field_name("body")?;
    let close = body.end_byte().checked_sub(1)?;
    Some(Fix {
        title: title.to_string(),
        edits: vec![Edit {
            path: Some(path),
            start: close,
            end: close,
            text: member.to_string(),
        }],
        kind,
    })
}

/// Turns `$x = expr;` (the diagnostic is on `$x`) into `expr;`, keeping any
/// side effect of the expression; `None` when the assignment has another
/// shape.
fn remove_assignment_fix(src: &str, diagnostic: &Diagnostic) -> Option<Fix> {
    let tree = parse(src);
    let var = tree
        .root_node()
        .descendant_for_byte_range(diagnostic.start, diagnostic.end)?;
    let var = if var.kind() == "variable_name" {
        var
    } else {
        var.parent()?
    };
    let assignment = var
        .parent()
        .filter(|p| p.kind() == "assignment_expression")?;
    let right = assignment.child_by_field_name("right")?;
    Some(Fix {
        title: "Remove unused variable (keep the value)".into(),
        edits: vec![Edit {
            path: None,
            start: assignment.start_byte(),
            end: right.start_byte(),
            text: String::new(),
        }],
        kind: FixKind::RemoveAssignment,
    })
}
