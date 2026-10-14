//! Control-flow context actions: inverting, merging and splitting `if`s,
//! adding and removing braces, and converting between `if`/`else`,
//! ternaries and `match`.

use super::expressions::negate;
use super::*;
use crate::inspection::unparen;

/// Collects the control-flow actions that apply at the caret.
pub(super) fn collect(ctx: &Context, out: &mut Vec<Fix>) {
    if let Some(statement) = if_at_head(ctx) {
        out.extend(invert_if(ctx, statement));
        out.extend(merge_nested_if(ctx, statement));
        out.extend(split_and_condition(ctx, statement));
        out.extend(if_to_ternary(ctx, statement));
    }
    out.extend(braces(ctx));
    out.extend(ternary_to_if(ctx));
    out.extend(switch_to_match(ctx));
}

/// The `if` statement whose head (keyword or condition, not its body) is
/// at the caret.
fn if_at_head<'t>(ctx: &Context<'t>) -> Option<Node<'t>> {
    ctx.ancestors()
        .into_iter()
        .filter(|n| n.kind() == "if_statement")
        .find(|n| {
            n.child_by_field_name("body")
                .is_some_and(|b| ctx.at < b.start_byte())
        })
}

/// The statements of a block (comments excluded), or the statement
/// itself when the body is not a block.
fn statements<'t>(body: Node<'t>) -> Vec<Node<'t>> {
    if body.kind() == "compound_statement" {
        let mut c = body.walk();
        body.named_children(&mut c)
            .filter(|n| n.kind() != "comment")
            .collect()
    } else {
        vec![body]
    }
}

/// Whether a block holds a comment (rewrites that drop the block skip it).
fn has_comment(body: Node) -> bool {
    let mut c = body.walk();
    body.named_children(&mut c).any(|n| n.kind() == "comment")
}

/// The condition of an `if` without its parentheses.
fn condition<'t>(statement: Node<'t>) -> Option<Node<'t>> {
    statement.child_by_field_name("condition").map(unparen)
}

/// The `else` clause of an `if` that has no `elseif`, with a block body.
fn plain_else<'t>(statement: Node<'t>) -> Option<Node<'t>> {
    let mut c = statement.walk();
    let alternatives: Vec<Node> = statement
        .children_by_field_name("alternative", &mut c)
        .collect();
    match alternatives.as_slice() {
        [only] if only.kind() == "else_clause" => only
            .child_by_field_name("body")
            .filter(|b| b.kind() == "compound_statement")
            .map(|_| *only),
        _ => None,
    }
}

/// `text` with every line after the first moved from `from` indentation
/// to `to`.
fn reindent(text: &str, from: &str, to: &str) -> String {
    let mut lines = text.split('\n');
    let mut out = lines.next().unwrap_or("").to_string();
    for line in lines {
        out.push('\n');
        match line.strip_prefix(from) {
            Some(rest) => {
                out.push_str(to);
                out.push_str(rest);
            }
            None => out.push_str(line),
        }
    }
    out
}

/// `if (c) { A } else { B }` to `if (!c) { B } else { A }`.
fn invert_if(ctx: &Context, statement: Node) -> Option<Fix> {
    let body = statement
        .child_by_field_name("body")
        .filter(|b| b.kind() == "compound_statement")?;
    let else_body = plain_else(statement)?.child_by_field_name("body")?;
    let cond = condition(statement)?;
    Some(replace(
        Action::InvertIf,
        statement,
        format!(
            "if ({}) {} else {}",
            negate(cond, ctx.src),
            ctx.text(else_body),
            ctx.text(body)
        ),
    ))
}

/// An operand of `&&`, parenthesized when it binds more loosely.
fn and_operand(node: Node, src: &str) -> String {
    let loose = match node.kind() {
        "binary_expression" => node.child_by_field_name("operator").is_some_and(|o| {
            matches!(
                text(o, src).to_ascii_lowercase().as_str(),
                "||" | "or" | "and" | "xor" | "??"
            )
        }),
        "conditional_expression" | "assignment_expression" => true,
        _ => false,
    };
    if loose {
        format!("({})", text(node, src))
    } else {
        text(node, src).to_string()
    }
}

/// `if (a) { if (b) { X } }` to `if (a && b) { X }`.
fn merge_nested_if(ctx: &Context, outer: Node) -> Option<Fix> {
    let mut c = outer.walk();
    if outer
        .children_by_field_name("alternative", &mut c)
        .next()
        .is_some()
    {
        return None;
    }
    let body = outer
        .child_by_field_name("body")
        .filter(|b| b.kind() == "compound_statement" && !has_comment(*b))?;
    let [inner] = statements(body)[..] else {
        return None;
    };
    let mut ic = inner.walk();
    if inner.kind() != "if_statement"
        || inner
            .children_by_field_name("alternative", &mut ic)
            .next()
            .is_some()
    {
        return None;
    }
    let inner_body = inner.child_by_field_name("body")?;
    let indent = indent_at(ctx.src, outer.start_byte());
    let unit = indent_unit(ctx.src);
    Some(replace(
        Action::MergeNestedIf,
        outer,
        format!(
            "if ({} && {}) {}",
            and_operand(condition(outer)?, ctx.src),
            and_operand(condition(inner)?, ctx.src),
            reindent(ctx.text(inner_body), &format!("{indent}{unit}"), indent)
        ),
    ))
}

/// `if (a && b) { X }` to `if (a) { if (b) { X } }`.
fn split_and_condition(ctx: &Context, statement: Node) -> Option<Fix> {
    let mut c = statement.walk();
    if statement
        .children_by_field_name("alternative", &mut c)
        .next()
        .is_some()
    {
        return None;
    }
    let cond = condition(statement)?;
    let is_and = cond.kind() == "binary_expression"
        && cond
            .child_by_field_name("operator")
            .is_some_and(|o| ctx.text(o) == "&&");
    if !is_and {
        return None;
    }
    let (left, right) = (
        cond.child_by_field_name("left")?,
        cond.child_by_field_name("right")?,
    );
    let body = statement
        .child_by_field_name("body")
        .filter(|b| b.kind() == "compound_statement")?;
    let indent = indent_at(ctx.src, statement.start_byte());
    let unit = indent_unit(ctx.src);
    let inner_body = reindent(ctx.text(body), indent, &format!("{indent}{unit}"));
    Some(replace(
        Action::SplitAndCondition,
        statement,
        format!(
            "if ({}) {{\n{indent}{unit}if ({}) {inner_body}\n{indent}}}",
            ctx.text(unparen(left)),
            ctx.text(unparen(right))
        ),
    ))
}

/// The control statement (or `else`) around the caret and its body.
fn controlled_body<'t>(ctx: &Context<'t>) -> Option<(Node<'t>, Node<'t>)> {
    ctx.ancestors().into_iter().find_map(|n| {
        let controls = matches!(
            n.kind(),
            "if_statement"
                | "else_clause"
                | "else_if_clause"
                | "while_statement"
                | "for_statement"
                | "foreach_statement"
        );
        let body = n.child_by_field_name("body")?;
        (controls && ctx.at <= body.start_byte().max(n.start_byte() + 2)).then_some((n, body))
    })
}

/// Adds braces around a single-statement body, or removes them from a
/// block holding one statement.
fn braces(ctx: &Context) -> Option<Fix> {
    let (statement, body) = controlled_body(ctx)?;
    let before = body.prev_sibling()?.end_byte();
    let indent = indent_at(ctx.src, statement.start_byte());
    let unit = indent_unit(ctx.src);
    match body.kind() {
        "compound_statement" => {
            let [single] = statements(body)[..] else {
                return None;
            };
            // A nested `if` without braces could capture a following `else`.
            if has_comment(body)
                || single.kind() == "if_statement"
                || ctx.text(single).contains('\n')
            {
                return None;
            }
            Some(rewrite(
                Action::RemoveBraces,
                before,
                body.end_byte(),
                format!("\n{indent}{unit}{}", ctx.text(single)),
            ))
        }
        "colon_block" | "if_statement" => None,
        _ => Some(rewrite(
            Action::AddBraces,
            before,
            body.end_byte(),
            format!(" {{\n{indent}{unit}{}\n{indent}}}", ctx.text(body)),
        )),
    }
}

/// What a one-statement branch does: return a value or assign one.
enum Branch<'t> {
    /// `return <value>;`
    Return(Node<'t>),
    /// `<target> = <value>;`
    Assign(&'t str, Node<'t>),
}

/// The branch kind of a block holding one `return` or assignment.
fn branch<'t>(ctx: &Context<'t>, body: Node<'t>) -> Option<Branch<'t>> {
    let [single] = statements(body)[..] else {
        return None;
    };
    match single.kind() {
        "return_statement" => single.named_child(0).map(Branch::Return),
        "expression_statement" => {
            let a = single
                .named_child(0)
                .filter(|a| a.kind() == "assignment_expression")?;
            Some(Branch::Assign(
                ctx.text(a.child_by_field_name("left")?),
                a.child_by_field_name("right")?,
            ))
        }
        _ => None,
    }
}

/// A ternary branch, parenthesized when it is itself a ternary or lower.
fn ternary_part(node: Node, src: &str) -> String {
    if matches!(
        node.kind(),
        "conditional_expression" | "assignment_expression"
    ) {
        format!("({})", text(node, src))
    } else {
        text(node, src).to_string()
    }
}

/// `if (c) { return a; } else { return b; }` to `return c ? a : b;`.
fn if_to_ternary(ctx: &Context, statement: Node) -> Option<Fix> {
    let body = statement.child_by_field_name("body")?;
    let else_body = plain_else(statement)?.child_by_field_name("body")?;
    if has_comment(body) || has_comment(else_body) {
        return None;
    }
    let cond = ternary_part(condition(statement)?, ctx.src);
    let text = match (branch(ctx, body)?, branch(ctx, else_body)?) {
        (Branch::Return(a), Branch::Return(b)) => format!(
            "return {cond} ? {} : {};",
            ternary_part(a, ctx.src),
            ternary_part(b, ctx.src)
        ),
        (Branch::Assign(t1, a), Branch::Assign(t2, b)) if t1 == t2 => format!(
            "{t1} = {cond} ? {} : {};",
            ternary_part(a, ctx.src),
            ternary_part(b, ctx.src)
        ),
        _ => return None,
    };
    Some(replace(Action::IfToTernary, statement, text))
}

/// `return c ? a : b;` (or an assignment of one) to `if`/`else`.
fn ternary_to_if(ctx: &Context) -> Option<Fix> {
    let statement = ctx.enclosing(&["return_statement", "expression_statement"], false)?;
    if !statement
        .parent()
        .is_some_and(|p| matches!(p.kind(), "compound_statement" | "program"))
    {
        return None;
    }
    let (prefix, value) = match statement.kind() {
        "return_statement" => ("return ".to_string(), statement.named_child(0)?),
        _ => {
            let a = statement
                .named_child(0)
                .filter(|a| a.kind() == "assignment_expression")?;
            (
                format!("{} = ", ctx.text(a.child_by_field_name("left")?)),
                a.child_by_field_name("right")?,
            )
        }
    };
    let ternary = unparen(value);
    if ternary.kind() != "conditional_expression" {
        return None;
    }
    let (cond, then, otherwise) = (
        ternary.child_by_field_name("condition")?,
        ternary.child_by_field_name("body")?,
        ternary.child_by_field_name("alternative")?,
    );
    let indent = indent_at(ctx.src, statement.start_byte());
    let unit = indent_unit(ctx.src);
    Some(replace(
        Action::TernaryToIf,
        statement,
        format!(
            "if ({}) {{\n{indent}{unit}{prefix}{};\n{indent}}} else {{\n{indent}{unit}{prefix}{};\n{indent}}}",
            ctx.text(unparen(cond)),
            ctx.text(then),
            ctx.text(otherwise)
        ),
    ))
}

/// A `switch` whose cases each return a value, or each assign the same
/// variable and break, to a `match`. A `default` is required: `match`
/// throws where `switch` would fall out.
fn switch_to_match(ctx: &Context) -> Option<Fix> {
    let switch = ctx
        .ancestors()
        .into_iter()
        .find(|n| n.kind() == "switch_statement")?;
    let block = switch.child_by_field_name("body")?;
    if ctx.at >= block.start_byte() {
        return None;
    }
    let subject = unparen(switch.child_by_field_name("condition")?);
    let mut c = block.walk();
    let cases: Vec<Node> = block.named_children(&mut c).collect();
    let mut arms: Vec<String> = Vec::new();
    let mut pending: Vec<&str> = Vec::new();
    let mut target: Option<&str> = None;
    let mut returns = None;
    let mut has_default = false;
    for case in cases {
        if case.kind() == "comment" {
            return None;
        }
        let value = case.child_by_field_name("value");
        let label = match value {
            Some(v) => ctx.text(v),
            None => {
                has_default = true;
                "default"
            }
        };
        let mut cc = case.walk();
        let mut body: Vec<Node> = case
            .named_children(&mut cc)
            .filter(|n| Some(n.id()) != value.map(|v| v.id()))
            .collect();
        if body.iter().any(|n| n.kind() == "comment") {
            return None;
        }
        if body.is_empty() {
            pending.push(label);
            continue;
        }
        if body.last().is_some_and(|n| n.kind() == "break_statement") {
            body.pop();
        }
        let [single] = body[..] else {
            return None;
        };
        let (is_return, result) = match single.kind() {
            "return_statement" => (true, single.named_child(0)?),
            "expression_statement" => {
                let a = single
                    .named_child(0)
                    .filter(|a| a.kind() == "assignment_expression")?;
                let left = ctx.text(a.child_by_field_name("left")?);
                if *target.get_or_insert(left) != left {
                    return None;
                }
                (false, a.child_by_field_name("right")?)
            }
            _ => return None,
        };
        if *returns.get_or_insert(is_return) != is_return {
            return None;
        }
        pending.push(label);
        if pending.contains(&"default") && pending.len() > 1 {
            return None;
        }
        arms.push(format!("{} => {}", pending.join(", "), ctx.text(result)));
        pending.clear();
    }
    if !has_default || !pending.is_empty() || arms.is_empty() {
        return None;
    }
    let indent = indent_at(ctx.src, switch.start_byte());
    let unit = indent_unit(ctx.src);
    let lines: String = arms
        .iter()
        .map(|a| format!("{indent}{unit}{a},\n"))
        .collect();
    let lead = match (returns, target) {
        (Some(true), _) => "return ".to_string(),
        (_, Some(t)) => format!("{t} = "),
        _ => return None,
    };
    Some(replace(
        Action::SwitchToMatch,
        switch,
        format!("{lead}match ({}) {{\n{lines}{indent}}};", ctx.text(subject)),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::apply;
    use super::*;

    #[test]
    fn if_else_should_invert() {
        assert_eq!(
            apply(
                "<?php i|f ($a > 1) { f(); } else { g(); }",
                Action::InvertIf
            )
            .as_deref(),
            Some("<?php if ($a <= 1) { g(); } else { f(); }")
        );
    }

    #[test]
    fn nested_ifs_should_merge() {
        let src = "<?php\nif| ($a || $b) {\n    if ($c) {\n        f();\n    }\n}";
        assert_eq!(
            apply(src, Action::MergeNestedIf).as_deref(),
            Some("<?php\nif (($a || $b) && $c) {\n    f();\n}")
        );
    }

    #[test]
    fn and_condition_should_split() {
        let src = "<?php\nif| ($a && $b) {\n    f();\n}";
        assert_eq!(
            apply(src, Action::SplitAndCondition).as_deref(),
            Some("<?php\nif ($a) {\n    if ($b) {\n        f();\n    }\n}")
        );
    }

    #[test]
    fn braces_should_be_added() {
        let src = "<?php\nif| ($a)\n    f();";
        assert_eq!(
            apply(src, Action::AddBraces).as_deref(),
            Some("<?php\nif ($a) {\n    f();\n}")
        );
    }

    #[test]
    fn braces_should_be_removed() {
        let src = "<?php\nwhile| ($a) {\n    f();\n}";
        assert_eq!(
            apply(src, Action::RemoveBraces).as_deref(),
            Some("<?php\nwhile ($a)\n    f();")
        );
    }

    #[test]
    fn returning_if_should_become_ternary() {
        let src = "<?php function f($a) { i|f ($a) { return 1; } else { return 2; } }";
        assert_eq!(
            apply(src, Action::IfToTernary).as_deref(),
            Some("<?php function f($a) { return $a ? 1 : 2; }")
        );
    }

    #[test]
    fn assigned_ternary_should_become_if() {
        let src = "<?php\n$x = $a| ? 1 : 2;";
        assert_eq!(
            apply(src, Action::TernaryToIf).as_deref(),
            Some("<?php\nif ($a) {\n    $x = 1;\n} else {\n    $x = 2;\n}")
        );
    }

    #[test]
    fn returning_switch_should_become_match() {
        let src = "<?php\nfunction f($a) {\nswi|tch ($a) {\n    case 1:\n    case 2:\n        return 'low';\n    case 3:\n        return 'mid';\n    default:\n        return 'high';\n}\n}";
        assert_eq!(
            apply(src, Action::SwitchToMatch).as_deref(),
            Some(
                "<?php\nfunction f($a) {\nreturn match ($a) {\n    1, 2 => 'low',\n    3 => 'mid',\n    default => 'high',\n};\n}"
            )
        );
    }

    #[test]
    fn switch_without_default_should_not_become_match() {
        let src = "<?php sw|itch ($a) { case 1: $x = 1; break; }";
        assert_eq!(apply(src, Action::SwitchToMatch), None);
    }
}
