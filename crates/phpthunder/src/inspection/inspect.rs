//! Code inspections: problems found by resolving every name in a file against
//! the project index.
//!
//! Inspections are conservative: a symbol is reported missing only when the
//! index is complete enough to know (its namespace exists in the project, or
//! the PHP built-ins are indexed), so code that depends on packages not
//! installed yet does not light up with false errors.

use super::*;
use crate::analysis::{Analyzer, Target};
use crate::index::Index;
use crate::symbol::{Kind, MemberTag};
use crate::syntax::LineIndex;
use crate::syntax::{NameContext, UseKind, context_at, is_class_like, parse, text, use_clauses};
use std::collections::HashSet;
use tree_sitter::Node;

/// Variables PHP defines in every scope.
const SUPERGLOBALS: &[&str] = &[
    "$this",
    "$GLOBALS",
    "$_SERVER",
    "$_GET",
    "$_POST",
    "$_FILES",
    "$_COOKIE",
    "$_SESSION",
    "$_REQUEST",
    "$_ENV",
    "$http_response_header",
    "$argc",
    "$argv",
];

/// Calls that create variables PHP cannot see statically; a function that
/// uses them is skipped by the undefined-variable inspection.
const DYNAMIC_SCOPE_CALLS: &[&str] = &[
    "extract",
    "compact",
    "get_defined_vars",
    "eval",
    "parse_str",
];

/// Whether the index holds the generated PHP built-ins, which makes
/// "undefined global symbol" checks trustworthy.
fn has_builtins(index: &Index) -> bool {
    !index.classes("Exception").is_empty() && !index.functions(&["strlen".to_string()]).is_empty()
}

/// Whether a missing class FQN should be reported: its root namespace must
/// be known to the index (so a missing vendor package is not an error).
fn namespace_known(index: &Index, fqn: &str, roots: &HashSet<String>) -> bool {
    match fqn.split_once('\\') {
        Some((root, _)) => roots.contains(&root.to_ascii_lowercase()),
        None => has_builtins(index),
    }
}

/// Lowercased first namespace segment of every indexed class.
fn known_roots(index: &Index) -> HashSet<String> {
    let mut roots = HashSet::new();
    for (_, symbols) in index.files() {
        for s in symbols.iter().filter(|s| s.kind.is_class_like()) {
            if let Some((root, _)) = s.fqn.split_once('\\') {
                roots.insert(root.to_ascii_lowercase());
            }
        }
    }
    roots
}

/// Whether every class in the hierarchy of `class` is indexed and none has
/// magic accessors, i.e. a missing member is a real problem.
fn hierarchy_closed(index: &Index, class: &str, tag: MemberTag) -> bool {
    let magic: &[&str] = match tag {
        MemberTag::Method => &["__call", "__callstatic"],
        MemberTag::Property => &["__get"],
        MemberTag::Constant => &[],
    };
    index.hierarchy(class).iter().all(|cls| {
        let found = index.classes(cls);
        !found.is_empty()
            && !cls.eq_ignore_ascii_case("stdClass")
            && magic
                .iter()
                .all(|m| index.find_member(cls, MemberTag::Method, m).is_empty())
    })
}

/// Runs every inspection over `src` and returns problems sorted by position.
///
/// Called by the IDE after edits (debounced) and by the language server on
/// document changes. Pure function of `src` and the index.
pub fn diagnostics(index: &Index, src: &str) -> Vec<Diagnostic> {
    let tree = parse(src);
    let analyzer = Analyzer::new(index, src, &tree);
    let lines = LineIndex::new(src);
    let roots = known_roots(index);
    let builtins = has_builtins(index);
    let mut out = Vec::new();
    let mut push = |node: Node, severity: Severity, message: String, code: Code| {
        out.push(Diagnostic {
            span: lines.span(src, node.start_byte(), node.end_byte()),
            start: node.start_byte(),
            end: node.end_byte(),
            severity,
            message,
            code,
        });
    };

    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
        if node.kind() != "name" {
            continue;
        }
        // Only the last segment of a qualified name is classified, so each
        // reference is checked once.
        if node.parent().is_some_and(|p| p.kind() == "namespace_name") {
            continue;
        }
        if is_declaration_name(node) {
            continue;
        }
        let Some(target) = analyzer.target_of(node) else {
            continue;
        };
        let report_node = node
            .parent()
            .filter(|p| matches!(p.kind(), "qualified_name" | "relative_name"))
            .unwrap_or(node);
        match target {
            Target::Class(fqn) => {
                if index.classes(&fqn).is_empty() && namespace_known(index, &fqn, &roots) {
                    let short = fqn.rsplit('\\').next().unwrap_or(&fqn);
                    push(
                        report_node,
                        Severity::Error,
                        format!("Undefined class '{short}'"),
                        Code::UndefinedClass(fqn),
                    );
                }
            }
            Target::Function(candidates) => {
                if builtins && index.functions(&candidates).is_empty() {
                    let name = text(report_node, src).to_string();
                    push(
                        report_node,
                        Severity::Error,
                        format!("Undefined function '{name}'"),
                        Code::UndefinedFunction(name),
                    );
                }
            }
            Target::Member { class, tag, name } => {
                if tag == MemberTag::Constant || !index.find_member(&class, tag, &name).is_empty() {
                    continue;
                }
                let is_enum_builtin = tag == MemberTag::Method
                    && matches!(
                        name.to_ascii_lowercase().as_str(),
                        "cases" | "from" | "tryfrom"
                    )
                    && index
                        .classes(&class)
                        .iter()
                        .any(|(_, s)| s.kind == Kind::Enum);
                if is_enum_builtin || !hierarchy_closed(index, &class, tag) {
                    continue;
                }
                let short = class.rsplit('\\').next().unwrap_or(&class).to_string();
                let is_static = node
                    .parent()
                    .is_some_and(|p| p.kind() == "scoped_call_expression");
                let (message, code) = match tag {
                    MemberTag::Method => (
                        format!("Method '{name}' not found in {short}"),
                        Code::UndefinedMethod {
                            class,
                            name,
                            is_static,
                        },
                    ),
                    _ => (
                        format!("Property '${name}' not found in {short}"),
                        Code::UndefinedProperty { class, name },
                    ),
                };
                push(node, Severity::Warning, message, code);
            }
        }
    }

    unused_imports(&tree, src, &mut push);
    undefined_variables(tree.root_node(), src, &mut push);
    out.sort_by_key(|d| (d.start, d.severity));
    out
}

/// Whether `name` is the name being declared (class, function, method,
/// constant, enum case), which by definition resolves to itself; checking
/// it would only report files that are not indexed yet.
fn is_declaration_name(name: Node) -> bool {
    name.parent().is_some_and(|p| {
        let declares = is_class_like(p.kind())
            || matches!(
                p.kind(),
                "function_definition" | "method_declaration" | "enum_case" | "const_element"
            );
        declares
            && p.child_by_field_name("name")
                .or_else(|| p.named_child(0))
                .is_some_and(|n| n.id() == name.id())
    })
}

/// Reports `use` clauses whose alias is never referenced (in code or docblocks).
fn unused_imports(
    tree: &tree_sitter::Tree,
    src: &str,
    push: &mut impl FnMut(Node, Severity, String, Code),
) {
    let root = tree.root_node();
    let mut used: HashSet<String> = HashSet::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let mut cursor = node.walk();
        match node.kind() {
            "namespace_use_declaration" => continue,
            "name" | "qualified_name" => {
                let first = text(node, src)
                    .trim_start_matches('\\')
                    .split('\\')
                    .next()
                    .unwrap_or("");
                used.insert(first.to_ascii_lowercase());
            }
            "comment" => {
                for word in
                    text(node, src).split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '\\'))
                {
                    let first = word
                        .trim_start_matches('\\')
                        .split('\\')
                        .next()
                        .unwrap_or("");
                    used.insert(first.to_ascii_lowercase());
                }
            }
            _ => {}
        }
        stack.extend(node.named_children(&mut cursor));
    }
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let mut cursor = node.walk();
        if node.kind() == "namespace_use_declaration" {
            let clauses = use_clauses(node, src);
            for clause in &clauses {
                if clause.kind == UseKind::Const
                    || used.contains(&clause.alias.to_ascii_lowercase())
                {
                    continue;
                }
                // A single-clause statement is reported as a whole so the fix
                // removes the line.
                let target = if clauses.len() == 1 {
                    node
                } else {
                    clause.name_node
                };
                push(
                    target,
                    Severity::Warning,
                    format!("Unused import '{}'", clause.fqn),
                    Code::UnusedImport,
                );
            }
            continue;
        }
        stack.extend(node.named_children(&mut cursor));
    }
}

/// Reports variables read in a function body that are never assigned,
/// passed by reference, or otherwise introduced in that function.
fn undefined_variables(root: Node, src: &str, push: &mut impl FnMut(Node, Severity, String, Code)) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
        if matches!(
            node.kind(),
            "function_definition" | "method_declaration" | "anonymous_function" | "arrow_function"
        ) {
            check_function_scope(node, src, push);
        }
    }
}

/// The undefined-variable inspection for one function-like scope.
fn check_function_scope(
    function: Node,
    src: &str,
    push: &mut impl FnMut(Node, Severity, String, Code),
) {
    // Arrow functions see the enclosing scope implicitly: skip them.
    if function.kind() == "arrow_function" || function.child_by_field_name("body").is_none() {
        return;
    }
    let mut defined: HashSet<String> = SUPERGLOBALS.iter().map(|s| (*s).to_string()).collect();
    let mut reads: Vec<Node> = Vec::new();
    let mut dynamic = false;
    if let Some(params) = function.child_by_field_name("parameters") {
        let mut c = params.walk();
        for p in params.named_children(&mut c) {
            if let Some(name) = p.child_by_field_name("name") {
                defined.insert(text(name, src).to_string());
            }
        }
    }
    // Closure `use ($a, &$b)` imports.
    let mut c = function.walk();
    for child in function.named_children(&mut c) {
        if child.kind() == "anonymous_function_use_clause" {
            collect_variables(child, src, &mut defined);
        }
    }
    let mut stack: Vec<Node> = function.child_by_field_name("body").into_iter().collect();
    while let Some(node) = stack.pop() {
        let mut cursor = node.walk();
        match node.kind() {
            // Nested functions have their own scope (checked separately).
            "function_definition"
            | "anonymous_function"
            | "arrow_function"
            | "class_declaration" => {
                if node.kind() == "anonymous_function" {
                    // Variables imported by a closure must exist outside it.
                    let mut cc = node.walk();
                    for child in node.named_children(&mut cc) {
                        if child.kind() == "anonymous_function_use_clause" {
                            let mut uc = child.walk();
                            reads.extend(
                                child
                                    .named_children(&mut uc)
                                    .filter(|v| v.kind() == "variable_name"),
                            );
                        }
                    }
                }
                continue;
            }
            "assignment_expression"
            | "reference_assignment_expression"
            | "augmented_assignment_expression" => {
                if let Some(left) = node.child_by_field_name("left") {
                    collect_variables(left, src, &mut defined);
                }
                if node.kind() == "augmented_assignment_expression" {
                    // `$a .= 'x'` reads $a too, but PHP only warns; keep quiet.
                }
            }
            "foreach_statement" => {
                let mut fc = node.walk();
                for child in node.named_children(&mut fc).skip(1) {
                    if child.kind() != "compound_statement" && child.kind() != "colon_block" {
                        collect_variables(child, src, &mut defined);
                    }
                }
            }
            "catch_clause" => {
                if let Some(name) = node.child_by_field_name("name") {
                    defined.insert(text(name, src).to_string());
                }
            }
            "global_declaration" | "function_static_declaration" | "unset_statement" => {
                collect_variables(node, src, &mut defined);
            }
            "arguments" => {
                // By-reference out-parameters (`preg_match($re, $s, $m)`) define
                // variables; without signatures, any bare variable argument counts.
                let mut ac = node.walk();
                for arg in node.named_children(&mut ac) {
                    if let Some(v) = arg.named_child(0).filter(|v| v.kind() == "variable_name") {
                        defined.insert(text(v, src).to_string());
                    }
                }
            }
            "function_call_expression" => {
                let name = node
                    .child_by_field_name("function")
                    .map(|f| text(f, src).trim_start_matches('\\').to_ascii_lowercase());
                if name.is_some_and(|n| DYNAMIC_SCOPE_CALLS.contains(&n.as_str())) {
                    dynamic = true;
                }
            }
            "include_expression"
            | "include_once_expression"
            | "require_expression"
            | "require_once_expression" => dynamic = true,
            "variable_name" => reads.push(node),
            "dynamic_variable_name" => dynamic = true,
            _ => {}
        }
        stack.extend(node.named_children(&mut cursor));
    }
    if dynamic {
        return;
    }
    let mut reported = HashSet::new();
    reads.sort_by_key(|n| n.start_byte());
    for var in reads {
        let name = text(var, src);
        if !defined.contains(name) && reported.insert(name.to_string()) {
            push(
                var,
                Severity::Warning,
                format!("Undefined variable '{name}'"),
                Code::UndefinedVariable(name.to_string()),
            );
        }
    }
}

/// Adds every `$variable` inside `node` to `out` (assignment targets,
/// destructuring, `global`/`static` lists).
fn collect_variables(node: Node, src: &str, out: &mut HashSet<String>) {
    if node.kind() == "variable_name" {
        out.insert(text(node, src).to_string());
        return;
    }
    let mut c = node.walk();
    for child in node.named_children(&mut c) {
        collect_variables(child, src, out);
    }
}

/// Name context at `byte`, exposed for callers composing their own fixes.
pub fn name_context(src: &str, byte: usize) -> NameContext {
    let tree = parse(src);
    context_at(tree.root_node(), src, byte)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::index_source;
    use std::path::{Path, PathBuf};

    fn index_with(files: &[(&str, &str)]) -> Index {
        let mut index = Index::default();
        for (path, src) in files {
            index.update(PathBuf::from(path), index_source(src));
        }
        index
    }

    const BUILTINS: &str = "<?php class Exception {} function strlen(string $s): int {} function preg_match($p, $s, &$m = null) {} function extract(array &$a) {}";

    fn codes(index: &Index, src: &str) -> Vec<Code> {
        diagnostics(index, src)
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn diagnostics_should_report_undefined_class_in_known_namespace() {
        let index = index_with(&[
            ("/b.php", BUILTINS),
            ("/u.php", "<?php namespace App\\Models; class User {}"),
        ]);
        let src = "<?php namespace App\\Http; use App\\Models\\User; new User(); new Usr(); new \\Vendor\\Pkg\\Thing();";
        assert_eq!(
            codes(&index, src),
            vec![Code::UndefinedClass("App\\Http\\Usr".into())]
        );
    }

    #[test]
    fn diagnostics_should_report_undefined_function_only_with_builtins() {
        let src = "<?php strlen('x'); nope();";
        let without = index_with(&[]);
        assert!(codes(&without, src).is_empty());
        let with = index_with(&[("/b.php", BUILTINS)]);
        assert_eq!(
            codes(&with, src),
            vec![Code::UndefinedFunction("nope".into())]
        );
    }

    #[test]
    fn diagnostics_should_report_missing_member_on_closed_hierarchy() {
        let index = index_with(&[
            ("/b.php", BUILTINS),
            (
                "/a.php",
                "<?php class A { function ok() {} } class M { function __call($n, $a) {} }",
            ),
        ]);
        let src = "<?php function f(A $a, M $m) { $a->ok(); $a->missing(); $m->anything(); }";
        assert_eq!(
            codes(&index, src),
            vec![Code::UndefinedMethod {
                class: "A".into(),
                name: "missing".into(),
                is_static: false
            }]
        );
    }

    #[test]
    fn diagnostics_should_report_unused_imports_but_not_docblock_uses() {
        let index = index_with(&[("/b.php", BUILTINS)]);
        let src = "<?php\nnamespace App;\nuse Lib\\Used;\nuse Lib\\Doc;\nuse Lib\\Unused;\n/** @var Doc $d */\nnew Used();\n";
        let diags = diagnostics(&index, src);
        let unused: Vec<&str> = diags
            .iter()
            .filter(|d| d.code == Code::UnusedImport)
            .map(|d| &src[d.start..d.end])
            .collect();
        assert_eq!(unused, vec!["use Lib\\Unused;"]);
    }

    #[test]
    fn diagnostics_should_report_undefined_variable_once() {
        let index = index_with(&[("/b.php", BUILTINS)]);
        let src = "<?php function f($a) { $b = 1; foreach ([] as $k => $v) {} preg_match('/x/', 'x', $m); echo $a, $b, $k, $v, $m, $c, $c; }";
        assert_eq!(
            codes(&index, src),
            vec![Code::UndefinedVariable("$c".into())]
        );
    }

    #[test]
    fn diagnostics_should_skip_scopes_using_extract() {
        let index = index_with(&[("/b.php", BUILTINS)]);
        let src = "<?php function f() { extract([]); echo $x; }";
        assert!(codes(&index, src).is_empty());
    }

    #[test]
    fn quick_fix_should_import_matching_class() {
        let index = index_with(&[
            ("/b.php", BUILTINS),
            ("/u.php", "<?php namespace App\\Models; class User {}"),
        ]);
        let src = "<?php\nnamespace App\\Http;\n\nnew User();\n";
        let diag = diagnostics(&index, src).remove(0);
        let fixes = quick_fixes(&index, src, &diag, |_| None);
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0].title, "Import class App\\Models\\User");
        assert_eq!(fixes[0].edits[0].text, "\nuse App\\Models\\User;");
    }

    #[test]
    fn quick_fix_should_remove_unused_import_line() {
        let index = index_with(&[("/b.php", BUILTINS)]);
        let src = "<?php\nuse Lib\\Unused;\necho 1;\n";
        let diag = diagnostics(&index, src).remove(0);
        let fix = quick_fixes(&index, src, &diag, |_| None).remove(0);
        let e = &fix.edits[0];
        let mut out = src.to_string();
        out.replace_range(e.start..e.end, &e.text);
        assert_eq!(out, "<?php\necho 1;\n");
    }

    #[test]
    fn quick_fix_should_add_method_to_class_file() {
        let class_src = "<?php class A {\n    function ok() {}\n}\n";
        let index = index_with(&[("/b.php", BUILTINS), ("/a.php", class_src)]);
        let src = "<?php function f(A $a) { $a->make(); }";
        let diag = diagnostics(&index, src).remove(0);
        let fix = quick_fixes(&index, src, &diag, |_| Some(class_src.to_string())).remove(0);
        let e = &fix.edits[0];
        assert_eq!(e.path.as_deref(), Some(Path::new("/a.php")));
        let mut out = class_src.to_string();
        out.replace_range(e.start..e.end, &e.text);
        assert_eq!(
            out,
            "<?php class A {\n    function ok() {}\n\n    public function make()\n    {\n    }\n}\n"
        );
    }
}
