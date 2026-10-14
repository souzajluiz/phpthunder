//! Redundant and suspicious code found from the syntax tree: overrides
//! that only call the parent, fully qualified names their short name
//! already means, loops that never loop, conditions already checked,
//! duplicated `case`/`match` bodies and trailing `return;`/`continue;`.

use super::*;
use crate::syntax::{context_at, text};
use std::collections::HashMap;
use tree_sitter::Node;

/// Runs every smell rule over the tree rooted at `root`.
///
/// Called by [`diagnostics`] on each inspection pass.
pub(crate) fn smell_checks(root: Node, src: &str, out: &mut Findings) {
    for node in descendants(root) {
        if node.is_error() {
            continue;
        }
        match node.kind() {
            "method_declaration" => {
                redundant_override(node, src, out);
                trailing_return(node, src, out);
            }
            "function_definition" => trailing_return(node, src, out),
            "qualified_name" => unnecessary_fqn(root, node, src, out),
            "namespace_use_clause" => {
                if text(node, src).starts_with('\\') {
                    out.add(node, Rule::LeadingBackslashInUse, "");
                }
            }
            "while_statement" | "for_statement" | "do_statement" => loop_never_iterates(node, out),
            "foreach_statement" => trailing_continue(node, src, out),
            "if_statement" => condition_already_checked(node, src, out),
            "switch_block" => duplicate_case_bodies(node, src, out),
            "match_block" => duplicate_arm_bodies(node, src, out),
            _ => {}
        }
        if matches!(
            node.kind(),
            "while_statement" | "for_statement" | "do_statement"
        ) {
            trailing_continue(node, src, out);
        }
    }
}

/// `public function f($a) { return parent::f($a); }`.
fn redundant_override(method: Node, src: &str, out: &mut Findings) {
    let (Some(name), Some(params), Some(body)) = (
        method.child_by_field_name("name"),
        method.child_by_field_name("parameters"),
        method.child_by_field_name("body"),
    ) else {
        return;
    };
    // Attributes and doc comments may change behavior or document intent;
    // a constructor or a default value may narrow the parent's signature.
    let narrows = text(name, src).eq_ignore_ascii_case("__construct")
        || descendants(params)
            .iter()
            .any(|p| p.child_by_field_name("default_value").is_some());
    if narrows
        || method.child_by_field_name("attributes").is_some()
        || method
            .prev_named_sibling()
            .is_some_and(|p| p.kind() == "comment")
    {
        return;
    }
    let mut c = body.walk();
    let statements: Vec<Node> = body.named_children(&mut c).collect();
    let [single] = statements[..] else {
        return;
    };
    let call = match single.kind() {
        "return_statement" | "expression_statement" => single.named_child(0),
        _ => None,
    };
    let Some(call) = call.filter(|c| c.kind() == "scoped_call_expression") else {
        return;
    };
    let calls_parent = call
        .child_by_field_name("scope")
        .is_some_and(|s| text(s, src).eq_ignore_ascii_case("parent"))
        && call
            .child_by_field_name("name")
            .is_some_and(|n| text(n, src).eq_ignore_ascii_case(text(name, src)));
    let mut pc = params.walk();
    let declared: Vec<&str> = params
        .named_children(&mut pc)
        .filter_map(|p| p.child_by_field_name("name"))
        .map(|n| text(n, src))
        .collect();
    let passed: Option<Vec<&str>> =
        call_args(call).map(|a| a.iter().map(|n| text(*n, src)).collect());
    if calls_parent && passed.is_some_and(|p| p == declared) {
        out.add(name, Rule::RedundantMethodOverride, "");
    }
}

/// A `return;` ending a function body.
fn trailing_return(function: Node, src: &str, out: &mut Findings) {
    let Some(body) = function.child_by_field_name("body") else {
        return;
    };
    let mut c = body.walk();
    let last = body
        .named_children(&mut c)
        .filter(|n| n.kind() != "comment")
        .last();
    if let Some(ret) = last.filter(|r| r.kind() == "return_statement" && r.named_child_count() == 0)
    {
        out.add(
            ret,
            Rule::UnnecessaryStopStatement,
            text(ret, src).trim_end_matches(';'),
        );
    }
}

/// A `continue;` ending a loop body.
fn trailing_continue(loop_node: Node, src: &str, out: &mut Findings) {
    let Some(body) = loop_node
        .child_by_field_name("body")
        .filter(|b| b.kind() == "compound_statement")
    else {
        return;
    };
    let mut c = body.walk();
    let last = body
        .named_children(&mut c)
        .filter(|n| n.kind() != "comment")
        .last();
    if let Some(cont) =
        last.filter(|r| r.kind() == "continue_statement" && r.named_child_count() == 0)
    {
        out.add(
            cont,
            Rule::UnnecessaryStopStatement,
            text(cont, src).trim_end_matches(';'),
        );
    }
}

/// Whether a qualified name stands for a class (not a function, constant
/// or namespace).
fn is_class_position(node: Node) -> bool {
    node.parent().is_some_and(|p| {
        matches!(
            p.kind(),
            "named_type"
                | "object_creation_expression"
                | "base_clause"
                | "class_interface_clause"
                | "catch_clause"
                | "type_list"
                | "optional_type"
                | "union_type"
        ) || (matches!(
            p.kind(),
            "scoped_call_expression" | "class_constant_access_expression"
        ) && p.named_child(0).is_some_and(|s| s.id() == node.id()))
            || (p.kind() == "binary_expression"
                && p.child_by_field_name("right")
                    .is_some_and(|r| r.id() == node.id()))
    })
}

/// `\App\Models\User` where `User` already means that class.
fn unnecessary_fqn(root: Node, name: Node, src: &str, out: &mut Findings) {
    if !is_class_position(name) {
        return;
    }
    let written = text(name, src);
    let Some(short) = written.rsplit('\\').next().filter(|s| !s.is_empty()) else {
        return;
    };
    let ctx = context_at(root, src, name.start_byte());
    let fqn = ctx.resolve_class(written);
    if ctx.resolve_class(short).eq_ignore_ascii_case(&fqn) {
        out.add(name, Rule::UnnecessaryFullyQualifiedName, written);
    }
}

/// A `while`/`for`/`do` loop whose body always leaves at its end and
/// never continues: it runs at most once.
fn loop_never_iterates(loop_node: Node, out: &mut Findings) {
    let Some(body) = loop_node
        .child_by_field_name("body")
        .filter(|b| b.kind() == "compound_statement")
    else {
        return;
    };
    let mut c = body.walk();
    let last = body
        .named_children(&mut c)
        .filter(|n| n.kind() != "comment")
        .last();
    let leaves = last.is_some_and(|l| {
        matches!(l.kind(), "return_statement" | "break_statement")
            || (l.kind() == "expression_statement"
                && l.named_child(0)
                    .is_some_and(|e| e.kind() == "throw_expression"))
    });
    // A `continue` anywhere in this loop (not in a nested one) iterates.
    let continues = {
        let mut stack = vec![body];
        let mut found = false;
        while let Some(n) = stack.pop() {
            match n.kind() {
                "continue_statement" => found = true,
                "while_statement"
                | "for_statement"
                | "foreach_statement"
                | "do_statement"
                | "anonymous_function"
                | "arrow_function"
                | "function_definition"
                    if n.id() != body.id() => {}
                _ => {
                    let mut c = n.walk();
                    stack.extend(n.named_children(&mut c));
                }
            }
        }
        found
    };
    if leaves && !continues {
        out.add(
            loop_node.child(0).unwrap_or(loop_node),
            Rule::LoopNeverIterates,
            "",
        );
    }
}

/// `if ($a) { if ($a) { … } }`: the inner check always passes, unless a
/// variable in the condition is assigned in between.
fn condition_already_checked(statement: Node, src: &str, out: &mut Findings) {
    let Some(cond) = statement.child_by_field_name("condition") else {
        return;
    };
    let key: String = text(cond, src).split_whitespace().collect();
    if !is_pure(cond) {
        return;
    }
    let vars: Vec<&str> = descendants(cond)
        .into_iter()
        .filter(|n| n.kind() == "variable_name")
        .map(|n| text(n, src))
        .collect();
    let mut child = statement;
    while let Some(parent) = child.parent() {
        if matches!(
            parent.kind(),
            "function_definition" | "method_declaration" | "anonymous_function" | "arrow_function"
        ) {
            return;
        }
        let in_body = parent.kind() == "if_statement"
            && parent
                .child_by_field_name("body")
                .is_some_and(|b| b.byte_range().contains(&statement.start_byte()));
        if in_body && let Some(outer) = parent.child_by_field_name("condition") {
            let same: String = text(outer, src).split_whitespace().collect();
            let body = parent.child_by_field_name("body");
            let reassigned = body.is_some_and(|b| {
                descendants(b).iter().any(|n| {
                    n.start_byte() < statement.start_byte()
                        && matches!(
                            n.kind(),
                            "assignment_expression"
                                | "augmented_assignment_expression"
                                | "update_expression"
                        )
                        && n.named_child(0)
                            .is_some_and(|l| vars.contains(&text(l, src)))
                })
            });
            if same == key && !reassigned {
                out.add(cond, Rule::ConditionAlreadyChecked, "");
            }
            return;
        }
        child = parent;
    }
}

/// Non-empty `case` bodies repeated in one `switch`.
fn duplicate_case_bodies(block: Node, src: &str, out: &mut Findings) {
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut c = block.walk();
    for case in block.named_children(&mut c) {
        let value = case.child_by_field_name("value");
        let mut cc = case.walk();
        let body: String = case
            .named_children(&mut cc)
            .filter(|n| Some(n.id()) != value.map(|v| v.id()) && n.kind() != "comment")
            .map(|n| text(n, src).split_whitespace().collect::<String>())
            .collect();
        // One-statement bodies (`break;`, `return null;`) repeat naturally.
        let statements = {
            let mut cc = case.walk();
            case.named_children(&mut cc)
                .filter(|n| Some(n.id()) != value.map(|v| v.id()) && n.kind() != "comment")
                .count()
        };
        if statements < 2 {
            continue;
        }
        let label = value.map_or("default".to_string(), |v| text(v, src).to_string());
        match seen.get(&body) {
            Some(first) => out.add(
                case.child(0).unwrap_or(case),
                Rule::DuplicateCaseBody,
                first.clone(),
            ),
            None => {
                seen.insert(body, label);
            }
        }
    }
}

/// `match` arms with the same non-trivial result.
fn duplicate_arm_bodies(block: Node, src: &str, out: &mut Findings) {
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut c = block.walk();
    for arm in block.named_children(&mut c) {
        let Some(result) = arm.child_by_field_name("return_expression").or_else(|| {
            arm.named_child(
                arm.named_child_count()
                    .checked_sub(1)
                    .and_then(|i| u32::try_from(i).ok())?,
            )
        }) else {
            continue;
        };
        // Literals repeat naturally (`1, 2 => true`, `3 => true`).
        if matches!(
            result.kind(),
            "string" | "encapsed_string" | "integer" | "float" | "boolean" | "null"
        ) || arm.kind() == "match_default_expression"
        {
            continue;
        }
        let key: String = text(result, src).split_whitespace().collect();
        let label = arm
            .child_by_field_name("conditional_expressions")
            .map_or("default", |c| text(c, src))
            .to_string();
        match seen.get(&key) {
            Some(first) => out.add(result, Rule::DuplicateArmBody, first.clone()),
            None => {
                seen.insert(key, label);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    fn rules(src: &str) -> Vec<Rule> {
        let tree = parse(src);
        let mut out = Findings::default();
        smell_checks(tree.root_node(), src, &mut out);
        out.list.iter().map(|f| f.rule).collect()
    }

    #[test]
    fn override_calling_only_parent_should_be_redundant() {
        assert_eq!(
            rules(
                "<?php class A extends B { public function f($a, $b) { return parent::f($a, $b); } }"
            ),
            [Rule::RedundantMethodOverride]
        );
    }

    #[test]
    fn override_changing_arguments_should_be_kept() {
        assert_eq!(
            rules("<?php class A extends B { public function f($a) { return parent::f($a, 1); } }"),
            []
        );
    }

    #[test]
    fn imported_fqn_should_be_unnecessary() {
        assert_eq!(
            rules("<?php namespace App; use Lib\\User; function f(\\Lib\\User $u) {}"),
            [Rule::UnnecessaryFullyQualifiedName]
        );
    }

    #[test]
    fn same_namespace_fqn_should_be_unnecessary() {
        assert_eq!(
            rules("<?php namespace App; $a = new \\App\\Post();"),
            [Rule::UnnecessaryFullyQualifiedName]
        );
    }

    #[test]
    fn needed_fqn_should_be_kept() {
        assert_eq!(rules("<?php namespace App; $a = new \\Lib\\Post();"), []);
    }

    #[test]
    fn leading_backslash_in_use_should_be_flagged() {
        assert_eq!(
            rules("<?php use \\Lib\\Post;"),
            [Rule::LeadingBackslashInUse]
        );
    }

    #[test]
    fn loop_ending_in_return_should_never_iterate() {
        assert_eq!(
            rules("<?php while ($a) { f(); return 1; }"),
            [Rule::LoopNeverIterates]
        );
    }

    #[test]
    fn loop_with_continue_should_iterate() {
        assert_eq!(
            rules("<?php while ($a) { if ($b) { continue; } break; }"),
            []
        );
    }

    #[test]
    fn nested_same_condition_should_be_already_checked() {
        assert_eq!(
            rules("<?php if ($a > 1) { f(); if ($a > 1) { g(); } }"),
            [Rule::ConditionAlreadyChecked]
        );
    }

    #[test]
    fn condition_after_reassignment_should_be_kept() {
        assert_eq!(rules("<?php if ($a) { $a = f(); if ($a) { g(); } }"), []);
    }

    #[test]
    fn duplicate_case_body_should_be_flagged() {
        assert_eq!(
            rules("<?php switch ($x) { case 1: f(); break; case 2: f(); break; }"),
            [Rule::DuplicateCaseBody]
        );
    }

    #[test]
    fn duplicate_arm_body_should_be_flagged() {
        assert_eq!(
            rules("<?php $a = match ($x) { 1 => f($y), 2 => f($y), default => 0 };"),
            [Rule::DuplicateArmBody]
        );
    }

    #[test]
    fn trailing_return_should_be_unnecessary() {
        assert_eq!(
            rules("<?php function f() { g(); return; }"),
            [Rule::UnnecessaryStopStatement]
        );
    }

    #[test]
    fn trailing_continue_should_be_unnecessary() {
        assert_eq!(
            rules("<?php foreach ($a as $b) { f(); continue; }"),
            [Rule::UnnecessaryStopStatement]
        );
    }
}
