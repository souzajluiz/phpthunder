//! Refactorings inside one function: extract a variable from an expression,
//! inline a variable into its uses, and extract statements into a method.

use super::*;
use crate::inspection::Edit;
use crate::syntax::{parse, text};
use std::collections::BTreeSet;
use tree_sitter::{Node, Tree};

/// Node kinds that are statements rather than expressions.
const STATEMENTS: &[&str] = &[
    "expression_statement",
    "return_statement",
    "if_statement",
    "foreach_statement",
    "for_statement",
    "while_statement",
    "do_statement",
    "switch_statement",
    "try_statement",
    "echo_statement",
    "unset_statement",
    "compound_statement",
    "throw_expression",
];

/// Start of the line containing `byte`.
fn line_start(src: &str, byte: usize) -> usize {
    src[..byte].rfind('\n').map_or(0, |i| i + 1)
}

/// Leading whitespace of the line containing `byte`.
fn indent_at(src: &str, byte: usize) -> &str {
    let start = line_start(src, byte);
    let line = &src[start..];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// `start..end` with surrounding whitespace trimmed.
fn trimmed(src: &str, start: usize, end: usize) -> (usize, usize) {
    let slice = &src[start..end];
    let lead = slice.len() - slice.trim_start().len();
    let trail = slice.len() - slice.trim_end().len();
    (start + lead, end - trail)
}

/// The innermost statement node containing `node`.
fn enclosing_statement(node: Node) -> Option<Node> {
    let mut cur = Some(node);
    while let Some(n) = cur {
        if n.parent().is_some_and(|p| {
            matches!(
                p.kind(),
                "compound_statement" | "program" | "case_statement" | "default_statement"
            )
        }) {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

/// The function, method or closure around `node`, and the scope to search
/// variables in (the whole file outside functions).
fn function_scope<'t>(tree: &'t Tree, node: Node<'t>) -> Node<'t> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if matches!(
            n.kind(),
            "function_definition" | "method_declaration" | "anonymous_function" | "arrow_function"
        ) {
            return n;
        }
        cur = n.parent();
    }
    tree.root_node()
}

/// Every `variable_name` node under `scope` (not descending into nested
/// named functions or classes), in source order.
fn variables_in<'t>(scope: Node<'t>) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    let mut stack = vec![scope];
    while let Some(n) = stack.pop() {
        if n.kind() == "variable_name" {
            out.push(n);
        }
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            if n.id() == scope.id()
                || !matches!(
                    child.kind(),
                    "function_definition" | "method_declaration" | "class_declaration"
                )
            {
                stack.push(child);
            }
        }
    }
    out.sort_by_key(|n| n.start_byte());
    out
}

/// Whether `var` is written: the left side of an assignment, an
/// increment/decrement, a `foreach` value or key, a by-reference argument
/// target, or a `list()` target.
fn is_write(var: Node) -> bool {
    let Some(parent) = var.parent() else {
        return false;
    };
    match parent.kind() {
        "assignment_expression"
        | "augmented_assignment_expression"
        | "reference_assignment_expression" => parent
            .child_by_field_name("left")
            .is_some_and(|l| l.id() == var.id()),
        "update_expression" | "foreach_statement" | "pair" | "by_ref" => true,
        _ => {
            parent.kind() == "array_element_initializer"
                && parent.parent().is_some_and(|g| g.kind() == "list_literal")
        }
    }
}

/// A name for a variable holding `expression`: the called method or
/// function without `get`, else `value` (`$user->getName()` -> `$name`).
fn suggested_name(expression: &str) -> String {
    let callee = expression
        .rsplit(['>', ':'])
        .next()
        .unwrap_or(expression)
        .split('(')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_start_matches('$');
    let base = callee
        .strip_prefix("get")
        .filter(|r| r.starts_with(|c: char| c.is_ascii_uppercase()))
        .unwrap_or(callee);
    let mut chars = base.chars();
    let name: String = match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => c
            .to_lowercase()
            .chain(chars)
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect(),
        _ => String::new(),
    };
    if name.is_empty() {
        "value".into()
    } else {
        name
    }
}

/// Extracts the expression selected by `start..end` into a new variable
/// assigned just before the statement holding it; identical occurrences in
/// that statement are replaced too. `name` defaults to a name derived
/// from the expression.
///
/// # Errors
/// Returns a message when the selection is not one whole expression.
pub fn extract_variable(
    src: &str,
    start: usize,
    end: usize,
    name: Option<&str>,
) -> Result<LocalRefactoring, String> {
    let (start, end) = trimmed(src, start, end);
    let tree = parse(src);
    let node = tree
        .root_node()
        .descendant_for_byte_range(start, end)
        .filter(|n| n.start_byte() == start && n.end_byte() == end && n.is_named())
        .filter(|n| {
            !STATEMENTS.contains(&n.kind()) && n.kind() != "variable_name"
                || n.parent()
                    .is_some_and(|p| p.kind() != "expression_statement")
        })
        .ok_or("Select a whole expression to extract")?;
    if STATEMENTS.contains(&node.kind()) || node.kind() == "name" {
        return Err("Select a whole expression to extract".into());
    }
    let statement = enclosing_statement(node).ok_or("Select an expression inside a statement")?;
    let expression = text(node, src);
    let name = name.map_or_else(
        || suggested_name(expression),
        |n| n.trim_start_matches('$').to_string(),
    );
    let at = line_start(src, statement.start_byte());
    let indent = indent_at(src, statement.start_byte());
    let declaration = format!("{indent}${name} = {expression};\n");
    let mut edits = vec![Edit {
        path: None,
        start: at,
        end: at,
        text: declaration,
    }];
    let body = &src[statement.start_byte()..statement.end_byte()];
    let mut search = 0;
    while let Some(found) = body[search..].find(expression) {
        let s = statement.start_byte() + search + found;
        edits.push(Edit {
            path: None,
            start: s,
            end: s + expression.len(),
            text: format!("${name}"),
        });
        search += found + expression.len();
    }
    let caret = at + indent.len() + 1;
    Ok(LocalRefactoring { edits, caret })
}

/// The whole expression node selected by `start..end` (whitespace
/// trimmed), as Extract Variable and the Introduce refactorings need.
///
/// # Errors
/// Returns a message when the selection is not one whole expression.
fn selected_expression<'t>(
    tree: &'t Tree,
    src: &str,
    start: usize,
    end: usize,
) -> Result<Node<'t>, String> {
    let (start, end) = trimmed(src, start, end);
    let node = tree
        .root_node()
        .descendant_for_byte_range(start, end)
        .filter(|n| n.start_byte() == start && n.end_byte() == end && n.is_named())
        .ok_or("Select a whole expression")?;
    if STATEMENTS.contains(&node.kind()) || node.kind() == "name" {
        return Err("Select a whole expression".into());
    }
    Ok(node)
}

/// The nearest ancestor of `node` whose kind is one of `kinds`.
fn ancestor<'t>(node: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if kinds.contains(&n.kind()) {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

/// Edits replacing every occurrence of `expression` inside `within` with
/// `with`.
fn replace_occurrences(src: &str, within: Node, expression: &str, with: &str) -> Vec<Edit> {
    let body = &src[within.start_byte()..within.end_byte()];
    let mut edits = Vec::new();
    let mut search = 0;
    while let Some(found) = body[search..].find(expression) {
        let s = within.start_byte() + search + found;
        edits.push(Edit {
            path: None,
            start: s,
            end: s + expression.len(),
            text: with.to_string(),
        });
        search += found + expression.len();
    }
    edits
}

/// Where a new member goes at the top of the class body `body` (the line
/// after its `{`), and the indentation members use.
fn member_slot(src: &str, body: Node) -> (usize, String) {
    let open = body.start_byte() + 1;
    let at = if src[open..].starts_with('\n') {
        open + 1
    } else {
        open
    };
    let mut c = body.walk();
    let indent = body
        .named_children(&mut c)
        .next()
        .map(|m| indent_at(src, m.start_byte()).to_string())
        .unwrap_or_else(|| format!("{}    ", indent_at(src, body.start_byte())));
    (at, indent)
}

/// `name` in `UPPER_SNAKE_CASE`, splitting camel-case humps.
fn constant_name(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 && !out.ends_with('_') {
            out.push('_');
        }
        out.extend(c.to_uppercase());
    }
    out
}

/// Introduce Constant: declares `private const NAME = expression;` at the
/// top of the enclosing class and replaces every occurrence of the selected
/// expression in the class with `self::NAME`.
///
/// # Errors
/// Returns a message when the selection is not one whole expression inside
/// a class.
pub fn introduce_constant(
    src: &str,
    start: usize,
    end: usize,
    name: Option<&str>,
) -> Result<LocalRefactoring, String> {
    let tree = parse(src);
    let node = selected_expression(&tree, src, start, end)?;
    let class = ancestor(
        node,
        &["class_declaration", "trait_declaration", "enum_declaration"],
    )
    .ok_or("Select an expression inside a class")?;
    let body = class
        .child_by_field_name("body")
        .ok_or("Select an expression inside a class")?;
    let expression = text(node, src);
    let name = name.map_or_else(
        || constant_name(&suggested_name(expression)),
        |n| n.trim().to_string(),
    );
    let (at, indent) = member_slot(src, body);
    let mut edits = vec![Edit {
        path: None,
        start: at,
        end: at,
        text: format!("{indent}private const {name} = {expression};\n\n"),
    }];
    edits.extend(replace_occurrences(
        src,
        body,
        expression,
        &format!("self::{name}"),
    ));
    let caret = at + indent.len() + "private const ".len();
    Ok(LocalRefactoring { edits, caret })
}

/// Introduce Field: declares `private $name;` at the top of the enclosing
/// class, assigns the selected expression to `$this->name` before the
/// statement holding it, and replaces its occurrences in the method.
///
/// # Errors
/// Returns a message when the selection is not one whole expression
/// inside a method.
pub fn introduce_field(
    src: &str,
    start: usize,
    end: usize,
    name: Option<&str>,
) -> Result<LocalRefactoring, String> {
    let tree = parse(src);
    let node = selected_expression(&tree, src, start, end)?;
    let method =
        ancestor(node, &["method_declaration"]).ok_or("Select an expression inside a method")?;
    let class = ancestor(
        method,
        &["class_declaration", "trait_declaration", "enum_declaration"],
    )
    .ok_or("Select an expression inside a class")?;
    let body = class
        .child_by_field_name("body")
        .ok_or("Select an expression inside a class")?;
    let statement = enclosing_statement(node).ok_or("Select an expression inside a statement")?;
    let expression = text(node, src);
    let name = name.map_or_else(
        || suggested_name(expression),
        |n| n.trim().trim_start_matches('$').to_string(),
    );
    let (at, indent) = member_slot(src, body);
    let line = line_start(src, statement.start_byte());
    let statement_indent = indent_at(src, statement.start_byte());
    let mut edits = vec![
        Edit {
            path: None,
            start: at,
            end: at,
            text: format!("{indent}private ${name};\n\n"),
        },
        Edit {
            path: None,
            start: line,
            end: line,
            text: format!("{statement_indent}$this->{name} = {expression};\n"),
        },
    ];
    edits.extend(replace_occurrences(
        src,
        statement,
        expression,
        &format!("$this->{name}"),
    ));
    let caret = at + indent.len() + "private $".len();
    Ok(LocalRefactoring { edits, caret })
}

/// Introduce Parameter: adds `$name = expression` as the last parameter of
/// the enclosing function or method and uses it in place of the selected
/// expression throughout the body; existing calls keep working through the
/// default value.
///
/// # Errors
/// Returns a message when the selection is not one whole expression inside
/// a function or method.
pub fn introduce_parameter(
    src: &str,
    start: usize,
    end: usize,
    name: Option<&str>,
) -> Result<LocalRefactoring, String> {
    let tree = parse(src);
    let node = selected_expression(&tree, src, start, end)?;
    let function = ancestor(node, &["function_definition", "method_declaration"])
        .ok_or("Select an expression inside a function")?;
    let params = function
        .child_by_field_name("parameters")
        .ok_or("Select an expression inside a function")?;
    let body = function
        .child_by_field_name("body")
        .ok_or("Select an expression inside a function")?;
    let expression = text(node, src);
    let name = name.map_or_else(
        || suggested_name(expression),
        |n| n.trim().trim_start_matches('$').to_string(),
    );
    let close = params.end_byte() - 1;
    let separator = if params.named_child_count() > 0 {
        ", "
    } else {
        ""
    };
    let mut edits = vec![Edit {
        path: None,
        start: close,
        end: close,
        text: format!("{separator}${name} = {expression}"),
    }];
    edits.extend(replace_occurrences(
        src,
        body,
        expression,
        &format!("${name}"),
    ));
    let caret = close + separator.len() + 1;
    Ok(LocalRefactoring { edits, caret })
}

/// Inlines the variable at `byte`: its single assignment `$x = expr;` is
/// removed and every later read of `$x` becomes `expr` (parenthesized when
/// it is an operation).
///
/// # Errors
/// Returns a message when the caret is not on a variable, or the variable
/// is assigned more than once, written indirectly, or read before it is
/// assigned.
pub fn inline_variable(src: &str, byte: usize) -> Result<LocalRefactoring, String> {
    let tree = parse(src);
    let node = tree
        .root_node()
        .descendant_for_byte_range(byte, byte)
        .ok_or("Place the caret on a variable")?;
    let var = if node.kind() == "variable_name" {
        Some(node)
    } else {
        node.parent().filter(|p| p.kind() == "variable_name")
    }
    .ok_or("Place the caret on a variable")?;
    let name = text(var, src);
    if name == "$this" {
        return Err("$this cannot be inlined".into());
    }
    let scope = function_scope(&tree, var);
    let uses: Vec<Node> = variables_in(scope)
        .into_iter()
        .filter(|v| text(*v, src) == name)
        .collect();
    let writes: Vec<Node> = uses.iter().copied().filter(|v| is_write(*v)).collect();
    let [write] = writes.as_slice() else {
        return Err(format!("{name} is assigned more than once"));
    };
    let assignment = write
        .parent()
        .filter(|p| p.kind() == "assignment_expression")
        .ok_or_else(|| format!("{name} is not assigned with ="))?;
    let statement = assignment
        .parent()
        .filter(|p| p.kind() == "expression_statement")
        .ok_or_else(|| format!("{name} is assigned inside an expression"))?;
    let value = assignment
        .child_by_field_name("right")
        .ok_or("Missing value")?;
    if uses.iter().any(|u| u.start_byte() < write.start_byte()) {
        return Err(format!("{name} is used before it is assigned"));
    }
    let raw = text(value, src);
    let needs_parens = matches!(
        value.kind(),
        "binary_expression"
            | "conditional_expression"
            | "assignment_expression"
            | "unary_op_expression"
    );
    let replacement = if needs_parens {
        format!("({raw})")
    } else {
        raw.to_string()
    };
    let from = line_start(src, statement.start_byte());
    let to = src[statement.end_byte()..]
        .find('\n')
        .map_or(src.len(), |i| statement.end_byte() + i + 1);
    let mut edits = vec![Edit {
        path: None,
        start: from,
        end: to,
        text: String::new(),
    }];
    for read in uses.iter().filter(|u| u.id() != write.id()) {
        edits.push(Edit {
            path: None,
            start: read.start_byte(),
            end: read.end_byte(),
            text: replacement.clone(),
        });
    }
    Ok(LocalRefactoring { edits, caret: from })
}

/// Extracts the statements selected by `start..end` into a new private
/// method of the enclosing class, placed after the current method, and
/// calls it in their place. Variables defined before the selection become
/// parameters; a variable assigned in the selection and used after it is
/// returned and assigned back.
///
/// # Errors
/// Returns a message when the selection is not whole statements of one
/// block inside a method, contains `return`, or more than one variable
/// would have to be returned.
pub fn extract_method(
    src: &str,
    start: usize,
    end: usize,
    name: &str,
) -> Result<LocalRefactoring, String> {
    let name = name.trim();
    if !name
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        || !name.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        return Err(format!("'{name}' is not a valid method name"));
    }
    let (start, end) = trimmed(src, start, end);
    let tree = parse(src);
    let first = tree
        .root_node()
        .descendant_for_byte_range(start, start)
        .and_then(enclosing_statement)
        .ok_or("Select whole statements")?;
    let block = first
        .parent()
        .filter(|p| p.kind() == "compound_statement")
        .ok_or("Select statements inside a method body")?;
    let mut cursor = block.walk();
    let selected: Vec<Node> = block
        .named_children(&mut cursor)
        .filter(|n| {
            n.start_byte() >= start && n.end_byte() <= end && n.kind() != "comment"
                || n.kind() == "comment" && n.start_byte() >= start && n.end_byte() <= end
        })
        .collect();
    let (Some(head), Some(tail)) = (selected.first(), selected.last()) else {
        return Err("Select whole statements".into());
    };
    if head.start_byte() != start || tail.end_byte() != end {
        return Err("Select whole statements".into());
    }
    let method = function_scope(&tree, *head);
    if method.kind() != "method_declaration" {
        return Err("Statements can be extracted only inside a class method".into());
    }
    let is_static = {
        let mut c = method.walk();
        method
            .children(&mut c)
            .any(|n| n.kind() == "static_modifier")
    };
    let mut stack: Vec<Node> = selected.clone();
    while let Some(n) = stack.pop() {
        if n.kind() == "return_statement" {
            return Err("The selection contains return; it cannot be extracted".into());
        }
        let mut c = n.walk();
        stack.extend(
            n.named_children(&mut c)
                .filter(|c| !matches!(c.kind(), "anonymous_function" | "arrow_function")),
        );
    }
    let vars = variables_in(method);
    let inside = |n: &Node| n.start_byte() >= start && n.end_byte() <= end;
    let names_inside: BTreeSet<&str> = vars
        .iter()
        .filter(|v| inside(v))
        .map(|v| text(*v, src))
        .filter(|n| *n != "$this")
        .collect();
    let mut params: Vec<&str> = Vec::new();
    let mut outputs: Vec<&str> = Vec::new();
    for name in &names_inside {
        let before = vars
            .iter()
            .any(|v| v.end_byte() <= start && text(*v, src) == *name);
        let after = vars
            .iter()
            .any(|v| v.start_byte() >= end && text(*v, src) == *name);
        let written = vars
            .iter()
            .any(|v| inside(v) && text(*v, src) == *name && is_write(*v));
        if before {
            params.push(name);
        }
        if written && after {
            outputs.push(name);
        }
    }
    if outputs.len() > 1 {
        return Err(format!(
            "{} would have to be returned; extract fewer statements",
            outputs.join(", ")
        ));
    }
    let uses_this = vars.iter().any(|v| inside(v) && text(*v, src) == "$this");
    if is_static && uses_this {
        return Err("$this is used inside a static method".into());
    }
    let method_indent = indent_at(src, method.start_byte()).to_string();
    let body_indent = format!("{method_indent}    ");
    let first_line = line_start(src, start);
    let selection_indent = indent_at(src, start).to_string();
    let selected_text = &src[first_line..end];
    let mut body: String = selected_text
        .lines()
        .map(|l| {
            let stripped = l
                .strip_prefix(selection_indent.as_str())
                .unwrap_or(l.trim_start());
            if stripped.is_empty() {
                String::new()
            } else {
                format!("{body_indent}{stripped}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(out) = outputs.first() {
        body.push_str(&format!("\n\n{body_indent}return {out};"));
    }
    let receiver = if is_static { "self::" } else { "$this->" };
    let args = params.join(", ");
    let call = match outputs.first() {
        Some(out) => format!("{out} = {receiver}{name}({args});"),
        None => format!("{receiver}{name}({args});"),
    };
    let modifier = if is_static {
        "private static function"
    } else {
        "private function"
    };
    let method_text = format!(
        "\n\n{method_indent}{modifier} {name}({args})\n{method_indent}{{\n{body}\n{method_indent}}}"
    );
    let insert_at = method.end_byte();
    let edits = vec![
        Edit {
            path: None,
            start: first_line,
            end,
            text: format!("{selection_indent}{call}"),
        },
        Edit {
            path: None,
            start: insert_at,
            end: insert_at,
            text: method_text,
        },
    ];
    Ok(LocalRefactoring {
        edits,
        caret: first_line + selection_indent.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refactor::apply_edits;

    fn run(src: &str, plan: &LocalRefactoring) -> String {
        apply_edits(src, &plan.edits)
    }

    #[test]
    fn extract_variable_should_name_and_hoist_the_expression() {
        let src = "<?php\nfunction f($user) {\n    echo strtoupper($user->getName()) . $user->getName();\n}\n";
        let start = src.find("$user->getName()").unwrap();
        let plan = extract_variable(src, start, start + "$user->getName()".len(), None).unwrap();
        let out = run(src, &plan);
        assert_eq!(
            out,
            "<?php\nfunction f($user) {\n    $name = $user->getName();\n    echo strtoupper($name) . $name;\n}\n"
        );
    }

    #[test]
    fn extract_variable_should_refuse_partial_selections() {
        let src = "<?php\n$a = 1 + 2 + 3;\n";
        let start = src.find("+ 2").unwrap();
        assert!(extract_variable(src, start, start + 3, Some("x")).is_err());
    }

    #[test]
    fn inline_variable_should_replace_reads_and_drop_the_assignment() {
        let src = "<?php\nfunction f($a, $b) {\n    $sum = $a + $b;\n    return $sum * $sum;\n}\n";
        let plan = inline_variable(src, src.find("$sum *").unwrap()).unwrap();
        assert_eq!(
            run(src, &plan),
            "<?php\nfunction f($a, $b) {\n    return ($a + $b) * ($a + $b);\n}\n"
        );
    }

    #[test]
    fn inline_variable_should_refuse_reassigned_variables() {
        let src = "<?php\nfunction f() {\n    $x = 1;\n    $x = 2;\n    return $x;\n}\n";
        assert!(inline_variable(src, src.find("return $x").unwrap() + 7).is_err());
    }

    #[test]
    fn extract_method_should_pass_inputs_and_return_the_output() {
        let src = "<?php\nclass A {\n    public function run($items)\n    {\n        $total = 0;\n        foreach ($items as $item) {\n            $total += $item;\n        }\n        echo $total;\n    }\n}\n";
        let start = src.find("$total = 0;").unwrap();
        let end = src.find("        echo").unwrap() - 1;
        let plan = extract_method(src, start, end, "sum").unwrap();
        let out = run(src, &plan);
        assert!(
            out.contains("        $total = $this->sum($items);\n        echo $total;"),
            "{out}"
        );
        assert!(out.contains("    private function sum($items)\n    {\n        $total = 0;\n        foreach ($items as $item) {\n            $total += $item;\n        }\n\n        return $total;\n    }"), "{out}");
    }

    #[test]
    fn extract_method_should_refuse_returns() {
        let src =
            "<?php\nclass A {\n    public function run()\n    {\n        return 1;\n    }\n}\n";
        let start = src.find("return 1;").unwrap();
        assert!(extract_method(src, start, start + "return 1;".len(), "x").is_err());
    }

    #[test]
    fn introduce_constant_should_declare_and_replace() {
        let src = "<?php\nclass A\n{\n    public function f()\n    {\n        return 60 * 60;\n    }\n}\n";
        let at = src.find("60 * 60").unwrap();
        let plan = introduce_constant(src, at, at + 7, Some("HOUR")).unwrap();
        let out = apply_edits(src, &plan.edits);
        assert!(
            out.contains("private const HOUR = 60 * 60;") && out.contains("return self::HOUR;")
        );
    }

    #[test]
    fn introduce_parameter_should_add_a_default() {
        let src = "<?php\nfunction f($a)\n{\n    return $a * 1.2;\n}\n";
        let at = src.find("1.2").unwrap();
        let plan = introduce_parameter(src, at, at + 3, Some("rate")).unwrap();
        let out = apply_edits(src, &plan.edits);
        assert!(out.contains("function f($a, $rate = 1.2)") && out.contains("return $a * $rate;"));
    }

    #[test]
    fn introduce_field_should_assign_before_the_statement() {
        let src =
            "<?php\nclass A\n{\n    public function f()\n    {\n        g(new B());\n    }\n}\n";
        let at = src.find("new B()").unwrap();
        let plan = introduce_field(src, at, at + 7, Some("b")).unwrap();
        let out = apply_edits(src, &plan.edits);
        assert!(
            out.contains("private $b;")
                && out.contains("$this->b = new B();\n        g($this->b);")
        );
    }
}
