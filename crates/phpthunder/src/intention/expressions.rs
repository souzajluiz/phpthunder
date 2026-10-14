//! Expression-level context actions: quotes, interpolation, operand
//! order, strict and compound operators, increments, De Morgan's law,
//! array layout and arrow functions.

use super::*;
use crate::inspection::{descendants, is_pure, unparen};
use std::collections::HashSet;

/// Collects the expression actions that apply at the caret.
pub(super) fn collect(ctx: &Context, out: &mut Vec<Fix>) {
    out.extend(quotes(ctx));
    out.extend(concat_to_interpolation(ctx));
    out.extend(binary_actions(ctx));
    out.extend(assignment_actions(ctx));
    out.extend(de_morgan(ctx));
    out.extend(array_layout(ctx));
    out.extend(arrow_to_closure(ctx));
}

/// `node`'s source, parenthesized unless it is a primary expression.
pub(crate) fn atom(node: Node, src: &str) -> String {
    let simple = matches!(
        node.kind(),
        "variable_name"
            | "integer"
            | "float"
            | "string"
            | "encapsed_string"
            | "boolean"
            | "null"
            | "name"
            | "qualified_name"
            | "member_access_expression"
            | "nullsafe_member_access_expression"
            | "member_call_expression"
            | "nullsafe_member_call_expression"
            | "scoped_call_expression"
            | "function_call_expression"
            | "subscript_expression"
            | "class_constant_access_expression"
            | "scoped_property_access_expression"
            | "parenthesized_expression"
            | "unary_op_expression"
    );
    if simple {
        text(node, src).to_string()
    } else {
        format!("({})", text(node, src))
    }
}

/// The operator of a binary expression, as written.
fn operator<'a>(binary: Node, src: &'a str) -> Option<&'a str> {
    binary.child_by_field_name("operator").map(|o| text(o, src))
}

/// Whether a binary expression is a logical `&&`/`||`/`and`/`or`.
fn is_logical(node: Node, src: &str) -> bool {
    node.kind() == "binary_expression"
        && operator(node, src).is_some_and(|op| {
            matches!(op.to_ascii_lowercase().as_str(), "&&" | "||" | "and" | "or")
        })
}

/// The logical negation of `node`, written so it can stand as an operand
/// of `&&` or `||`: comparisons flip their operator, `!x` drops the `!`,
/// booleans swap, anything else gets a `!`.
pub(crate) fn negate(node: Node, src: &str) -> String {
    let node = unparen(node);
    match node.kind() {
        "unary_op_expression" if text(node, src).starts_with('!') => {
            let inner = node.child_by_field_name("argument").map(unparen);
            match inner {
                Some(i) if is_logical(i, src) => format!("({})", text(i, src)),
                Some(i) => text(i, src).to_string(),
                None => text(node, src).to_string(),
            }
        }
        "boolean" => {
            if text(node, src).eq_ignore_ascii_case("true") {
                "false".into()
            } else {
                "true".into()
            }
        }
        "binary_expression" => {
            let flipped = operator(node, src).and_then(|op| match op {
                "==" => Some("!="),
                "!=" | "<>" => Some("=="),
                "===" => Some("!=="),
                "!==" => Some("==="),
                "<" => Some(">="),
                ">" => Some("<="),
                "<=" => Some(">"),
                ">=" => Some("<"),
                _ => None,
            });
            match (
                flipped,
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ) {
                (Some(op), Some(l), Some(r)) => {
                    format!("{} {op} {}", text(l, src), text(r, src))
                }
                _ => format!("!({})", text(node, src)),
            }
        }
        _ => format!("!{}", atom(node, src)),
    }
}

/// Single- and double-quote conversions of a plain string literal.
fn quotes(ctx: &Context) -> Option<Fix> {
    let node = ctx.enclosing(&["string", "encapsed_string"], false)?;
    let written = ctx.text(node);
    let inner = written.get(1..written.len().checked_sub(1)?)?;
    if written.starts_with('\'') {
        let plain = !inner.contains(['"', '$'])
            && !inner.replace("\\'", "").replace("\\\\", "").contains('\\');
        plain.then(|| {
            replace(
                Action::ToDoubleQuotes,
                node,
                format!("\"{}\"", inner.replace("\\'", "'")),
            )
        })
    } else if written.starts_with('"') {
        let mut c = node.walk();
        let literal = node.named_children(&mut c).all(|ch| match ch.kind() {
            "string_content" => true,
            "escape_sequence" => matches!(ctx.text(ch), "\\\"" | "\\\\" | "\\$"),
            _ => false,
        });
        literal.then(|| {
            let converted = inner
                .replace("\\\"", "\"")
                .replace("\\$", "$")
                .replace('\'', "\\'");
            replace(Action::ToSingleQuotes, node, format!("'{converted}'"))
        })
    } else {
        None
    }
}

/// The operands of a left-associative `.` chain, in order.
fn concat_parts<'t>(node: Node<'t>, src: &str, out: &mut Vec<Node<'t>>) {
    if node.kind() == "binary_expression"
        && operator(node, src) == Some(".")
        && let (Some(l), Some(r)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        )
    {
        concat_parts(l, src, out);
        concat_parts(r, src, out);
        return;
    }
    out.push(node);
}

/// `'a' . $b . 'c'` to `"a{$b}c"` when every operand is a literal or an
/// interpolable expression.
fn concat_to_interpolation(ctx: &Context) -> Option<Fix> {
    let mut top = ctx.enclosing(&["binary_expression"], true)?;
    if operator(top, ctx.src) != Some(".") {
        return None;
    }
    while let Some(p) = top
        .parent()
        .filter(|p| p.kind() == "binary_expression" && operator(*p, ctx.src) == Some("."))
    {
        top = p;
    }
    let mut parts = Vec::new();
    concat_parts(top, ctx.src, &mut parts);
    let mut body = String::new();
    let (mut literals, mut values) = (0, 0);
    for part in parts {
        let written = ctx.text(part);
        match part.kind() {
            "string" if written.starts_with('\'') => {
                literals += 1;
                let inner = &written[1..written.len() - 1];
                body.push_str(
                    &inner
                        .replace("\\'", "'")
                        .replace('"', "\\\"")
                        .replace('$', "\\$"),
                );
            }
            "encapsed_string" | "string" if written.starts_with('"') => {
                literals += 1;
                body.push_str(&written[1..written.len() - 1]);
            }
            "variable_name"
            | "member_access_expression"
            | "member_call_expression"
            | "subscript_expression"
                if written.starts_with('$') && !written.contains(['"', '\'']) =>
            {
                values += 1;
                body.push('{');
                body.push_str(written);
                body.push('}');
            }
            _ => return None,
        }
    }
    (literals > 0 && values > 0)
        .then(|| replace(Action::ConcatToInterpolation, top, format!("\"{body}\"")))
}

/// Flip operands and strict comparison, on the binary at the caret.
fn binary_actions(ctx: &Context) -> Vec<Fix> {
    let Some(binary) = ctx.enclosing(&["binary_expression"], true) else {
        return Vec::new();
    };
    let (Some(op_node), Some(left), Some(right)) = (
        binary.child_by_field_name("operator"),
        binary.child_by_field_name("left"),
        binary.child_by_field_name("right"),
    ) else {
        return Vec::new();
    };
    let op = ctx.text(op_node);
    let mut out = Vec::new();
    let mirrored = match op {
        "<" => Some(">"),
        ">" => Some("<"),
        "<=" => Some(">="),
        ">=" => Some("<="),
        "==" | "===" | "!=" | "!==" | "<>" => Some(op),
        "&&" | "||" | "*" if is_pure(left) && is_pure(right) => Some(op),
        _ => None,
    };
    if let Some(m) = mirrored {
        out.push(replace(
            Action::FlipBinary,
            binary,
            format!("{} {m} {}", ctx.text(right), ctx.text(left)),
        ));
    }
    if matches!(op, "==" | "!=") {
        out.push(rewrite(
            Action::StrictComparison,
            op_node.start_byte(),
            op_node.end_byte(),
            format!("{op}="),
        ));
    }
    out
}

/// Compound assignments and increments, both ways.
fn assignment_actions(ctx: &Context) -> Vec<Fix> {
    let mut out = Vec::new();
    let Some(assignment) = ctx.enclosing(
        &["assignment_expression", "augmented_assignment_expression"],
        true,
    ) else {
        return out;
    };
    let (Some(left), Some(right)) = (
        assignment.child_by_field_name("left"),
        assignment.child_by_field_name("right"),
    ) else {
        return out;
    };
    let target = ctx.text(left);
    let statement_level = assignment
        .parent()
        .is_some_and(|p| p.kind() == "expression_statement");
    // The operator and the other operand of `$a = $a OP x` or `$a OP= x`.
    let (op, operand): (String, Node) = if assignment.kind() == "assignment_expression" {
        let r = unparen(right);
        let rewritable = r.kind() == "binary_expression"
            && r.child_by_field_name("left")
                .is_some_and(|l| ctx.text(l) == target);
        let Some(op) = operator(r, ctx.src).filter(|op| {
            rewritable
                && matches!(
                    *op,
                    "+" | "-" | "*" | "/" | "." | "%" | "**" | "??" | "|" | "&" | "^" | "<<" | ">>"
                )
        }) else {
            return out;
        };
        let Some(other) = r.child_by_field_name("right") else {
            return out;
        };
        out.push(replace(
            Action::ToCompoundAssignment,
            assignment,
            format!("{target} {op}= {}", ctx.text(other)),
        ));
        (op.to_string(), other)
    } else {
        let Some(op) = assignment
            .child_by_field_name("operator")
            .map(|o| ctx.text(o).trim_end_matches('=').to_string())
        else {
            return out;
        };
        out.push(replace(
            Action::ExpandCompoundAssignment,
            assignment,
            format!("{target} = {target} {op} {}", atom(right, ctx.src)),
        ));
        (op, right)
    };
    if statement_level && matches!(op.as_str(), "+" | "-") && ctx.text(operand) == "1" {
        let step = if op == "+" { "++" } else { "--" };
        out.push(replace(
            Action::ToIncrement,
            assignment,
            format!("{target}{step}"),
        ));
    }
    out
}

/// De Morgan's law on `!(a && b)` or on a bare `a && b`.
fn de_morgan(ctx: &Context) -> Option<Fix> {
    let binary = ctx.enclosing(&["binary_expression"], true)?;
    let mut top = binary;
    // Look through parentheses to a negation around the expression.
    while let Some(p) = top
        .parent()
        .filter(|p| p.kind() == "parenthesized_expression")
    {
        top = p;
    }
    let negated = top
        .parent()
        .filter(|p| p.kind() == "unary_op_expression" && ctx.text(*p).starts_with('!'));
    let op = operator(binary, ctx.src)?.to_ascii_lowercase();
    let flipped = match op.as_str() {
        "&&" => "||",
        "||" => "&&",
        _ => return None,
    };
    let (left, right) = (
        binary.child_by_field_name("left")?,
        binary.child_by_field_name("right")?,
    );
    let body = format!(
        "{} {flipped} {}",
        negate(left, ctx.src),
        negate(right, ctx.src)
    );
    match negated {
        Some(not) => {
            let wrap = not
                .parent()
                .is_some_and(|p| p.kind() == "binary_expression");
            let text = if wrap { format!("({body})") } else { body };
            Some(replace(Action::DeMorgan, not, text))
        }
        None => Some(replace(Action::DeMorgan, binary, format!("!({body})"))),
    }
}

/// Array elements of an array literal, as source.
fn elements<'t>(array: Node<'t>) -> Vec<Node<'t>> {
    let mut c = array.walk();
    array
        .named_children(&mut c)
        .filter(|n| n.kind() == "array_element_initializer")
        .collect()
}

/// One element per line, or all on one line.
fn array_layout(ctx: &Context) -> Option<Fix> {
    let array = ctx.enclosing(&["array_creation_expression"], true)?;
    let items = elements(array);
    let written = ctx.text(array);
    let (open, close) = if written.starts_with('[') {
        ("[", "]")
    } else {
        let paren = written.find('(')?;
        (&written[..=paren], ")")
    };
    let parts: Vec<&str> = items.iter().map(|e| ctx.text(*e)).collect();
    if written.contains('\n') {
        let has_comment = descendants(array).iter().any(|n| n.kind() == "comment");
        (!has_comment).then(|| {
            replace(
                Action::JoinArray,
                array,
                format!("{open}{}{close}", parts.join(", ")),
            )
        })
    } else if parts.len() >= 2 {
        let indent = indent_at(ctx.src, array.start_byte());
        let unit = indent_unit(ctx.src);
        let lines: String = parts
            .iter()
            .map(|p| format!("{indent}{unit}{p},\n"))
            .collect();
        Some(replace(
            Action::SplitArray,
            array,
            format!("{open}\n{lines}{indent}{close}"),
        ))
    } else {
        None
    }
}

/// `fn ($x) => $x + $y` to `function ($x) use ($y) { return $x + $y; }`.
fn arrow_to_closure(ctx: &Context) -> Option<Fix> {
    let arrow = ctx.enclosing(&["arrow_function"], false)?;
    let params = arrow.child_by_field_name("parameters")?;
    let body = arrow.child_by_field_name("body")?;
    let head = &ctx.src[arrow.start_byte()..params.start_byte()];
    let is_static = head.trim_start().starts_with("static");
    let by_ref = head.contains('&');
    let ret = arrow
        .child_by_field_name("return_type")
        .map(|t| format!(": {}", ctx.text(t)))
        .unwrap_or_default();
    let declared: HashSet<&str> = descendants(params)
        .into_iter()
        .filter(|n| n.kind() == "variable_name")
        .map(|n| ctx.text(n))
        .collect();
    let mut captured: Vec<&str> = Vec::new();
    for n in descendants(body) {
        let name = ctx.text(n);
        if n.kind() == "variable_name"
            && name != "$this"
            && !declared.contains(name)
            && !name.starts_with("$_")
            && name != "$GLOBALS"
            && !captured.contains(&name)
            && n.parent()
                .is_none_or(|p| p.kind() != "scoped_property_access_expression")
        {
            captured.push(name);
        }
    }
    let uses = if captured.is_empty() {
        String::new()
    } else {
        format!(" use ({})", captured.join(", "))
    };
    Some(replace(
        Action::ArrowToClosure,
        arrow,
        format!(
            "{}function {}{}{uses}{ret} {{ return {}; }}",
            if is_static { "static " } else { "" },
            if by_ref { "&" } else { "" },
            ctx.text(params),
            ctx.text(body)
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::apply;
    use super::*;

    #[test]
    fn single_quotes_should_become_double() {
        assert_eq!(
            apply("<?php $a = 'it\\'s|';", Action::ToDoubleQuotes).as_deref(),
            Some("<?php $a = \"it's\";")
        );
    }

    #[test]
    fn string_with_dollar_should_not_become_double() {
        assert_eq!(apply("<?php $a = '$x|';", Action::ToDoubleQuotes), None);
    }

    #[test]
    fn double_quotes_should_become_single() {
        assert_eq!(
            apply("<?php $a = \"it's|\";", Action::ToSingleQuotes).as_deref(),
            Some("<?php $a = 'it\\'s';")
        );
    }

    #[test]
    fn interpolated_string_should_not_become_single() {
        assert_eq!(
            apply("<?php $a = \"hi $x|\";", Action::ToSingleQuotes),
            None
        );
    }

    #[test]
    fn concatenation_should_become_interpolation() {
        assert_eq!(
            apply(
                "<?php $a = 'Hi ' .| $user->name . '!';",
                Action::ConcatToInterpolation
            )
            .as_deref(),
            Some("<?php $a = \"Hi {$user->name}!\";")
        );
    }

    #[test]
    fn concatenation_with_call_should_not_be_offered() {
        assert_eq!(
            apply("<?php $a = 'Hi ' .| f();", Action::ConcatToInterpolation),
            None
        );
    }

    #[test]
    fn comparison_should_flip() {
        assert_eq!(
            apply("<?php $a = $x <| 10;", Action::FlipBinary).as_deref(),
            Some("<?php $a = 10 > $x;")
        );
    }

    #[test]
    fn loose_comparison_should_become_strict() {
        assert_eq!(
            apply("<?php $a = $x =|= 10;", Action::StrictComparison).as_deref(),
            Some("<?php $a = $x === 10;")
        );
    }

    #[test]
    fn assignment_should_become_compound() {
        assert_eq!(
            apply("<?php $a| = $a . 'x';", Action::ToCompoundAssignment).as_deref(),
            Some("<?php $a .= 'x';")
        );
    }

    #[test]
    fn compound_assignment_should_expand() {
        assert_eq!(
            apply("<?php $a |*= $b + 1;", Action::ExpandCompoundAssignment).as_deref(),
            Some("<?php $a = $a * ($b + 1);")
        );
    }

    #[test]
    fn plus_one_should_become_increment() {
        assert_eq!(
            apply("<?php $i |+= 1;", Action::ToIncrement).as_deref(),
            Some("<?php $i++;")
        );
    }

    #[test]
    fn negated_and_should_apply_de_morgan() {
        assert_eq!(
            apply("<?php $a = !($x &|& $y > 1);", Action::DeMorgan).as_deref(),
            Some("<?php $a = !$x || $y <= 1;")
        );
    }

    #[test]
    fn bare_or_should_apply_de_morgan() {
        assert_eq!(
            apply("<?php $a = $x |‸| !$y;", Action::DeMorgan).as_deref(),
            Some("<?php $a = !(!$x && $y);")
        );
    }

    #[test]
    fn array_should_split_onto_lines() {
        assert_eq!(
            apply("<?php\n    $a = [1, |2];", Action::SplitArray).as_deref(),
            Some("<?php\n    $a = [\n        1,\n        2,\n    ];")
        );
    }

    #[test]
    fn multiline_array_should_join() {
        assert_eq!(
            apply("<?php $a = [\n    1,|\n    2,\n];", Action::JoinArray).as_deref(),
            Some("<?php $a = [1, 2];")
        );
    }

    #[test]
    fn arrow_function_should_become_closure_capturing_outer_variables() {
        assert_eq!(
            apply("<?php $f = f|n ($x) => $x + $y;", Action::ArrowToClosure).as_deref(),
            Some("<?php $f = function ($x) use ($y) { return $x + $y; };")
        );
    }
}
