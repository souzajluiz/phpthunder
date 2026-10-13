//! Syntax-only inspections: suspicious code that needs no type
//! information, so it is checked on every keystroke cheaply. Unreachable
//! statements, duplicate array keys, assignments used as conditions,
//! loose comparisons with `null`, empty `catch` blocks, self-assignments,
//! and private members nothing in their class uses.

use super::*;
use crate::syntax::text;
use std::collections::HashMap;
use tree_sitter::Node;

/// Statements after which the rest of a block never runs.
const EXITS: &[&str] = &[
    "return_statement",
    "break_statement",
    "continue_statement",
    "exit_statement",
];

/// Methods PHP or frameworks call by name, never reported as unused.
const MAGIC: &[&str] = &[
    "__construct",
    "__destruct",
    "__call",
    "__callstatic",
    "__get",
    "__set",
    "__isset",
    "__unset",
    "__sleep",
    "__wakeup",
    "__serialize",
    "__unserialize",
    "__tostring",
    "__invoke",
    "__set_state",
    "__clone",
    "__debuginfo",
];

/// Every node under `root`, depth first.
fn all_nodes(root: Node) -> Vec<Node> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        out.push(n);
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    out
}

/// Whether `stmt` always leaves its block: `return`, `break`, `continue`,
/// `exit`, or an expression statement that throws.
fn leaves_block(stmt: Node) -> bool {
    if EXITS.contains(&stmt.kind()) {
        return true;
    }
    stmt.kind() == "expression_statement"
        && stmt
            .named_child(0)
            .is_some_and(|e| e.kind() == "throw_expression")
}

/// Runs every syntax-only inspection over the tree rooted at `root`.
pub(crate) fn style_checks(
    root: Node,
    src: &str,
    push: &mut impl FnMut(Node, Severity, String, Code),
) {
    for node in all_nodes(root) {
        match node.kind() {
            "compound_statement" => unreachable_code(node, push),
            "array_creation_expression" => duplicate_keys(node, src, push),
            "if_statement" | "while_statement" | "else_if_clause" => {
                assignment_in_condition(node, push);
            }
            "binary_expression" => loose_null_comparison(node, src, push),
            "catch_clause" => empty_catch(node, push),
            "assignment_expression" => self_assignment(node, src, push),
            "class_declaration" | "trait_declaration" | "enum_declaration" => {
                unused_private_members(node, src, push);
            }
            _ => {}
        }
    }
}

/// The first statement after one that leaves the block.
fn unreachable_code(block: Node, push: &mut impl FnMut(Node, Severity, String, Code)) {
    let mut c = block.walk();
    let statements: Vec<Node> = block
        .named_children(&mut c)
        .filter(|n| n.kind() != "comment")
        .collect();
    if let Some(i) = statements.iter().position(|s| leaves_block(*s))
        && let Some(next) = statements.get(i + 1)
        // A function declared after `return` is still hoisted.
        && !matches!(next.kind(), "function_definition" | "class_declaration")
    {
        push(
            *next,
            Severity::Warning,
            "Unreachable statement".into(),
            Code::UnreachableCode,
        );
    }
}

/// Literal keys repeated in one array literal; the later one wins.
fn duplicate_keys(array: Node, src: &str, push: &mut impl FnMut(Node, Severity, String, Code)) {
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut c = array.walk();
    for element in array.named_children(&mut c) {
        if element.kind() != "array_element_initializer" {
            continue;
        }
        let has_arrow = (0..element.child_count())
            .filter_map(|i| element.child(i))
            .any(|ch| ch.kind() == "=>");
        let Some(key) = element.named_child(0).filter(|_| has_arrow) else {
            continue;
        };
        if !matches!(key.kind(), "string" | "encapsed_string" | "integer") {
            continue;
        }
        let literal = text(key, src).trim_matches(['\'', '"']).to_string();
        if literal.contains('$') {
            continue;
        }
        if seen.insert(literal.clone(), ()).is_some() {
            push(
                key,
                Severity::Warning,
                format!("Duplicate array key '{literal}'"),
                Code::DuplicateArrayKey(literal),
            );
        }
    }
}

/// `if ($a = 5)` / `if ($a = $b)`: an assignment of a constant or plain
/// variable, where a comparison was almost surely meant. Assigning a call
/// result (`if ($user = find())`) is a common idiom and is not reported.
fn assignment_in_condition(statement: Node, push: &mut impl FnMut(Node, Severity, String, Code)) {
    let Some(condition) = statement.child_by_field_name("condition") else {
        return;
    };
    let inner = if condition.kind() == "parenthesized_expression" {
        condition.named_child(0)
    } else {
        Some(condition)
    };
    let typo = |n: &Node| {
        n.kind() == "assignment_expression"
            && n.child_by_field_name("right").is_some_and(|r| {
                matches!(
                    r.kind(),
                    "integer"
                        | "float"
                        | "string"
                        | "encapsed_string"
                        | "boolean"
                        | "null"
                        | "variable_name"
                )
            })
    };
    if let Some(assignment) = inner.filter(typo) {
        push(
            assignment,
            Severity::Warning,
            "Assignment in condition".into(),
            Code::AssignmentInCondition,
        );
    }
}

/// `$a == null` / `$a != null`, which also match `0`, `''` and `[]`.
fn loose_null_comparison(
    binary: Node,
    src: &str,
    push: &mut impl FnMut(Node, Severity, String, Code),
) {
    let Some(operator) = binary.child_by_field_name("operator") else {
        return;
    };
    let op = text(operator, src);
    if !matches!(op, "==" | "!=") {
        return;
    }
    let is_null = |n: Option<Node>| {
        n.is_some_and(|n| n.kind() == "null" || text(n, src).eq_ignore_ascii_case("null"))
    };
    if is_null(binary.child_by_field_name("left")) || is_null(binary.child_by_field_name("right")) {
        push(
            operator,
            Severity::Warning,
            format!("Loose comparison with null; use {op}="),
            Code::LooseNullComparison,
        );
    }
}

/// A `catch` whose block is empty, without even a comment saying why.
fn empty_catch(clause: Node, push: &mut impl FnMut(Node, Severity, String, Code)) {
    let Some(body) = clause.child_by_field_name("body") else {
        return;
    };
    if body.named_child_count() == 0 {
        push(
            clause,
            Severity::Warning,
            "Empty catch block".into(),
            Code::EmptyCatch,
        );
    }
}

/// `$a = $a;`.
fn self_assignment(
    assignment: Node,
    src: &str,
    push: &mut impl FnMut(Node, Severity, String, Code),
) {
    let (Some(left), Some(right)) = (
        assignment.child_by_field_name("left"),
        assignment.child_by_field_name("right"),
    ) else {
        return;
    };
    if left.kind() == right.kind()
        && matches!(left.kind(), "variable_name" | "member_access_expression")
        && text(left, src) == text(right, src)
    {
        push(
            assignment,
            Severity::Warning,
            format!("{} is assigned to itself", text(left, src)),
            Code::SelfAssignment,
        );
    }
}

/// Whether `node` (a member declaration) is marked `private`.
fn is_private(node: Node, src: &str) -> bool {
    let mut c = node.walk();
    node.children(&mut c)
        .any(|ch| ch.kind() == "visibility_modifier" && text(ch, src) == "private")
}

/// Private methods and properties never mentioned elsewhere in the class.
fn unused_private_members(
    class: Node,
    src: &str,
    push: &mut impl FnMut(Node, Severity, String, Code),
) {
    let Some(body) = class.child_by_field_name("body") else {
        return;
    };
    let class_text = text(body, src);
    let mut c = body.walk();
    for member in body.named_children(&mut c) {
        match member.kind() {
            "method_declaration" if is_private(member, src) => {
                let Some(name) = member.child_by_field_name("name") else {
                    continue;
                };
                let name_text = text(name, src);
                if MAGIC.contains(&name_text.to_ascii_lowercase().as_str()) {
                    continue;
                }
                let used = [
                    format!("->{name_text}("),
                    format!("::{name_text}("),
                    format!("'{name_text}'"),
                    format!("\"{name_text}\""),
                    format!("->{name_text}(...)"),
                ]
                .iter()
                .any(|p| class_text.contains(p.as_str()));
                if !used {
                    push(
                        name,
                        Severity::Warning,
                        format!("Unused private method '{name_text}'"),
                        Code::UnusedPrivateMember(name_text.to_string()),
                    );
                }
            }
            "property_declaration" if is_private(member, src) => {
                let mut pc = member.walk();
                for element in member.named_children(&mut pc) {
                    if element.kind() != "property_element" {
                        continue;
                    }
                    let Some(var) = element.named_child(0) else {
                        continue;
                    };
                    let name = text(var, src).trim_start_matches('$');
                    let used = class_text
                        .match_indices(&format!("->{name}"))
                        .any(|(i, m)| {
                            !class_text[i + m.len()..]
                                .starts_with(|c: char| c.is_alphanumeric() || c == '_')
                        })
                        || class_text.contains(&format!("::${name}"))
                        || class_text.contains(&format!("'{name}'"));
                    if !used {
                        push(
                            var,
                            Severity::Warning,
                            format!("Unused private property '${name}'"),
                            Code::UnusedPrivateMember(format!("${name}")),
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    fn codes(src: &str) -> Vec<Code> {
        let tree = parse(src);
        let mut out = Vec::new();
        style_checks(tree.root_node(), src, &mut |_, _, _, code| out.push(code));
        out
    }

    #[test]
    fn style_checks_should_find_unreachable_code() {
        assert_eq!(
            codes("<?php function f() { return 1; echo 2; }"),
            [Code::UnreachableCode]
        );
    }

    #[test]
    fn style_checks_should_find_duplicate_keys() {
        assert_eq!(
            codes("<?php $a = ['x' => 1, 'y' => 2, 'x' => 3];"),
            [Code::DuplicateArrayKey("x".into())]
        );
    }

    #[test]
    fn style_checks_should_find_assignment_in_condition() {
        assert_eq!(codes("<?php if ($a = 5) {}"), [Code::AssignmentInCondition]);
    }

    #[test]
    fn style_checks_should_accept_assigning_a_call_in_condition() {
        assert_eq!(codes("<?php if ($a = f()) {}"), []);
    }

    #[test]
    fn style_checks_should_find_loose_null_comparison() {
        assert_eq!(codes("<?php $b = $a == null;"), [Code::LooseNullComparison]);
    }

    #[test]
    fn style_checks_should_accept_strict_null_comparison() {
        assert_eq!(codes("<?php $b = $a === null;"), []);
    }

    #[test]
    fn style_checks_should_find_empty_catch() {
        assert_eq!(
            codes("<?php try { f(); } catch (E $e) {}"),
            [Code::EmptyCatch]
        );
    }

    #[test]
    fn style_checks_should_find_self_assignment() {
        assert_eq!(codes("<?php $a = $a;"), [Code::SelfAssignment]);
    }

    #[test]
    fn style_checks_should_find_unused_private_method() {
        assert_eq!(
            codes("<?php class A { private function helper() {} public function f() {} }"),
            [Code::UnusedPrivateMember("helper".into())]
        );
    }

    #[test]
    fn style_checks_should_accept_used_private_members() {
        assert_eq!(
            codes(
                "<?php class A { private $x; private function h() {} public function f() { return $this->h() + $this->x; } }"
            ),
            []
        );
    }
}
