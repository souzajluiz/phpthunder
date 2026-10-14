//! Probable bugs found from the syntax tree alone: fall-through cases,
//! duplicate `match` conditions, division by zero, loop variables that
//! clobber their array, results of printing functions used as values,
//! expressions whose value is thrown away, PHP 8 removals and
//! deprecations, wrong magic method modifiers, and `sprintf` formats that
//! disagree with their arguments.

use super::*;
use crate::syntax::text;
use std::collections::HashSet;
use tree_sitter::Node;

/// Magic methods PHP requires to be public.
const PUBLIC_MAGIC: &[&str] = &[
    "__call",
    "__callstatic",
    "__get",
    "__set",
    "__isset",
    "__unset",
    "__tostring",
    "__invoke",
    "__debuginfo",
    "__serialize",
    "__unserialize",
    "__set_state",
    "__sleep",
    "__wakeup",
];

/// Magic methods PHP requires to be static; every other magic method must
/// not be.
const STATIC_MAGIC: &[&str] = &["__callstatic", "__set_state"];

/// Magic methods PHP requires to be instance methods.
const INSTANCE_MAGIC: &[&str] = &[
    "__construct",
    "__destruct",
    "__call",
    "__get",
    "__set",
    "__isset",
    "__unset",
    "__tostring",
    "__invoke",
    "__clone",
    "__debuginfo",
    "__serialize",
    "__unserialize",
    "__sleep",
    "__wakeup",
];

/// Functions returning a position or key that may be `0`.
const SEARCH_FUNCTIONS: &[&str] = &[
    "strpos",
    "stripos",
    "strrpos",
    "strripos",
    "mb_strpos",
    "mb_stripos",
    "mb_strrpos",
    "array_search",
];

/// Runs every probable-bug rule over the tree rooted at `root`.
///
/// Called by [`diagnostics`] on each inspection pass.
pub(crate) fn bug_checks(root: Node, src: &str, out: &mut Findings) {
    for node in descendants(root) {
        if node.is_error() {
            continue;
        }
        match node.kind() {
            "switch_block" => missing_break(node, src, out),
            "match_block" => duplicate_match_conditions(node, src, out),
            "formal_parameters" => parameter_defaults(node, src, out),
            "binary_expression" => division_by_zero(node, src, out),
            "foreach_statement" => foreach_variables(node, src, out),
            "function_call_expression" => calls(node, src, out),
            "expression_statement" => expression_unused(node, src, out),
            "continue_statement" => continue_in_switch(node, src, out),
            "catch_clause" => rethrown(node, src, out),
            "conditional_expression" => nested_ternary(node, out),
            "dynamic_variable_name" => dollar_brace(node, src, out),
            "method_declaration" => {
                magic_modifiers(node, src, out);
                redundant_promoted_assignments(node, src, out);
            }
            "compound_statement" | "program" => rewritten_values(node, src, out),
            "cast_expression" => removed_cast(node, src, out),
            _ => {}
        }
        if matches!(
            node.kind(),
            "function_definition" | "method_declaration" | "anonymous_function"
        ) {
            inconsistent_return(node, src, out);
        }
    }
}

/// A non-empty `case` whose last statement does not leave, so execution
/// continues into the next case. A comment saying "fall through" or
/// "no break" marks it intended.
fn missing_break(block: Node, src: &str, out: &mut Findings) {
    let mut c = block.walk();
    let cases: Vec<Node> = block
        .named_children(&mut c)
        .filter(|n| matches!(n.kind(), "case_statement" | "default_statement"))
        .collect();
    for pair in cases.windows(2) {
        let (case, next) = (pair[0], pair[1]);
        let value = case.child_by_field_name("value").map(|v| v.id());
        let mut cc = case.walk();
        let last = case
            .named_children(&mut cc)
            .filter(|n| Some(n.id()) != value && n.kind() != "comment")
            .last();
        let Some(last) = last else { continue };
        let tail = src[last.end_byte()..next.start_byte()].to_ascii_lowercase();
        let intended = tail.contains("fall") || tail.contains("no break");
        if !intended && !ends_flow(last, src) {
            out.add(case.child(0).unwrap_or(case), Rule::MissingBreak, "");
        }
    }
}

/// A condition listed by an earlier `match` arm; its arm never runs.
fn duplicate_match_conditions(block: Node, src: &str, out: &mut Findings) {
    let mut seen = HashSet::new();
    for list in descendants(block).into_iter().filter(|n| {
        n.kind() == "match_condition_list" && n.parent().and_then(|p| p.parent()) == Some(block)
    }) {
        let mut c = list.walk();
        for condition in list.named_children(&mut c) {
            let key: String = text(condition, src).split_whitespace().collect();
            if !is_pure(condition) {
                continue;
            }
            if !seen.insert(key) {
                out.add(
                    condition,
                    Rule::DuplicateMatchCondition,
                    text(condition, src),
                );
            }
        }
    }
}

/// Whether a parameter's declared type already admits `null`.
fn type_allows_null(ty: Node, src: &str) -> bool {
    match ty.kind() {
        "optional_type" => true,
        "union_type" | "disjunctive_normal_form_type" => {
            let mut c = ty.walk();
            ty.named_children(&mut c)
                .any(|t| text(t, src).eq_ignore_ascii_case("null"))
        }
        _ => matches!(
            text(ty, src).to_ascii_lowercase().as_str(),
            "mixed" | "null"
        ),
    }
}

/// Optional parameters before required ones, and `Type $x = null`
/// without a nullable type.
fn parameter_defaults(params: Node, src: &str, out: &mut Findings) {
    let mut c = params.walk();
    let list: Vec<Node> = params
        .named_children(&mut c)
        .filter(|p| p.kind().ends_with("parameter"))
        .collect();
    let last_required = list.iter().rposition(|p| {
        p.kind() != "variadic_parameter" && p.child_by_field_name("default_value").is_none()
    });
    for (i, p) in list.iter().enumerate() {
        let Some(default) = p.child_by_field_name("default_value") else {
            continue;
        };
        let name = p
            .child_by_field_name("name")
            .map(|n| text(n, src))
            .unwrap_or("");
        let is_null = text(default, src).eq_ignore_ascii_case("null");
        let ty = p.child_by_field_name("type");
        if let Some(ty) = ty
            && is_null
            && !type_allows_null(ty, src)
        {
            out.add(ty, Rule::ImplicitlyNullableParameter, name);
        } else if last_required.is_some_and(|r| i < r) && !(is_null && ty.is_some()) {
            out.add(*p, Rule::OptionalBeforeRequired, name);
        }
    }
}

/// Whether `node` is the literal number zero.
fn is_zero(node: Node, src: &str) -> bool {
    let node = unparen(node);
    matches!(node.kind(), "integer" | "float")
        && text(node, src)
            .replace('_', "")
            .parse::<f64>()
            .is_ok_and(|v| v == 0.0)
}

/// `$a / 0` and `$a % 0`.
fn division_by_zero(binary: Node, src: &str, out: &mut Findings) {
    let op = binary.child_by_field_name("operator").map(|o| text(o, src));
    if matches!(op, Some("/" | "%"))
        && binary
            .child_by_field_name("right")
            .is_some_and(|r| is_zero(r, src))
    {
        out.add(binary, Rule::DivisionByZero, "");
    }
}

/// Variables bound by a `foreach` (key and value, destructured or by
/// reference), with their nodes.
fn loop_variables(foreach: Node) -> Vec<Node> {
    let mut c = foreach.walk();
    foreach
        .named_children(&mut c)
        .skip(1)
        .filter(|n| {
            !matches!(n.kind(), "compound_statement" | "colon_block")
                && !n.kind().ends_with("statement")
        })
        .flat_map(descendants)
        .filter(|n| n.kind() == "variable_name")
        .collect()
}

/// `foreach ($a as $a)` and nested loops reusing an outer loop variable.
fn foreach_variables(foreach: Node, src: &str, out: &mut Findings) {
    let vars = loop_variables(foreach);
    if let Some(subject) = foreach
        .named_child(0)
        .filter(|s| s.kind() == "variable_name")
    {
        let name = text(subject, src);
        if vars.iter().any(|v| text(*v, src) == name) {
            out.add(subject, Rule::ForeachOverwritesArray, name);
        }
    }
    let mut cur = foreach;
    while let Some(parent) = cur.parent() {
        if matches!(
            parent.kind(),
            "function_definition" | "method_declaration" | "anonymous_function" | "arrow_function"
        ) {
            break;
        }
        let in_body = parent.kind() == "foreach_statement"
            && parent
                .child_by_field_name("body")
                .is_some_and(|b| b.id() == cur.id());
        if in_body {
            let outer: HashSet<&str> = loop_variables(parent)
                .into_iter()
                .map(|v| text(v, src))
                .collect();
            if let Some(v) = vars.iter().find(|v| outer.contains(text(**v, src))) {
                out.add(*v, Rule::ForeachVariableConflict, text(*v, src));
                return;
            }
        }
        cur = parent;
    }
}

/// Rules about specific function calls.
fn calls(call: Node, src: &str, out: &mut Findings) {
    let Some(name) = call_name(call, src) else {
        return;
    };
    let Some(args) = call_args(call) else {
        return;
    };
    match name.as_str() {
        "var_export" | "print_r" if args.len() == 1 => {
            let used = call
                .parent()
                .is_some_and(|p| p.kind() != "expression_statement");
            if used {
                out.add(call, Rule::OutputFunctionResultUsed, name);
            }
        }
        "rand" | "mt_rand" | "random_int" if args.len() == 2 => {
            let num = |n: Node| text(unparen(n), src).replace('_', "").parse::<i64>().ok();
            if let (Some(a), Some(b)) = (num(args[0]), num(args[1]))
                && a > b
            {
                out.add(call, Rule::RandArgumentsReversed, "");
            }
        }
        "sprintf" | "printf" | "fprintf" => {
            let skip = usize::from(name == "fprintf");
            let Some(format) = args.get(skip).and_then(|f| literal_string(*f, src)) else {
                return;
            };
            if let Some(expected) = format_argument_count(&format)
                && expected != args.len() - skip - 1
            {
                out.add(call, Rule::FormatArgumentsMismatch, expected.to_string());
            }
        }
        _ if SEARCH_FUNCTIONS.contains(&name.as_str()) && used_as_boolean(call, src) => {
            out.add(call, Rule::SearchResultAsBoolean, name);
        }
        _ => {}
    }
}

/// The value of a string literal without interpolation, escapes resolved
/// only as far as `%` counting needs (they never contain `%`).
pub(crate) fn literal_string(node: Node, src: &str) -> Option<String> {
    let node = unparen(node);
    match node.kind() {
        "string" | "encapsed_string" => {
            let mut c = node.walk();
            let plain = node
                .named_children(&mut c)
                .all(|ch| matches!(ch.kind(), "string_content" | "escape_sequence"));
            let t = text(node, src);
            (plain && t.len() >= 2).then(|| t[1..t.len() - 1].to_string())
        }
        _ => None,
    }
}

/// How many arguments a `printf` format consumes; `None` when the format
/// has a directive this parser does not understand.
fn format_argument_count(format: &str) -> Option<usize> {
    let bytes = format.as_bytes();
    let (mut i, mut sequential, mut positional) = (0, 0usize, 0usize);
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        if bytes.get(i) == Some(&b'%') {
            i += 1;
            continue;
        }
        // Argument number: `%1$s`.
        let digits = bytes[i..].iter().take_while(|b| b.is_ascii_digit()).count();
        let mut argnum = None;
        if digits > 0 && bytes.get(i + digits) == Some(&b'$') {
            argnum = std::str::from_utf8(&bytes[i..i + digits])
                .ok()?
                .parse::<usize>()
                .ok();
            i += digits + 1;
        }
        // Flags.
        loop {
            match bytes.get(i) {
                Some(b'-' | b'+' | b' ' | b'0') => i += 1,
                Some(b'\'') => i += 2,
                _ => break,
            }
        }
        // Width and precision; `*` takes an argument.
        let mut stars = 0;
        for _ in 0..2 {
            if bytes.get(i) == Some(&b'.') {
                i += 1;
            }
            if bytes.get(i) == Some(&b'*') {
                stars += 1;
                i += 1;
            }
            i += bytes[i.min(bytes.len())..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
        }
        if !b"bcdeEfFgGhHosuxX".contains(bytes.get(i)?) {
            return None;
        }
        i += 1;
        match argnum {
            Some(n) => positional = positional.max(n),
            None => sequential += 1 + stars,
        }
    }
    Some(sequential.max(positional))
}

/// Whether the value of `call` is only tested for truth: an `if`/`while`
/// condition, a `!` operand, a `&&`/`||` operand or a ternary condition.
fn used_as_boolean(call: Node, src: &str) -> bool {
    let mut node = call;
    while let Some(p) = node
        .parent()
        .filter(|p| p.kind() == "parenthesized_expression")
    {
        node = p;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        "if_statement" | "while_statement" | "else_if_clause" | "do_statement" => true,
        "unary_op_expression" => text(parent, src).starts_with('!'),
        "binary_expression" => parent.child_by_field_name("operator").is_some_and(|o| {
            matches!(
                text(o, src).to_ascii_lowercase().as_str(),
                "&&" | "||" | "and" | "or"
            )
        }),
        "conditional_expression" => parent
            .child_by_field_name("condition")
            .is_some_and(|c| c.id() == node.id()),
        _ => false,
    }
}

/// A statement that computes a value and discards it (`$a == 1;`).
/// Property reads are left alone: `__get` may load data on access.
fn expression_unused(statement: Node, src: &str, out: &mut Findings) {
    let Some(expr) = statement.named_child(0) else {
        return;
    };
    let reportable = matches!(
        unparen(expr).kind(),
        "binary_expression"
            | "variable_name"
            | "class_constant_access_expression"
            | "string"
            | "encapsed_string"
            | "integer"
            | "float"
            | "boolean"
            | "null"
            | "array_creation_expression"
            | "unary_op_expression"
            | "cast_expression"
            | "conditional_expression"
            | "subscript_expression"
    );
    // A lone string at the top of a template is output, not an expression.
    let top_level_string = statement.parent().is_some_and(|p| p.kind() == "program")
        && matches!(expr.kind(), "string" | "encapsed_string");
    if reportable && !top_level_string && is_pure(expr) && !text(expr, src).is_empty() {
        out.add(expr, Rule::ExpressionResultUnused, "");
    }
}

/// `continue` (or `continue 1`) whose nearest breakable construct is a
/// `switch`: it acts like `break`.
fn continue_in_switch(statement: Node, src: &str, out: &mut Findings) {
    if statement
        .named_child(0)
        .is_some_and(|n| text(n, src).trim() != "1")
    {
        return;
    }
    let mut cur = statement.parent();
    while let Some(n) = cur {
        match n.kind() {
            "switch_statement" => {
                out.add(statement, Rule::ContinueInSwitch, "");
                return;
            }
            "for_statement"
            | "foreach_statement"
            | "while_statement"
            | "do_statement"
            | "function_definition"
            | "method_declaration"
            | "anonymous_function"
            | "arrow_function" => return,
            _ => cur = n.parent(),
        }
    }
}

/// `catch (E $e) { throw $e; }` as the only catch of its `try`.
fn rethrown(clause: Node, src: &str, out: &mut Findings) {
    let (Some(body), Some(name)) = (
        clause.child_by_field_name("body"),
        clause.child_by_field_name("name"),
    ) else {
        return;
    };
    let only_catch = clause.parent().is_some_and(|t| {
        let mut c = t.walk();
        t.named_children(&mut c)
            .filter(|n| n.kind() == "catch_clause")
            .count()
            == 1
    });
    let mut c = body.walk();
    let statements: Vec<Node> = body
        .named_children(&mut c)
        .filter(|n| n.kind() != "comment")
        .collect();
    let rethrows = statements.len() == 1
        && statements[0].kind() == "expression_statement"
        && statements[0].named_child(0).is_some_and(|t| {
            t.kind() == "throw_expression"
                && t.named_child(0)
                    .is_some_and(|v| text(v, src) == text(name, src))
        });
    let commented = body.named_child_count() != statements.len();
    if only_catch && rethrows && !commented {
        out.add(clause, Rule::ExceptionImmediatelyRethrown, "");
    }
}

/// `$a ? $b : $c ? $d : $e` without parentheses. Chains of the short
/// form (`$a ?: $b ?: $c`) stay valid.
fn nested_ternary(expr: Node, out: &mut Findings) {
    let Some(inner) = expr
        .child_by_field_name("condition")
        .filter(|c| c.kind() == "conditional_expression")
    else {
        return;
    };
    let short = |n: Node| n.child_by_field_name("body").is_none();
    if !(short(expr) && short(inner)) {
        out.add(expr, Rule::NestedTernary, "");
    }
}

/// `"${name}"` inside a string.
fn dollar_brace(var: Node, src: &str, out: &mut Findings) {
    let inside_string = var
        .parent()
        .is_some_and(|p| matches!(p.kind(), "encapsed_string" | "heredoc_body"));
    if inside_string && text(var, src).starts_with("${") {
        out.add(var, Rule::DollarBraceInterpolation, "");
    }
}

/// Magic methods with the wrong visibility or `static` modifier.
fn magic_modifiers(method: Node, src: &str, out: &mut Findings) {
    let Some(name) = method.child_by_field_name("name") else {
        return;
    };
    let lower = text(name, src).to_ascii_lowercase();
    let mut c = method.walk();
    let children: Vec<Node> = method.children(&mut c).collect();
    if PUBLIC_MAGIC.contains(&lower.as_str())
        && let Some(v) = children
            .iter()
            .find(|n| n.kind() == "visibility_modifier" && text(**n, src) != "public")
    {
        out.add(*v, Rule::MagicMethodNotPublic, text(name, src));
    }
    let is_static = children.iter().any(|n| n.kind() == "static_modifier");
    let wrong = if STATIC_MAGIC.contains(&lower.as_str()) {
        !is_static
    } else {
        INSTANCE_MAGIC.contains(&lower.as_str()) && is_static
    };
    if wrong {
        out.add(name, Rule::MagicMethodStatic, text(name, src));
    }
}

/// `$this->x = $x;` in a constructor that already promotes `$x`.
fn redundant_promoted_assignments(method: Node, src: &str, out: &mut Findings) {
    let is_constructor = method
        .child_by_field_name("name")
        .is_some_and(|n| text(n, src).eq_ignore_ascii_case("__construct"));
    let (Some(params), Some(body)) = (
        method.child_by_field_name("parameters"),
        method.child_by_field_name("body"),
    ) else {
        return;
    };
    if !is_constructor {
        return;
    }
    let mut c = params.walk();
    let promoted: HashSet<&str> = params
        .named_children(&mut c)
        .filter(|p| p.kind() == "property_promotion_parameter")
        .filter_map(|p| p.child_by_field_name("name"))
        .map(|n| text(n, src).trim_start_matches('$'))
        .collect();
    let mut bc = body.walk();
    for statement in body.named_children(&mut bc) {
        let Some(assignment) = statement.named_child(0).filter(|a| {
            statement.kind() == "expression_statement" && a.kind() == "assignment_expression"
        }) else {
            continue;
        };
        let left = assignment.child_by_field_name("left").map(|l| text(l, src));
        let right = assignment
            .child_by_field_name("right")
            .map(|r| text(r, src));
        if let (Some(left), Some(right)) = (left, right)
            && let Some(prop) = left.strip_prefix("$this->")
            && promoted.contains(prop)
            && right == format!("${prop}")
        {
            out.add(statement, Rule::RedundantPromotedAssignment, prop);
        }
    }
}

/// The plain assignment an expression statement makes, if any.
fn plain_assignment(statement: Node) -> Option<Node> {
    if statement.kind() != "expression_statement" {
        return None;
    }
    statement
        .named_child(0)
        .filter(|a| a.kind() == "assignment_expression")
}

/// `$a[k] = 1; $a[k] = 2;`: the first value is never read.
fn rewritten_values(block: Node, src: &str, out: &mut Findings) {
    let mut c = block.walk();
    let statements: Vec<Node> = block
        .named_children(&mut c)
        .filter(|n| n.kind() != "comment")
        .collect();
    for pair in statements.windows(2) {
        let (Some(first), Some(second)) = (plain_assignment(pair[0]), plain_assignment(pair[1]))
        else {
            continue;
        };
        let (Some(left1), Some(left2), Some(right2)) = (
            first.child_by_field_name("left"),
            second.child_by_field_name("left"),
            second.child_by_field_name("right"),
        ) else {
            continue;
        };
        let target = text(left1, src);
        let comparable = matches!(
            left1.kind(),
            "variable_name" | "subscript_expression" | "member_access_expression"
        ) && is_pure(left1)
            && !target.ends_with("[]");
        let reads_target = descendants(right2)
            .iter()
            .chain(descendants(left2).iter().skip(1))
            .any(|n| text(*n, src) == target);
        // A by-reference or array-append target, or `$a = $a + 1` style
        // reads, keep the first write meaningful.
        if comparable
            && target == text(left2, src)
            && !reads_target
            && !right2.kind().contains("reference")
        {
            out.add(first, Rule::ValueImmediatelyRewritten, target);
        }
    }
}

/// `(unset)` and `(real)` casts, removed in PHP 8.
fn removed_cast(cast: Node, src: &str, out: &mut Findings) {
    let Some(ty) = cast.child_by_field_name("type") else {
        return;
    };
    let name = text(ty, src).trim().to_ascii_lowercase();
    if matches!(name.as_str(), "unset" | "real") {
        out.add(cast, Rule::RemovedCast, format!("({name})"));
    }
}

/// A function mixing `return;` with `return value;`.
fn inconsistent_return(function: Node, src: &str, out: &mut Findings) {
    let Some(body) = function.child_by_field_name("body") else {
        return;
    };
    let mut bare = Vec::new();
    let mut with_value = false;
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "function_definition"
            | "anonymous_function"
            | "arrow_function"
            | "class_declaration"
            | "anonymous_class" => continue,
            "yield_expression" => return,
            "return_statement" => {
                let mut c = n.walk();
                if n.named_children(&mut c).any(|ch| ch.kind() != "comment") {
                    with_value = true;
                } else {
                    bare.push(n);
                }
            }
            _ => {}
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    let void = function
        .child_by_field_name("return_type")
        .is_some_and(|t| text(t, src).eq_ignore_ascii_case("void"));
    if with_value
        && !void
        && let Some(first) = bare.into_iter().min_by_key(|n| n.start_byte())
    {
        out.add(first, Rule::InconsistentReturn, "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    fn rules(src: &str) -> Vec<Rule> {
        let tree = parse(src);
        let mut out = Findings::default();
        bug_checks(tree.root_node(), src, &mut out);
        out.list.iter().map(|f| f.rule).collect()
    }

    #[test]
    fn missing_break_should_flag_fall_through_case() {
        assert_eq!(
            rules("<?php switch ($x) { case 1: f(); case 2: g(); break; default: h(); }"),
            [Rule::MissingBreak]
        );
    }

    #[test]
    fn missing_break_should_accept_grouped_and_commented_cases() {
        assert_eq!(
            rules(
                "<?php switch ($x) { case 1: case 2: g(); // fall through\n case 3: return 1; default: h(); }"
            ),
            []
        );
    }

    #[test]
    fn missing_break_should_accept_abort() {
        assert_eq!(
            rules("<?php switch ($x) { case 1: abort(403); case 2: return 1; default: h(); }"),
            []
        );
    }

    #[test]
    fn missing_break_should_accept_if_else_that_both_leave() {
        assert_eq!(
            rules(
                "<?php switch ($x) { case 1: if ($a) { return 1; } else { throw new E(); } default: h(); }"
            ),
            []
        );
    }

    #[test]
    fn duplicate_match_condition_should_be_flagged() {
        assert_eq!(
            rules("<?php $m = match ($x) { 1, 2 => 'a', 2 => 'b', default => 'c' };"),
            [Rule::DuplicateMatchCondition]
        );
    }

    #[test]
    fn optional_before_required_should_be_flagged() {
        assert_eq!(
            rules("<?php function f($a = 1, $b) {}"),
            [Rule::OptionalBeforeRequired]
        );
    }

    #[test]
    fn implicitly_nullable_parameter_should_be_flagged() {
        assert_eq!(
            rules(
                "<?php function f(Foo $a = null, ?Foo $b = null, Foo|null $c = null, mixed $d = null) {}"
            ),
            [Rule::ImplicitlyNullableParameter]
        );
    }

    #[test]
    fn division_by_zero_should_be_flagged() {
        assert_eq!(rules("<?php $a = $b / 0;"), [Rule::DivisionByZero]);
    }

    #[test]
    fn foreach_overwriting_its_array_should_be_flagged() {
        assert_eq!(
            rules("<?php foreach ($items as $items) {}"),
            [Rule::ForeachOverwritesArray]
        );
    }

    #[test]
    fn nested_foreach_reusing_a_variable_should_be_flagged() {
        assert_eq!(
            rules("<?php foreach ($a as $k => $v) { foreach ($b as $v) {} }"),
            [Rule::ForeachVariableConflict]
        );
    }

    #[test]
    fn print_r_result_used_should_be_flagged() {
        assert_eq!(
            rules("<?php $s = print_r($a); print_r($a); $t = print_r($a, true);"),
            [Rule::OutputFunctionResultUsed]
        );
    }

    #[test]
    fn rand_with_reversed_bounds_should_be_flagged() {
        assert_eq!(
            rules("<?php $r = rand(10, 1);"),
            [Rule::RandArgumentsReversed]
        );
    }

    #[test]
    fn sprintf_with_missing_argument_should_be_flagged() {
        assert_eq!(
            rules("<?php $s = sprintf('%s and %d', $a); $t = sprintf('%1$s %1$s %%', $a);"),
            [Rule::FormatArgumentsMismatch]
        );
    }

    #[test]
    fn strpos_as_condition_should_be_flagged() {
        assert_eq!(
            rules("<?php if (strpos($a, 'x')) {} if (strpos($a, 'x') !== false) {}"),
            [Rule::SearchResultAsBoolean]
        );
    }

    #[test]
    fn comparison_statement_should_be_flagged_as_unused() {
        assert_eq!(
            rules("<?php $a == 1; f(); $b = 2; $c++;"),
            [Rule::ExpressionResultUnused]
        );
    }

    #[test]
    fn continue_in_switch_should_be_flagged() {
        assert_eq!(
            rules("<?php foreach ($a as $x) { switch ($x) { case 1: continue; } }"),
            [Rule::ContinueInSwitch]
        );
    }

    #[test]
    fn continue_in_loop_inside_switch_should_be_accepted() {
        assert_eq!(
            rules("<?php switch ($x) { case 1: foreach ($a as $b) { continue; } break; }"),
            []
        );
    }

    #[test]
    fn immediately_rethrown_exception_should_be_flagged() {
        assert_eq!(
            rules("<?php try { f(); } catch (E $e) { throw $e; }"),
            [Rule::ExceptionImmediatelyRethrown]
        );
    }

    #[test]
    fn nested_ternary_should_be_flagged() {
        assert_eq!(
            rules("<?php $x = $a ? 1 : $b ? 2 : 3;"),
            [Rule::NestedTernary]
        );
    }

    #[test]
    fn chained_short_ternary_should_be_accepted() {
        assert_eq!(rules("<?php $x = $a ?: $b ?: $c;"), []);
    }

    #[test]
    fn dollar_brace_interpolation_should_be_flagged() {
        assert_eq!(
            rules("<?php $s = \"a ${b} {$c}\";"),
            [Rule::DollarBraceInterpolation]
        );
    }

    #[test]
    fn private_magic_getter_should_be_flagged() {
        assert_eq!(
            rules("<?php class A { private function __get($n) {} }"),
            [Rule::MagicMethodNotPublic]
        );
    }

    #[test]
    fn static_constructor_should_be_flagged() {
        assert_eq!(
            rules("<?php class A { public static function __construct() {} }"),
            [Rule::MagicMethodStatic]
        );
    }

    #[test]
    fn redundant_promoted_assignment_should_be_flagged() {
        assert_eq!(
            rules(
                "<?php class A { public function __construct(private int $x) { $this->x = $x; } }"
            ),
            [Rule::RedundantPromotedAssignment]
        );
    }

    #[test]
    fn immediately_rewritten_key_should_be_flagged() {
        assert_eq!(
            rules("<?php $a['k'] = 1; $a['k'] = 2; $b = 1; $b = $b + 1;"),
            [Rule::ValueImmediatelyRewritten]
        );
    }

    #[test]
    fn unset_cast_should_be_flagged() {
        assert_eq!(rules("<?php $a = (unset) $b;"), [Rule::RemovedCast]);
    }

    #[test]
    fn mixed_returns_should_be_flagged() {
        assert_eq!(
            rules("<?php function f($a) { if ($a) { return; } return 1; }"),
            [Rule::InconsistentReturn]
        );
    }
}
