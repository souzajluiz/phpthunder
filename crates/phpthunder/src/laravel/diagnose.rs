//! Laravel inspections: `orderBy('x', 'desc')` that reads better as
//! `orderByDesc('x')`, and warnings for strings that name a Laravel key the
//! project does not declare (`route('users.shwo')`, `view('missing')`,
//! `Storage::disk('s4')`). Only kinds whose catalog is complete are
//! checked: abilities, middleware and validation rules can come from
//! packages or the database, so they are never reported. Packages also
//! add routes and config keys, so those are reported only next to
//! declared siblings (`users.shwo` beside `users.show`) or, for config,
//! inside the framework's own files.

use super::*;
use crate::inspection::{Code, Diagnostic, Edit, Severity};
use crate::syntax::{LineIndex, parse, text};
use tree_sitter::Node;

/// Kinds reported when unknown. Inertia pages and Livewire components only
/// when the project has some, so other stacks stay quiet. Database
/// connections are left out: multi-tenant apps add them at runtime.
const CHECKED: &[KeyKind] = &[
    KeyKind::Route,
    KeyKind::View,
    KeyKind::Config,
    KeyKind::Translation,
    KeyKind::Guard,
    KeyKind::Disk,
    KeyKind::CacheStore,
    KeyKind::QueueConnection,
    KeyKind::LogChannel,
    KeyKind::Mailer,
    KeyKind::InertiaPage,
    KeyKind::Livewire,
];

/// Config files the framework ships; only keys under these are checked,
/// since packages merge their own config at runtime.
const FRAMEWORK_GROUPS: &[&str] = &[
    "app",
    "auth",
    "broadcasting",
    "cache",
    "concurrency",
    "cors",
    "database",
    "filesystems",
    "hashing",
    "logging",
    "mail",
    "queue",
    "services",
    "session",
    "view",
];

/// A warning for each string in `src` naming a key of a checked kind that
/// `project` lacks. Strings built at runtime (`"users.$x"`, `{$a}`) and
/// empty ones are skipped.
pub fn key_diagnostics(project: &LaravelProject, src: &str) -> Vec<Diagnostic> {
    let lines = LineIndex::new(src);
    let mut out = Vec::new();
    let mut last_start = None;
    for (i, c) in src.char_indices() {
        if c != '\'' && c != '"' {
            continue;
        }
        let Some(key) = key_at(src, i + 1).filter(|k| k.start == i + 1) else {
            continue;
        };
        if last_start == Some(key.start) {
            continue;
        }
        last_start = Some(key.start);
        if !CHECKED.contains(&key.kind)
            || project.keys(key.kind).is_empty()
            || is_dynamic(&key.value)
            || is_concatenated(src, i, key.end)
            || is_known(project, key.kind, &key.value)
        {
            continue;
        }
        out.push(Diagnostic {
            span: lines.span(src, key.start, key.end),
            start: key.start,
            end: key.end,
            severity: Severity::Warning,
            message: format!("Unknown {} '{}'", key.kind.label(), key.value),
            code: Code::UnknownLaravelKey {
                kind: key.kind,
                name: key.value,
            },
        });
    }
    out
}

/// Whether a string's value is computed or a pattern: empty,
/// interpolated, wildcard (`asaas.*`), or unterminated (still being typed).
fn is_dynamic(value: &str) -> bool {
    value.is_empty() || value.contains(['$', '{', '*', '\n'])
}

/// Whether the string between the quote at `quote` and `end` is joined
/// with `.` to other text (`'users.' . $action`), so it is only part of
/// the key.
fn is_concatenated(src: &str, quote: usize, end: usize) -> bool {
    let after = src.get(end + 1..).unwrap_or_default().trim_start();
    let before = src[..quote].trim_end();
    after.starts_with('.') && !after.starts_with("..") || before.ends_with('.')
}

/// Whether a declared key shares the first dotted segment of `name`.
fn has_sibling(keys: &[LaravelKey], name: &str) -> bool {
    let Some((group, _)) = name.split_once('.') else {
        return false;
    };
    let prefix = format!("{group}.");
    keys.iter().any(|k| k.name.starts_with(&prefix))
}

/// Whether `name` is declared, allowing what the catalog cannot list:
/// config keys nested deeper than scanned or holding whole arrays, JSON
/// translation strings, and namespaced keys (`package::file.key`).
fn is_known(project: &LaravelProject, kind: KeyKind, name: &str) -> bool {
    if name.contains("::") || project.find(kind, name).is_some() {
        return true;
    }
    let keys = project.keys(kind);
    match kind {
        // Packages declare routes too (`login`, `filament.*`): only a
        // name next to declared siblings is a likely typo.
        KeyKind::Route => !has_sibling(keys, name),
        // Only the framework's files are checked; `app.providers.x.y`
        // past the scanned depth is known when a declared key is a dotted
        // prefix of it.
        KeyKind::Config => {
            let group = name.split('.').next().unwrap_or_default();
            !FRAMEWORK_GROUPS.contains(&group)
                || keys.iter().any(|k| {
                    name.strip_prefix(k.name.as_str())
                        .is_some_and(|rest| rest.starts_with('.'))
                })
        }
        // Only `group.key` forms whose group exists are checked; other
        // strings are JSON translations (`__('Log in')`).
        KeyKind::Translation => name.contains(' ') || !has_sibling(keys, name),
        _ => false,
    }
}

/// `orderBy(col, 'desc')` calls, offered as `orderByDesc(col)`.
pub fn order_by_diagnostics(src: &str) -> Vec<Diagnostic> {
    if !src.contains("orderBy") {
        return Vec::new();
    }
    let tree = parse(src);
    let lines = LineIndex::new(src);
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        let mut c = node.walk();
        stack.extend(node.named_children(&mut c));
        let Some((name, _, _)) = desc_order_by(node, src) else {
            continue;
        };
        let end = node
            .child_by_field_name("arguments")
            .map_or(name.end_byte(), |a| a.end_byte());
        out.push(Diagnostic {
            span: lines.span(src, name.start_byte(), end),
            start: name.start_byte(),
            end,
            severity: Severity::Warning,
            message: "orderBy(…, 'desc') can be orderByDesc(…)".to_string(),
            code: Code::SimplifyOrderBy,
        });
    }
    out
}

/// For a call `orderBy(first, 'desc')`: its name node and the two argument
/// nodes.
fn desc_order_by<'t>(call: Node<'t>, src: &str) -> Option<(Node<'t>, Node<'t>, Node<'t>)> {
    if !matches!(
        call.kind(),
        "member_call_expression" | "scoped_call_expression" | "nullsafe_member_call_expression"
    ) {
        return None;
    }
    let name = call.child_by_field_name("name")?;
    if text(name, src) != "orderBy" {
        return None;
    }
    let args = call.child_by_field_name("arguments")?;
    let mut c = args.walk();
    let list: Vec<Node> = args
        .named_children(&mut c)
        .filter(|a| a.kind() == "argument")
        .collect();
    let [first, second] = list.as_slice() else {
        return None;
    };
    let direction = text(*second, src).trim_matches(['\'', '"']);
    direction
        .eq_ignore_ascii_case("desc")
        .then_some((name, *first, *second))
}

/// Edits turning the `orderBy(…, 'desc')` reported at `start` into
/// `orderByDesc(…)`. Empty when the call is no longer there.
pub fn order_by_fix(src: &str, start: usize) -> Vec<Edit> {
    let tree = parse(src);
    let Some(mut node) = tree.root_node().descendant_for_byte_range(start, start) else {
        return Vec::new();
    };
    loop {
        if let Some((name, first, second)) = desc_order_by(node, src) {
            return vec![
                Edit {
                    path: None,
                    start: first.end_byte(),
                    end: second.end_byte(),
                    text: String::new(),
                },
                Edit {
                    path: None,
                    start: name.start_byte(),
                    end: name.end_byte(),
                    text: "orderByDesc".to_string(),
                },
            ];
        }
        let Some(parent) = node.parent() else {
            return Vec::new();
        };
        node = parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn project() -> LaravelProject {
        let key = |name: &str| LaravelKey {
            name: name.to_string(),
            path: PathBuf::new(),
            pos: Default::default(),
            detail: String::new(),
        };
        let mut p = LaravelProject::default();
        p.add(KeyKind::Route, [key("users.show")]);
        p.add(KeyKind::Config, [key("app.name"), key("app.providers")]);
        p.add(KeyKind::Translation, [key("auth.failed")]);
        p.add(KeyKind::Disk, [key("local")]);
        p
    }

    fn names(src: &str) -> Vec<String> {
        key_diagnostics(&project(), src)
            .into_iter()
            .map(|d| src[d.start..d.end].to_string())
            .collect()
    }

    #[test]
    fn key_diagnostics_should_report_unknown_keys() {
        let src = "<?php route('users.shwo'); Storage::disk('s4'); config('app.nme');";
        assert_eq!(names(src), ["users.shwo", "s4", "app.nme"]);
    }

    #[test]
    fn key_diagnostics_should_accept_declared_and_dynamic_keys() {
        let src = "<?php route('users.show'); route(\"users.$x\"); route('users.' . $a); route('login'); config('app.providers.0.x'); config('forms.x'); __('Log in'); __('other.key'); view('x');";
        assert!(names(src).is_empty());
    }

    #[test]
    fn key_diagnostics_should_report_missing_translation_in_known_group() {
        assert_eq!(names("<?php __('auth.faild');"), ["auth.faild"]);
    }

    #[test]
    fn order_by_should_be_reported_and_fixed() {
        let src = "<?php User::query()->orderBy('name', 'DESC')->get();";
        let found = order_by_diagnostics(src);
        assert_eq!(found.len(), 1);
        let mut edits = order_by_fix(src, found[0].start);
        edits.sort_by_key(|e| std::cmp::Reverse(e.start));
        let mut fixed = src.to_string();
        for e in edits {
            fixed.replace_range(e.start..e.end, &e.text);
        }
        assert_eq!(fixed, "<?php User::query()->orderByDesc('name')->get();");
    }

    #[test]
    fn order_by_ascending_should_not_be_reported() {
        assert!(order_by_diagnostics("<?php $q->orderBy('name', 'asc');").is_empty());
    }
}
