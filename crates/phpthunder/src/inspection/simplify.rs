//! Simplifications: code that works but says more than it needs to, each
//! with a quick fix writing the shorter form (`?:`, `??`, `?->`, `**`,
//! `[]`, arrow functions, `str_contains`, …), plus unused parameters and
//! stray semicolons.

use super::*;
use crate::syntax::text;
use std::collections::HashSet;
use tree_sitter::Node;

/// Calls that read a function's variables by name, so a parameter that
/// looks unused may be read through them.
const VARIABLE_READERS: &[&str] = &[
    "func_get_args",
    "func_get_arg",
    "compact",
    "get_defined_vars",
    "extract",
];

/// Runs every simplification rule over the tree rooted at `root`.
///
/// Called by [`diagnostics`] on each inspection pass.
pub(crate) fn simplify_checks(root: Node, src: &str, out: &mut Findings) {
    for node in descendants(root) {
        if node.is_error() {
            continue;
        }
        match node.kind() {
            "conditional_expression" => ternaries(node, src, out),
            "binary_expression" => binaries(node, src, out),
            "function_call_expression" => calls(node, src, out),
            "array_creation_expression" => {
                if text(node, src)
                    .get(..5)
                    .is_some_and(|s| s.eq_ignore_ascii_case("array"))
                {
                    out.add(node, Rule::LongArraySyntax, "");
                }
            }
            "program" => closing_tag(node, src, out),
            "anonymous_function" => closure_to_arrow(node, src, out),
            "compound_statement" => local_returned(node, src, out),
            "error_suppression_expression" => out.add(node, Rule::SilenceOperator, ""),
            "cast_expression" => bool_cast(node, src, out),
            "break_statement" | "continue_statement" => {
                if let Some(level) = node.named_child(0).filter(|n| text(*n, src).trim() == "1") {
                    out.add(level, Rule::RedundantBreakArgument, "");
                }
            }
            "assignment_expression" => append_using_count(node, src, out),
            "match_block" => {
                let mut c = node.walk();
                let arms: Vec<Node> = node
                    .named_children(&mut c)
                    .filter(|n| n.kind() != "comment")
                    .collect();
                if arms.len() == 1 && arms[0].kind() == "match_default_expression" {
                    out.add(node.parent().unwrap_or(node), Rule::MatchOnlyDefault, "");
                }
            }
            "empty_statement" => {
                let stray = node.parent().is_some_and(|p| {
                    matches!(
                        p.kind(),
                        "compound_statement" | "program" | "declaration_list"
                    )
                });
                if stray {
                    out.add(node, Rule::UnnecessarySemicolon, "");
                }
            }
            "function_definition" | "method_declaration" => {
                unused_parameters(node, src, out);
            }
            _ => {}
        }
    }
}

/// The comparison `$x !== null` / `null !== $x` / `!is_null($x)`: the
/// checked expression's text.
fn not_null_check<'a>(cond: Node, src: &'a str) -> Option<&'a str> {
    let cond = unparen(cond);
    match cond.kind() {
        "binary_expression" => {
            let op = text(cond.child_by_field_name("operator")?, src);
            if !matches!(op, "!==" | "!=") {
                return None;
            }
            let (l, r) = (
                cond.child_by_field_name("left")?,
                cond.child_by_field_name("right")?,
            );
            let is_null = |n: Node| text(n, src).eq_ignore_ascii_case("null");
            if is_null(r) {
                Some(text(l, src))
            } else if is_null(l) {
                Some(text(r, src))
            } else {
                None
            }
        }
        "unary_op_expression" => {
            let inner = unparen(cond.child_by_field_name("argument")?);
            (call_name(inner, src).as_deref() == Some("is_null"))
                .then(|| call_args(inner))
                .flatten()
                .filter(|a| a.len() == 1)
                .map(|a| text(a[0], src))
        }
        _ => None,
    }
}

/// The single argument of an `isset(…)` call.
fn isset_argument(node: Node, src: &str) -> Option<String> {
    let node = unparen(node);
    if call_name(node, src).as_deref() != Some("isset") {
        // `isset` is parsed as its own construct in some grammar versions.
        if node.kind() != "isset_expression" {
            return None;
        }
    }
    let args = call_args(node).or_else(|| {
        let mut c = node.walk();
        Some(node.named_children(&mut c).collect())
    })?;
    (args.len() == 1).then(|| text(args[0], src).to_string())
}

/// Ternary rewrites: `?:`, plain condition, `??`, `?->`.
fn ternaries(expr: Node, src: &str, out: &mut Findings) {
    let (Some(cond), Some(body), Some(alt)) = (
        expr.child_by_field_name("condition"),
        expr.child_by_field_name("body"),
        expr.child_by_field_name("alternative"),
    ) else {
        return;
    };
    let lit = |n: Node| text(n, src).to_ascii_lowercase();
    if text(cond, src) == text(body, src) && is_pure(cond) {
        out.add(expr, Rule::TernaryToShort, "");
    } else if is_boolean(cond, src)
        && matches!(
            (lit(body).as_str(), lit(alt).as_str()),
            ("true", "false") | ("false", "true")
        )
    {
        out.add(expr, Rule::TernaryToCondition, "");
    } else if isset_argument(cond, src).is_some_and(|a| a == text(body, src)) {
        out.add(expr, Rule::IssetToCoalesce, "");
    } else if let Some(subject) = not_null_check(cond, src)
        && lit(alt) == "null"
        && matches!(
            body.kind(),
            "member_access_expression" | "member_call_expression"
        )
        && body
            .child_by_field_name("object")
            .is_some_and(|o| text(o, src) == subject)
    {
        out.add(expr, Rule::NullsafeCanBeUsed, "");
    }
}

/// Rules on binary operators: combined `isset`, boolean comparisons,
/// `strpos(…) !== false`.
fn binaries(binary: Node, src: &str, out: &mut Findings) {
    let Some(op) = binary.child_by_field_name("operator").map(|o| text(o, src)) else {
        return;
    };
    let (Some(left), Some(right)) = (
        binary.child_by_field_name("left"),
        binary.child_by_field_name("right"),
    ) else {
        return;
    };
    if op == "&&" && isset_argument(left, src).is_some() && isset_argument(right, src).is_some() {
        out.add(binary, Rule::CombineIsset, "");
    }
    let is_bool = |n: Node| matches!(text(n, src).to_ascii_lowercase().as_str(), "true" | "false");
    if matches!(op, "==" | "!=")
        && (is_bool(left) || is_bool(right))
        && !(is_bool(left) && is_bool(right))
    {
        out.add(binary, Rule::BooleanComparison, "");
    }
    if let Some(replacement) = str_function(binary, src) {
        out.add(binary, Rule::StrFunctionCanBeUsed, replacement);
    }
}

/// For `strpos($h, $n) !== false`, `=== false` and `=== 0`: the PHP 8
/// function saying it (`str_contains`, `str_starts_with`).
fn str_function(binary: Node, src: &str) -> Option<&'static str> {
    let op = text(binary.child_by_field_name("operator")?, src);
    let (left, right) = (
        unparen(binary.child_by_field_name("left")?),
        unparen(binary.child_by_field_name("right")?),
    );
    let (call, other) = if left.kind() == "function_call_expression" {
        (left, right)
    } else {
        (right, left)
    };
    if call_name(call, src).as_deref() != Some("strpos") || call_args(call)?.len() != 2 {
        return None;
    }
    match (op, text(other, src).to_ascii_lowercase().as_str()) {
        ("!==", "false") => Some("str_contains"),
        ("===", "false") => Some("str_contains"),
        ("===", "0") => Some("str_starts_with"),
        _ => None,
    }
}

/// Rules on specific calls.
fn calls(call: Node, src: &str, out: &mut Findings) {
    let (Some(name), Some(args)) = (call_name(call, src), call_args(call)) else {
        return;
    };
    match (name.as_str(), args.len()) {
        ("array_push", 2)
            if call
                .parent()
                .is_some_and(|p| p.kind() == "expression_statement") =>
        {
            out.add(call, Rule::ArrayPushSingle, "");
        }
        ("pow", 2) => out.add(call, Rule::PowToOperator, ""),
        ("get_class", 1) if args[0].kind() == "variable_name" && text(args[0], src) != "$this" => {
            out.add(call, Rule::GetClassToClassConstant, "");
        }
        ("in_array", 2 | 3) => {
            let haystack = unparen(args[1]);
            let mut c = haystack.walk();
            let elements: Vec<Node> = haystack
                .named_children(&mut c)
                .filter(|n| n.kind() != "comment")
                .collect();
            let single = haystack.kind() == "array_creation_expression"
                && elements.len() == 1
                && elements[0].named_child_count() == 1
                && !text(elements[0], src).starts_with("...");
            if single {
                out.add(call, Rule::InArraySingleElement, "");
            }
        }
        ("dirname", 1) => {
            let inner = unparen(args[0]);
            if call_name(inner, src).as_deref() == Some("dirname")
                && call_args(inner).is_some_and(|a| a.len() == 1)
                && call
                    .parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.parent())
                    .is_none_or(|p| call_name(p, src).as_deref() != Some("dirname"))
            {
                out.add(call, Rule::NestedDirname, "");
            }
        }
        ("max" | "min", n) if n >= 2 => {
            let nested = args.iter().any(|a| {
                call_name(unparen(*a), src).as_deref() == Some(name.as_str())
                    && call_args(unparen(*a)).is_some_and(|inner| inner.len() >= 2)
            });
            if nested {
                out.add(call, Rule::NestedMinMax, name.clone());
            }
        }
        _ => {}
    }
}

/// A `?>` ending a file that has no inline HTML after its opening tag.
fn closing_tag(program: Node, src: &str, out: &mut Findings) {
    let mut c = program.walk();
    let children: Vec<Node> = program.named_children(&mut c).collect();
    let Some(last) = children.last().filter(|n| n.kind() == "text_interpolation") else {
        return;
    };
    let html_elsewhere = children
        .iter()
        .filter(|n| n.kind() == "text_interpolation" || n.kind() == "text")
        .count()
        > 1;
    if !html_elsewhere
        && text(*last, src).trim() == "?>"
        && !src[..last.start_byte()].contains("?>")
    {
        out.add(*last, Rule::RedundantClosingTag, "");
    }
}

/// `function ($x) use ($y) { return $x + $y; }`: only returns an
/// expression and imports by value, so `fn ($x) => $x + $y` means the same.
fn closure_to_arrow(closure: Node, src: &str, out: &mut Findings) {
    let Some(body) = closure.child_by_field_name("body") else {
        return;
    };
    let mut c = body.walk();
    let statements: Vec<Node> = body.named_children(&mut c).collect();
    let single_return = statements.len() == 1
        && statements[0].kind() == "return_statement"
        && statements[0].named_child_count() == 1;
    let mut cc = closure.walk();
    let by_ref = closure
        .named_children(&mut cc)
        .any(|n| n.kind() == "anonymous_function_use_clause" && text(n, src).contains('&'));
    let multiline = statements
        .first()
        .and_then(|s| s.named_child(0))
        .is_some_and(|e| text(e, src).contains('\n'));
    if single_return && !by_ref && !multiline {
        out.add(closure, Rule::ClosureToArrowFunction, "");
    }
}

/// `$x = expr; return $x;` where `$x` is used nowhere else.
fn local_returned(block: Node, src: &str, out: &mut Findings) {
    let mut c = block.walk();
    let statements: Vec<Node> = block.named_children(&mut c).collect();
    let Some(function) = enclosing_function(block) else {
        return;
    };
    for pair in statements.windows(2) {
        let (assign_stmt, ret) = (pair[0], pair[1]);
        let Some(assignment) = assign_stmt.named_child(0).filter(|a| {
            assign_stmt.kind() == "expression_statement" && a.kind() == "assignment_expression"
        }) else {
            continue;
        };
        let Some(var) = assignment
            .child_by_field_name("left")
            .filter(|l| l.kind() == "variable_name")
        else {
            continue;
        };
        let name = text(var, src);
        let returns_it = ret.kind() == "return_statement"
            && ret.named_child_count() == 1
            && ret.named_child(0).is_some_and(|v| text(v, src) == name);
        if !returns_it || name == "$this" {
            continue;
        }
        let uses = descendants(function)
            .into_iter()
            .filter(|n| n.kind() == "variable_name" && text(*n, src) == name)
            .count();
        if uses == 2 {
            out.add(var, Rule::UnnecessaryLocalVariable, name);
        }
    }
}

/// The function-like node a node is in.
fn enclosing_function(node: Node) -> Option<Node> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if matches!(
            n.kind(),
            "function_definition" | "method_declaration" | "anonymous_function" | "arrow_function"
        ) {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

/// `(bool)` applied to a value that is already boolean.
fn bool_cast(cast: Node, src: &str, out: &mut Findings) {
    let to_bool = cast.child_by_field_name("type").is_some_and(|t| {
        matches!(
            text(t, src).trim().to_ascii_lowercase().as_str(),
            "bool" | "boolean"
        )
    });
    if to_bool
        && cast
            .child_by_field_name("value")
            .is_some_and(|v| is_boolean(v, src))
    {
        out.add(cast, Rule::UnnecessaryBoolCast, "");
    }
}

/// `$a[count($a)] = …`.
fn append_using_count(assignment: Node, src: &str, out: &mut Findings) {
    let Some(left) = assignment
        .child_by_field_name("left")
        .filter(|l| l.kind() == "subscript_expression")
    else {
        return;
    };
    let (Some(array), Some(index)) = (left.named_child(0), left.named_child(1)) else {
        return;
    };
    let counts_itself = call_name(index, src).as_deref() == Some("count")
        && call_args(index).is_some_and(|a| a.len() == 1 && text(a[0], src) == text(array, src));
    if counts_itself {
        out.add(left, Rule::ArrayAppendUsingCount, "");
    }
}

/// Parameters a function or private method never reads. Public and
/// protected methods are skipped (their signature is a contract), and so
/// are closures (callers fix their parameter lists).
fn unused_parameters(function: Node, src: &str, out: &mut Findings) {
    let (Some(params), Some(body)) = (
        function.child_by_field_name("parameters"),
        function.child_by_field_name("body"),
    ) else {
        return;
    };
    if function.kind() == "method_declaration" {
        let mut c = function.walk();
        let private = function
            .children(&mut c)
            .any(|n| n.kind() == "visibility_modifier" && text(n, src) == "private");
        if !private {
            return;
        }
    }
    let nodes = descendants(body);
    let dynamic = nodes.iter().any(|n| {
        n.kind() == "dynamic_variable_name"
            || call_name(*n, src).is_some_and(|c| VARIABLE_READERS.contains(&c.as_str()))
    });
    if dynamic {
        return;
    }
    let used: HashSet<&str> = nodes
        .iter()
        .filter(|n| n.kind() == "variable_name")
        .map(|n| text(*n, src))
        .collect();
    let mut c = params.walk();
    let list: Vec<(Node, &str)> = params
        .named_children(&mut c)
        .filter(|p| matches!(p.kind(), "simple_parameter" | "variadic_parameter"))
        .filter_map(|p| p.child_by_field_name("name").map(|n| (n, text(n, src))))
        .collect();
    for (node, name) in &list {
        if !used.contains(name) && !name.starts_with("$_") {
            out.add(*node, Rule::UnusedParameter, *name);
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
        simplify_checks(tree.root_node(), src, &mut out);
        out.list.iter().map(|f| f.rule).collect()
    }

    #[test]
    fn ternary_repeating_condition_should_be_shortened() {
        assert_eq!(rules("<?php $a = $b ? $b : $c;"), [Rule::TernaryToShort]);
    }

    #[test]
    fn ternary_returning_booleans_should_be_its_condition() {
        assert_eq!(
            rules("<?php $a = $b > 1 ? true : false;"),
            [Rule::TernaryToCondition]
        );
    }

    #[test]
    fn isset_ternary_should_be_coalesce() {
        assert_eq!(
            rules("<?php $a = isset($b['k']) ? $b['k'] : 0;"),
            [Rule::IssetToCoalesce]
        );
    }

    #[test]
    fn null_check_ternary_should_be_nullsafe() {
        assert_eq!(
            rules("<?php $a = $u !== null ? $u->name : null;"),
            [Rule::NullsafeCanBeUsed]
        );
    }

    #[test]
    fn isset_calls_should_combine() {
        assert_eq!(
            rules("<?php $a = isset($b) && isset($c);"),
            [Rule::CombineIsset]
        );
    }

    #[test]
    fn comparison_with_true_should_simplify() {
        assert_eq!(rules("<?php $a = $b == true;"), [Rule::BooleanComparison]);
    }

    #[test]
    fn strpos_not_false_should_be_str_contains() {
        assert_eq!(
            rules("<?php $a = strpos($h, 'x') !== false;"),
            [Rule::StrFunctionCanBeUsed]
        );
    }

    #[test]
    fn single_array_push_should_be_append() {
        assert_eq!(rules("<?php array_push($a, 1);"), [Rule::ArrayPushSingle]);
    }

    #[test]
    fn pow_should_be_operator() {
        assert_eq!(rules("<?php $a = pow(2, 3);"), [Rule::PowToOperator]);
    }

    #[test]
    fn long_array_should_be_short() {
        assert_eq!(rules("<?php $a = array(1, 2);"), [Rule::LongArraySyntax]);
    }

    #[test]
    fn closing_tag_of_pure_php_file_should_be_redundant() {
        assert_eq!(rules("<?php\necho 1;\n?>\n"), [Rule::RedundantClosingTag]);
    }

    #[test]
    fn closing_tag_before_html_should_be_kept() {
        assert_eq!(rules("<?php echo 1; ?>\n<p>x</p>\n"), []);
    }

    #[test]
    fn single_return_closure_should_be_arrow_function() {
        assert_eq!(
            rules("<?php $f = function ($x) use ($y) { return $x + $y; };"),
            [Rule::ClosureToArrowFunction]
        );
    }

    #[test]
    fn closure_with_reference_use_should_stay() {
        assert_eq!(
            rules("<?php $f = function ($x) use (&$y) { return $x + $y; };"),
            []
        );
    }

    #[test]
    fn get_class_should_be_class_constant() {
        assert_eq!(
            rules("<?php $a = get_class($o);"),
            [Rule::GetClassToClassConstant]
        );
    }

    #[test]
    fn returned_local_should_be_inlined() {
        assert_eq!(
            rules("<?php function f() { $x = g(); return $x; }"),
            [Rule::UnnecessaryLocalVariable]
        );
    }

    #[test]
    fn silence_operator_should_be_flagged() {
        assert_eq!(rules("<?php $a = @f();"), [Rule::SilenceOperator]);
    }

    #[test]
    fn bool_cast_of_comparison_should_be_flagged() {
        assert_eq!(
            rules("<?php $a = (bool) ($b > 1);"),
            [Rule::UnnecessaryBoolCast]
        );
    }

    #[test]
    fn break_one_should_be_flagged() {
        assert_eq!(
            rules("<?php while (1) { break 1; }"),
            [Rule::RedundantBreakArgument]
        );
    }

    #[test]
    fn in_array_with_one_element_should_be_comparison() {
        assert_eq!(
            rules("<?php $a = in_array($x, ['a']);"),
            [Rule::InArraySingleElement]
        );
    }

    #[test]
    fn count_index_append_should_be_flagged() {
        assert_eq!(
            rules("<?php $a[count($a)] = 1;"),
            [Rule::ArrayAppendUsingCount]
        );
    }

    #[test]
    fn nested_dirname_should_be_flagged_once() {
        assert_eq!(
            rules("<?php $a = dirname(dirname(dirname($p)));"),
            [Rule::NestedDirname]
        );
    }

    #[test]
    fn nested_max_should_be_flagged() {
        assert_eq!(
            rules("<?php $a = max($b, max($c, $d));"),
            [Rule::NestedMinMax]
        );
    }

    #[test]
    fn match_with_only_default_should_be_flagged() {
        assert_eq!(
            rules("<?php $a = match ($b) { default => 1 };"),
            [Rule::MatchOnlyDefault]
        );
    }

    #[test]
    fn double_semicolon_should_be_flagged() {
        assert_eq!(rules("<?php f();;"), [Rule::UnnecessarySemicolon]);
    }

    #[test]
    fn unused_function_parameter_should_be_flagged() {
        assert_eq!(
            rules("<?php function f($a, $b) { return $a; }"),
            [Rule::UnusedParameter]
        );
    }

    #[test]
    fn closure_parameter_should_be_accepted() {
        assert_eq!(rules("<?php $f = function ($key) { f(); };"), []);
    }

    #[test]
    fn public_method_parameter_should_be_accepted() {
        assert_eq!(rules("<?php class A { public function f($a) {} }"), []);
    }
}
