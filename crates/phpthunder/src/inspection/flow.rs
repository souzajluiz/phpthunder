//! Inspections over a function's body: variables assigned but never read,
//! declared return types the body can end without honouring, and returned
//! values that can never match the declared type.

use super::*;
use crate::analysis::Analyzer;
use crate::index::Index;
use crate::syntax::text;
use std::collections::{HashMap, HashSet};
use tree_sitter::Node;

/// Return types that need no `return` statement.
const NO_RETURN_NEEDED: &[&str] = &["void", "never", "mixed", "null"];

/// Scalar type names as declared.
const SCALARS: &[&str] = &["int", "float", "string", "bool", "false", "true"];

/// Whether `node` is a function-like scope.
fn is_function_like(kind: &str) -> bool {
    matches!(
        kind,
        "function_definition" | "method_declaration" | "anonymous_function" | "arrow_function"
    )
}

/// Every function-like node under `root`.
fn functions(root: Node) -> Vec<Node> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if is_function_like(n.kind()) {
            out.push(n);
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    out
}

/// Nodes of a function body, not descending into nested functions or
/// classes (their bodies are other scopes); closures are listed themselves.
fn body_nodes(body: Node) -> Vec<Node> {
    let mut out = Vec::new();
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        out.push(n);
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            match child.kind() {
                // Closures are listed (their `use` clause reads this scope)
                // but their bodies are their own scope.
                "anonymous_function" => out.push(child),
                "function_definition"
                | "class_declaration"
                | "anonymous_class"
                | "method_declaration" => {}
                _ => stack.push(child),
            }
        }
    }
    out
}

/// Reports variables assigned with `=` in a function but never read there.
/// Parameters, `foreach`/`catch` bindings, `global`/`static` names and
/// `$_`-prefixed names are exempt, and scopes using `compact()`,
/// `extract()`, `$$name` or includes are skipped.
pub(crate) fn unused_variables(
    root: Node,
    src: &str,
    push: &mut impl FnMut(Node, Severity, String, Code),
) {
    for function in functions(root) {
        if function.kind() == "arrow_function" {
            continue;
        }
        let Some(body) = function.child_by_field_name("body") else {
            continue;
        };
        let mut writes: HashMap<&str, Node> = HashMap::new();
        let mut reads: HashSet<&str> = HashSet::new();
        let mut dynamic = false;
        let mut exempt: HashSet<&str> = SUPERGLOBALS.iter().copied().collect();
        for node in body_nodes(body) {
            match node.kind() {
                "function_call_expression" => {
                    let name = node
                        .child_by_field_name("function")
                        .map(|f| text(f, src).trim_start_matches('\\').to_ascii_lowercase());
                    if name.is_some_and(|n| DYNAMIC_SCOPE_CALLS.contains(&n.as_str())) {
                        dynamic = true;
                    }
                }
                "dynamic_variable_name"
                | "include_expression"
                | "include_once_expression"
                | "require_expression"
                | "require_once_expression" => {
                    dynamic = true;
                }
                "global_declaration"
                | "function_static_declaration"
                | "foreach_statement"
                | "catch_clause" => {
                    let mut stack = vec![node];
                    while let Some(n) = stack.pop() {
                        if n.kind() == "variable_name" {
                            exempt.insert(text(n, src));
                        }
                        if n.kind() == "compound_statement" {
                            continue;
                        }
                        let mut c = n.walk();
                        stack.extend(n.named_children(&mut c));
                    }
                }
                "anonymous_function" => {
                    // Closures read what they import with `use`.
                    let mut c = node.walk();
                    for child in node.named_children(&mut c) {
                        if child.kind() == "anonymous_function_use_clause" {
                            let mut uc = child.walk();
                            for v in child.named_children(&mut uc) {
                                reads.insert(text(v, src).trim_start_matches('&'));
                            }
                        }
                    }
                }
                "variable_name" => {
                    let name = text(node, src);
                    let plain_write = node.parent().is_some_and(|p| {
                        p.kind() == "assignment_expression"
                            && p.child_by_field_name("left")
                                .is_some_and(|l| l.id() == node.id())
                    });
                    if plain_write {
                        writes.entry(name).or_insert(node);
                    } else {
                        reads.insert(name);
                    }
                }
                _ => {}
            }
        }
        // Arrow functions read their enclosing scope implicitly.
        for node in body_nodes(body)
            .into_iter()
            .filter(|n| n.kind() == "arrow_function")
        {
            let mut stack = vec![node];
            while let Some(n) = stack.pop() {
                if n.kind() == "variable_name" {
                    reads.insert(text(n, src));
                }
                let mut c = n.walk();
                stack.extend(n.named_children(&mut c));
            }
        }
        if dynamic {
            continue;
        }
        let mut unused: Vec<(&str, Node)> = writes
            .into_iter()
            .filter(|(name, _)| {
                !reads.contains(name) && !exempt.contains(name) && !name.starts_with("$_")
            })
            .collect();
        unused.sort_by_key(|(_, n)| n.start_byte());
        for (name, node) in unused {
            push(
                node,
                Severity::Warning,
                format!("Variable '{name}' is assigned but never used"),
                Code::UnusedVariable(name.to_string()),
            );
        }
    }
}

/// Whether executing `node` always ends in `return`, `throw`, `exit` or an
/// endless loop.
fn always_exits(node: Node, src: &str, never: &dyn Fn(Node) -> bool) -> bool {
    match node.kind() {
        "return_statement" | "exit_statement" => true,
        "expression_statement" => node.named_child(0).is_some_and(|e| {
            e.kind() == "throw_expression"
                || e.kind() == "exit_statement"
                || e.kind() == "function_call_expression"
                    && e.child_by_field_name("function")
                        .is_some_and(|f| matches!(text(f, src), "exit" | "die"))
                || never(e)
        }),
        "switch_statement" => {
            // Every path ends in an exit when there is a default and each
            // case either exits or falls through to the next one.
            let Some(block) = node.child_by_field_name("body") else {
                return false;
            };
            let mut c = block.walk();
            let cases: Vec<Node> = block
                .named_children(&mut c)
                .filter(|n| matches!(n.kind(), "case_statement" | "default_statement"))
                .collect();
            let has_default = cases.iter().any(|n| n.kind() == "default_statement");
            let exits = |case: &Node| {
                let mut cc = case.walk();
                case.named_children(&mut cc)
                    .any(|s| always_exits(s, src, never))
            };
            let falls_through = |case: &Node| {
                let mut cc = case.walk();
                case.named_children(&mut cc)
                    .nth(usize::from(case.kind() == "case_statement"))
                    .is_none()
            };
            has_default
                && !cases.is_empty()
                && cases.iter().all(|c| exits(c) || falls_through(c))
                && cases.last().is_some_and(exits)
        }
        "compound_statement" | "colon_block" => {
            let mut c = node.walk();
            node.named_children(&mut c)
                .any(|s| always_exits(s, src, never))
        }
        "if_statement" => {
            let Some(body) = node.child_by_field_name("body") else {
                return false;
            };
            let mut c = node.walk();
            let mut has_else = false;
            let branches_exit = node.named_children(&mut c).all(|child| match child.kind() {
                "else_clause" => {
                    has_else = true;
                    child
                        .child_by_field_name("body")
                        .is_some_and(|b| always_exits(b, src, never))
                }
                "else_if_clause" => child
                    .child_by_field_name("body")
                    .is_some_and(|b| always_exits(b, src, never)),
                _ => true,
            });
            has_else && branches_exit && always_exits(body, src, never)
        }
        "try_statement" => {
            let mut c = node.walk();
            let children: Vec<Node> = node.named_children(&mut c).collect();
            if children.iter().any(|ch| {
                ch.kind() == "finally_clause"
                    && ch
                        .child_by_field_name("body")
                        .is_some_and(|b| always_exits(b, src, never))
            }) {
                return true;
            }
            let body_exits = node
                .child_by_field_name("body")
                .is_some_and(|b| always_exits(b, src, never));
            body_exits
                && children
                    .iter()
                    .filter(|ch| ch.kind() == "catch_clause")
                    .all(|ch| {
                        ch.child_by_field_name("body")
                            .is_some_and(|b| always_exits(b, src, never))
                    })
        }
        "while_statement" | "do_statement" | "for_statement" => {
            let endless = match node.kind() {
                "for_statement" => text(node, src).replace(' ', "").contains(";;"),
                _ => node.child_by_field_name("condition").is_some_and(|c| {
                    matches!(
                        text(c, src)
                            .trim_matches(|c| c == '(' || c == ')')
                            .trim()
                            .to_ascii_lowercase()
                            .as_str(),
                        "true" | "1"
                    )
                }),
            };
            let mut stack = vec![node];
            let mut breaks = false;
            while let Some(n) = stack.pop() {
                if n.kind() == "break_statement" {
                    breaks = true;
                }
                let mut c = n.walk();
                stack.extend(
                    n.named_children(&mut c)
                        .filter(|c| !is_function_like(c.kind())),
                );
            }
            endless && !breaks
        }
        _ => false,
    }
}

/// Normalized declared return type: text with a leading `\` dropped.
fn declared_type(function: Node, src: &str) -> Option<String> {
    let ty = function.child_by_field_name("return_type")?;
    Some(text(ty, src).trim().trim_start_matches('\\').to_string())
}

/// Whether `body` contains `yield` (a generator needs no return).
fn is_generator(body: Node) -> bool {
    body_nodes(body)
        .iter()
        .any(|n| n.kind() == "yield_expression")
}

/// Reports functions with a declared return type whose body can end
/// without returning.
pub(crate) fn missing_returns(
    index: &Index,
    analyzer: &Analyzer,
    root: Node,
    src: &str,
    push: &mut impl FnMut(Node, Severity, String, Code),
) {
    // A call to a function or method declared `: never` ends the path too
    // (`$this->fail()`, `abort()`).
    let never = |call: Node| -> bool {
        let name = match call.kind() {
            "function_call_expression" => call.child_by_field_name("function"),
            "member_call_expression"
            | "nullsafe_member_call_expression"
            | "scoped_call_expression" => call.child_by_field_name("name"),
            _ => None,
        };
        let Some(name) = name else {
            return false;
        };
        let Some(target) = analyzer.target_of(name) else {
            return false;
        };
        crate::analysis::resolve(index, &target)
            .first()
            .is_some_and(|d| {
                d.symbol
                    .signature
                    .trim_end()
                    .to_ascii_lowercase()
                    .ends_with(": never")
            })
    };
    for function in functions(root) {
        if !matches!(
            function.kind(),
            "function_definition" | "method_declaration" | "anonymous_function"
        ) {
            continue;
        }
        let (Some(body), Some(ty)) = (
            function.child_by_field_name("body"),
            declared_type(function, src),
        ) else {
            continue;
        };
        if NO_RETURN_NEEDED.contains(&ty.to_ascii_lowercase().as_str())
            || is_generator(body)
            || always_exits(body, src, &never)
        {
            continue;
        }
        let anchor = function.child_by_field_name("name").unwrap_or(function);
        push(
            anchor,
            Severity::Error,
            format!("Missing return statement: '{ty}' expected"),
            Code::MissingReturn(ty),
        );
    }
}

/// Kind of value a `return` expression gives, when it is evident from the
/// expression alone: `null`, a scalar, `array`, or a class for `new X`.
fn evident_type(expr: Node, src: &str, analyzer: &Analyzer) -> Option<String> {
    match expr.kind() {
        "null" => Some("null".into()),
        "integer" => Some("int".into()),
        "float" => Some("float".into()),
        "boolean" => Some("bool".into()),
        "string" | "encapsed_string" | "heredoc" | "nowdoc" => {
            let raw = text(expr, src).trim_matches(|c| c == '\'' || c == '"');
            Some(if raw.trim().parse::<f64>().is_ok() {
                "numeric-string".into()
            } else {
                "string".into()
            })
        }
        "array_creation_expression" => Some("array".into()),
        "object_creation_expression" => {
            let class = expr
                .named_child(0)
                .filter(|n| matches!(n.kind(), "name" | "qualified_name"))?;
            Some(
                analyzer
                    .ctx(expr.start_byte())
                    .resolve_class(text(class, src)),
            )
        }
        _ => None,
    }
}

/// Whether a value of evident type `found` can never satisfy the declared
/// single type `expected` (coercive typing mode: scalars convert among
/// themselves, except non-numeric strings to numbers).
fn incompatible(
    index: &Index,
    expected: &str,
    found: &str,
    analyzer: &Analyzer,
    at: usize,
) -> bool {
    let nullable = expected.starts_with('?');
    let base = expected.trim_start_matches('?');
    let lower = base.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "mixed" | "self" | "static" | "object" | "iterable" | "callable"
    ) || base.contains(['|', '&'])
    {
        return false;
    }
    let is_class_type = !SCALARS.contains(&lower.as_str())
        && lower != "array"
        && lower != "void"
        && lower != "null"
        && lower != "never";
    match found {
        "null" => !nullable,
        "array" => lower != "array",
        "int" | "float" | "bool" | "numeric-string" => lower == "array" || is_class_type,
        "string" => lower == "array" || lower == "int" || lower == "float" || is_class_type,
        class => {
            if !is_class_type {
                return true;
            }
            let expected_fqn = analyzer.ctx(at).resolve_class(base);
            // Only judge classes the index knows completely.
            if index.classes(class).is_empty() || index.classes(&expected_fqn).is_empty() {
                return false;
            }
            !index
                .hierarchy(class)
                .iter()
                .any(|c| c.eq_ignore_ascii_case(&expected_fqn))
        }
    }
}

/// Reports `return` statements whose value can never match the function's
/// declared return type, and bare `return;` in functions that must return
/// a value.
pub(crate) fn return_mismatches(
    index: &Index,
    analyzer: &Analyzer,
    root: Node,
    src: &str,
    push: &mut impl FnMut(Node, Severity, String, Code),
) {
    for function in functions(root) {
        if function.kind() == "arrow_function" {
            continue;
        }
        let (Some(body), Some(ty)) = (
            function.child_by_field_name("body"),
            declared_type(function, src),
        ) else {
            continue;
        };
        let lower = ty.to_ascii_lowercase();
        if is_generator(body) || lower == "never" {
            continue;
        }
        for ret in body_nodes(body)
            .into_iter()
            .filter(|n| n.kind() == "return_statement")
        {
            let found = match ret.named_child(0) {
                None if lower == "void" => continue,
                None => Some("nothing".to_string()),
                Some(_) if lower == "void" => Some("a value".to_string()),
                Some(expr) => evident_type(expr, src, analyzer)
                    .filter(|f| incompatible(index, &ty, f, analyzer, ret.start_byte())),
            };
            if let Some(found) = found {
                let shown = if found == "numeric-string" {
                    "string".to_string()
                } else {
                    found
                };
                push(
                    ret,
                    Severity::Error,
                    format!("Return type mismatch: '{ty}' expected, {shown} returned"),
                    Code::ReturnTypeMismatch {
                        expected: ty.clone(),
                        found: shown,
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::index::{Index, index_source};
    use crate::inspection::{Code, diagnostics};
    use std::path::PathBuf;

    fn codes(src: &str) -> Vec<Code> {
        let mut index = Index::default();
        index.update(PathBuf::from("/p/a.php"), index_source(src));
        diagnostics(&index, src)
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn unused_variables_should_be_reported_once_and_respect_exemptions() {
        let src = "<?php\nfunction f($p) {\n    $a = 1;\n    $b = 2;\n    $c = 3;\n    foreach ([1] as $k => $v) {}\n    $fn = fn () => $c;\n    $g = function () use ($b) {};\n    return $fn . $g . $p;\n}\n";
        // `$b` is read by nothing in the closure, which the closure-use
        // check reports; the variable itself counts as used.
        assert_eq!(
            codes(src),
            vec![
                Code::UnusedVariable("$a".into()),
                Code::UnusedClosureUse("$b".into())
            ]
        );
        let dynamic = "<?php\nfunction f() {\n    $a = 1;\n    return compact('a');\n}\n";
        assert!(codes(dynamic).is_empty());
    }

    #[test]
    fn missing_returns_should_follow_branches() {
        let missing =
            "<?php\nfunction f(bool $x): int {\n    if ($x) {\n        return 1;\n    }\n}\n";
        assert_eq!(codes(missing), vec![Code::MissingReturn("int".into())]);
        let complete = "<?php\nfunction f(bool $x): int {\n    if ($x) {\n        return 1;\n    } else {\n        throw new \\Exception();\n    }\n}\nfunction g(): void {}\nfunction h(): iterable { yield 1; }\nfunction stop(): never { exit; }\nfunction i(): int { stop(); }\nfunction j(string $s): int {\n    switch ($s) {\n        case 'a':\n        case 'b':\n            return 1;\n        default:\n            return 2;\n    }\n}\n";
        assert!(
            codes(complete)
                .iter()
                .all(|c| !matches!(c, Code::MissingReturn(_))),
            "{:?}",
            codes(complete)
        );
    }

    #[test]
    fn return_mismatches_should_flag_impossible_values() {
        let src = "<?php\nclass A {}\nclass B {}\nclass C extends A {}\nfunction f(): int { return 'abc'; }\nfunction g(): A { return new B(); }\nfunction h(): A { return new C(); }\nfunction i(): string { return 42; }\nfunction j(): ?A { return null; }\nfunction k(): array { return null; }\nfunction l(): int { return; }\n";
        let found: Vec<String> = codes(src)
            .into_iter()
            .filter_map(|c| match c {
                Code::ReturnTypeMismatch { expected, found } => Some(format!("{expected}:{found}")),
                _ => None,
            })
            .collect();
        assert_eq!(found, ["int:string", "A:B", "array:null", "int:nothing"]);
    }
}
