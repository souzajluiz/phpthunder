//! Quick fixes of the rule-based inspections: each finds the reported node
//! again (by byte range) in a fresh parse and rewrites it.

use super::*;
use crate::syntax::{parse, text};
use tree_sitter::Node;

/// The node spanning exactly `start..end`, climbing from the smallest one
/// to the first ancestor of one of `kinds` with the same range. An empty
/// `kinds` accepts the smallest node.
fn node_at<'t>(root: Node<'t>, start: usize, end: usize, kinds: &[&str]) -> Option<Node<'t>> {
    let mut node = root.descendant_for_byte_range(start, end)?;
    loop {
        if node.start_byte() == start
            && node.end_byte() == end
            && (kinds.is_empty() || kinds.contains(&node.kind()))
        {
            return Some(node);
        }
        node = node.parent()?;
        if node.start_byte() < start || node.end_byte() > end {
            return None;
        }
    }
}

/// An edit replacing `node` with `text`.
fn replace(node: Node, text: impl Into<String>) -> Edit {
    Edit {
        path: None,
        start: node.start_byte(),
        end: node.end_byte(),
        text: text.into(),
    }
}

/// An edit inserting `text` at byte `at`.
fn insert(at: usize, text: impl Into<String>) -> Edit {
    Edit {
        path: None,
        start: at,
        end: at,
        text: text.into(),
    }
}

/// `node`'s source, in parentheses unless it binds tighter than any
/// operator it could end up next to.
fn operand(node: Node, src: &str) -> String {
    let simple = matches!(
        unparen(node).kind(),
        "variable_name"
            | "integer"
            | "float"
            | "string"
            | "encapsed_string"
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
            | "boolean"
            | "null"
    );
    if simple {
        text(node, src).to_string()
    } else {
        format!("({})", text(node, src))
    }
}

/// The edits fixing a rule finding at `start..end` of `src`, or `None`
/// when the rule has no fix or the code changed shape.
///
/// Called by [`quick_fixes`] for [`Code::Rule`] diagnostics.
pub(crate) fn rule_fix(
    rule: Rule,
    src: &str,
    start: usize,
    end: usize,
    detail: &str,
) -> Option<Vec<Edit>> {
    let tree = parse(src);
    let root = tree.root_node();
    let at = |kinds: &[&str]| node_at(root, start, end, kinds);
    let edits = match rule {
        Rule::OutputFunctionResultUsed => {
            let call = at(&["function_call_expression"])?;
            let args = call.child_by_field_name("arguments")?;
            vec![insert(args.end_byte() - 1, ", true")]
        }
        Rule::RandArgumentsReversed => {
            let call = at(&["function_call_expression"])?;
            let args = call_args(call)?;
            let (a, b) = (args.first()?, args.get(1)?);
            vec![Edit {
                path: None,
                start: a.start_byte(),
                end: b.end_byte(),
                text: format!(
                    "{}{}{}",
                    text(*b, src),
                    &src[a.end_byte()..b.start_byte()],
                    text(*a, src)
                ),
            }]
        }
        Rule::ContinueInSwitch => {
            let statement = at(&["continue_statement"])?;
            let semicolon = if text(statement, src).ends_with(';') {
                ";"
            } else {
                ""
            };
            vec![replace(statement, format!("continue 2{semicolon}"))]
        }
        Rule::DollarBraceInterpolation => {
            let var = at(&["dynamic_variable_name"])?;
            let inner = text(var, src).strip_prefix("${")?;
            if !inner.starts_with(|c: char| c.is_alphabetic() || c == '_') {
                return None;
            }
            vec![replace(var, format!("{{${inner}"))]
        }
        Rule::ImplicitlyNullableParameter => {
            let ty = at(&[])?;
            let written = text(ty, src);
            let nullable = match ty.kind() {
                "union_type" => format!("{written}|null"),
                "intersection_type" => format!("({written})|null"),
                _ => format!("?{written}"),
            };
            vec![replace(ty, nullable)]
        }
        Rule::MagicMethodNotPublic | Rule::VarKeyword => vec![replace(at(&[])?, "public")],
        Rule::RedundantPromotedAssignment => {
            vec![remove_lines_edit(src, start, end)]
        }
        Rule::ValueImmediatelyRewritten => {
            let assignment = at(&["assignment_expression"])?;
            let right = assignment.child_by_field_name("right")?;
            if is_pure(right) {
                let statement = assignment.parent()?;
                vec![remove_lines_edit(
                    src,
                    statement.start_byte(),
                    statement.end_byte(),
                )]
            } else {
                vec![Edit {
                    path: None,
                    start: assignment.start_byte(),
                    end: right.start_byte(),
                    text: String::new(),
                }]
            }
        }
        Rule::TernaryToShort => {
            let expr = at(&["conditional_expression"])?;
            let cond = expr.child_by_field_name("condition")?;
            let alt = expr.child_by_field_name("alternative")?;
            vec![replace(
                expr,
                format!("{} ?: {}", text(cond, src), text(alt, src)),
            )]
        }
        Rule::TernaryToCondition => {
            let expr = at(&["conditional_expression"])?;
            let cond = expr.child_by_field_name("condition")?;
            let body = expr.child_by_field_name("body")?;
            let text_cond = if text(body, src).eq_ignore_ascii_case("true") {
                text(cond, src).to_string()
            } else {
                format!("!{}", operand(cond, src))
            };
            vec![replace(expr, text_cond)]
        }
        Rule::IssetToCoalesce => {
            let expr = at(&["conditional_expression"])?;
            let body = expr.child_by_field_name("body")?;
            let alt = expr.child_by_field_name("alternative")?;
            vec![replace(
                expr,
                format!("{} ?? {}", text(body, src), text(alt, src)),
            )]
        }
        Rule::NullsafeCanBeUsed => {
            let expr = at(&["conditional_expression"])?;
            let body = expr.child_by_field_name("body")?;
            let object = body.child_by_field_name("object")?;
            let rest = &text(body, src)[object.end_byte() - body.start_byte()..];
            vec![replace(expr, format!("{}?{}", text(object, src), rest))]
        }
        Rule::CombineIsset => {
            let binary = at(&["binary_expression"])?;
            let mut parts = Vec::new();
            for side in ["left", "right"] {
                let call = unparen(binary.child_by_field_name(side)?);
                let args = call_args(call).or_else(|| {
                    let mut c = call.walk();
                    Some(call.named_children(&mut c).collect())
                })?;
                parts.extend(args.iter().map(|a| text(*a, src).to_string()));
            }
            vec![replace(binary, format!("isset({})", parts.join(", ")))]
        }
        Rule::BooleanComparison => {
            let binary = at(&["binary_expression"])?;
            let op = text(binary.child_by_field_name("operator")?, src);
            let (left, right) = (
                binary.child_by_field_name("left")?,
                binary.child_by_field_name("right")?,
            );
            let is_bool =
                |n: Node| matches!(text(n, src).to_ascii_lowercase().as_str(), "true" | "false");
            let (literal, other) = if is_bool(right) {
                (right, left)
            } else {
                (left, right)
            };
            let truthy = text(literal, src).eq_ignore_ascii_case("true") == (op == "==");
            let other_text = if truthy {
                text(other, src).to_string()
            } else {
                format!("!{}", operand(other, src))
            };
            vec![replace(binary, other_text)]
        }
        Rule::ArrayPushSingle => {
            let call = at(&["function_call_expression"])?;
            let args = call_args(call)?;
            vec![replace(
                call,
                format!("{}[] = {}", text(args[0], src), text(args[1], src)),
            )]
        }
        Rule::PowToOperator => {
            let call = at(&["function_call_expression"])?;
            let args = call_args(call)?;
            vec![replace(
                call,
                format!("{} ** {}", operand(args[0], src), operand(args[1], src)),
            )]
        }
        Rule::LongArraySyntax => {
            let array = at(&["array_creation_expression"])?;
            let written = text(array, src);
            let open = written.find('(')?;
            let close = written.rfind(')')?;
            vec![replace(array, format!("[{}]", &written[open + 1..close]))]
        }
        Rule::RedundantClosingTag => {
            let before = src[..start].trim_end_matches([' ', '\t']);
            let newline = if before.ends_with('\n') { "" } else { "\n" };
            vec![Edit {
                path: None,
                start: before.len(),
                end: src.len(),
                text: newline.into(),
            }]
        }
        Rule::ClosureToArrowFunction => {
            let closure = at(&["anonymous_function"])?;
            let params = closure.child_by_field_name("parameters")?;
            let body = closure.child_by_field_name("body")?;
            let expr = body.named_child(0)?.named_child(0)?;
            let head = &src[closure.start_byte()..params.start_byte()];
            let is_static = head.trim_start().starts_with("static");
            let by_ref = head.contains('&');
            let ret = closure
                .child_by_field_name("return_type")
                .map(|t| format!(": {}", text(t, src)))
                .unwrap_or_default();
            vec![replace(
                closure,
                format!(
                    "{}fn{}{}{} => {}",
                    if is_static { "static " } else { "" },
                    if by_ref { "&" } else { "" },
                    text(params, src),
                    ret,
                    text(expr, src)
                ),
            )]
        }
        Rule::GetClassToClassConstant => {
            let call = at(&["function_call_expression"])?;
            let args = call_args(call)?;
            vec![replace(call, format!("{}::class", text(args[0], src)))]
        }
        Rule::StrFunctionCanBeUsed => {
            let binary = at(&["binary_expression"])?;
            let op = text(binary.child_by_field_name("operator")?, src);
            let left = unparen(binary.child_by_field_name("left")?);
            let call = if left.kind() == "function_call_expression" {
                left
            } else {
                unparen(binary.child_by_field_name("right")?)
            };
            let args = call_args(call)?;
            let negate = detail == "str_contains" && op == "===";
            vec![replace(
                binary,
                format!(
                    "{}{detail}({}, {})",
                    if negate { "!" } else { "" },
                    text(args[0], src),
                    text(args[1], src)
                ),
            )]
        }
        Rule::UnnecessaryLocalVariable => {
            let var = at(&["variable_name"])?;
            let assignment = var.parent()?;
            let statement = assignment.parent()?;
            let ret = statement.next_named_sibling()?;
            let right = assignment.child_by_field_name("right")?;
            vec![Edit {
                path: None,
                start: statement.start_byte(),
                end: ret.end_byte(),
                text: format!("return {};", text(right, src)),
            }]
        }
        Rule::UnnecessaryBoolCast => {
            let cast = at(&["cast_expression"])?;
            vec![replace(cast, text(cast.child_by_field_name("value")?, src))]
        }
        Rule::RedundantBreakArgument => {
            let level = at(&[])?;
            let keyword_end = level.prev_sibling()?.end_byte();
            vec![Edit {
                path: None,
                start: keyword_end,
                end: level.end_byte(),
                text: String::new(),
            }]
        }
        Rule::InArraySingleElement => {
            let call = at(&["function_call_expression"])?;
            let args = call_args(call)?;
            let element = unparen(args[1]).named_child(0)?.named_child(0)?;
            let strict = args
                .get(2)
                .is_some_and(|a| text(*a, src).eq_ignore_ascii_case("true"));
            let op = if strict { "===" } else { "==" };
            let comparison = format!("{} {op} {}", text(args[0], src), text(element, src));
            let wrap = call.parent().is_some_and(|p| {
                matches!(
                    p.kind(),
                    "unary_op_expression" | "binary_expression" | "cast_expression"
                )
            });
            vec![replace(
                call,
                if wrap {
                    format!("({comparison})")
                } else {
                    comparison
                },
            )]
        }
        Rule::ArrayAppendUsingCount => {
            let subscript = at(&["subscript_expression"])?;
            vec![replace(
                subscript,
                format!("{}[]", text(subscript.named_child(0)?, src)),
            )]
        }
        Rule::NestedDirname => {
            let call = at(&["function_call_expression"])?;
            let (mut levels, mut inner) = (1, call);
            loop {
                let arg = unparen(*call_args(inner)?.first()?);
                if call_name(arg, src).as_deref() == Some("dirname")
                    && call_args(arg).is_some_and(|a| a.len() == 1)
                {
                    levels += 1;
                    inner = arg;
                } else {
                    break vec![replace(
                        call,
                        format!("dirname({}, {levels})", text(arg, src)),
                    )];
                }
            }
        }
        Rule::NestedMinMax => {
            let call = at(&["function_call_expression"])?;
            let args = call_args(call)?;
            let mut flat = Vec::new();
            for a in args {
                let inner = unparen(a);
                match call_args(inner) {
                    Some(nested)
                        if call_name(inner, src).as_deref() == Some(detail)
                            && nested.len() >= 2 =>
                    {
                        flat.extend(nested.iter().map(|n| text(*n, src).to_string()));
                    }
                    _ => flat.push(text(a, src).to_string()),
                }
            }
            vec![replace(call, format!("{detail}({})", flat.join(", ")))]
        }
        Rule::UnnecessarySemicolon => vec![replace(at(&[])?, "")],
        Rule::ElseIf => {
            let keyword = at(&[])?;
            let clause = keyword.parent()?;
            let nested_if = clause.child_by_field_name("body")?;
            vec![Edit {
                path: None,
                start: keyword.start_byte(),
                end: nested_if.start_byte() + 2,
                text: "elseif".into(),
            }]
        }
        Rule::MissingVisibility => {
            let mut member = at(&[])?;
            while !matches!(
                member.kind(),
                "method_declaration" | "property_declaration" | "const_declaration"
            ) {
                member = member.parent()?;
            }
            let mut c = member.walk();
            let first = member
                .children(&mut c)
                .find(|n| !matches!(n.kind(), "attribute_list" | "comment"))?;
            vec![insert(first.start_byte(), "public ")]
        }
        Rule::ModifierOrder => {
            let mut member = root.descendant_for_byte_range(start, start)?;
            while !matches!(
                member.kind(),
                "method_declaration" | "property_declaration" | "const_declaration"
            ) {
                member = member.parent()?;
            }
            let mut c = member.walk();
            let mut modifiers: Vec<Node> = member
                .children(&mut c)
                .filter(|n| n.start_byte() >= start && n.end_byte() <= end && !n.is_extra())
                .collect();
            let rank = |n: &Node| match n.kind() {
                "abstract_modifier" | "final_modifier" => 0,
                "visibility_modifier" => 1,
                _ => 2,
            };
            modifiers.sort_by_key(rank);
            let words: Vec<&str> = modifiers.iter().map(|m| text(*m, src)).collect();
            vec![Edit {
                path: None,
                start,
                end,
                text: words.join(" "),
            }]
        }
        Rule::NewWithoutParentheses => vec![insert(end, "()")],
        Rule::RedundantMethodOverride => {
            let method = at(&[])?
                .parent()
                .filter(|m| m.kind() == "method_declaration")?;
            vec![remove_lines_edit(
                src,
                method.start_byte(),
                method.end_byte(),
            )]
        }
        Rule::UnnecessaryFullyQualifiedName => {
            let name = at(&["qualified_name"])?;
            vec![replace(name, text(name, src).rsplit('\\').next()?)]
        }
        Rule::LeadingBackslashInUse => vec![Edit {
            path: None,
            start,
            end: start + 1,
            text: String::new(),
        }],
        Rule::UnnecessaryStopStatement => vec![remove_lines_edit(src, start, end)],
        Rule::LongTypeCast => vec![replace(at(&["cast_type"])?, detail)],
        Rule::AssertEqualsLiteral
        | Rule::AssertCount
        | Rule::AssertArrayHasKey
        | Rule::AssertEmpty
        | Rule::AssertContains
        | Rule::AssertInstanceOf
        | Rule::AssertArgumentsOrder => {
            let call = at(&["member_call_expression", "scoped_call_expression"])?;
            let rewrite = assertion_rewrite(call, src)?;
            let consumed = match rule {
                Rule::AssertEqualsLiteral | Rule::AssertCount | Rule::AssertArgumentsOrder => 2,
                _ => 1,
            };
            let mut args = rewrite.args;
            args.extend(
                call_args(call)?
                    .iter()
                    .skip(consumed)
                    .map(|a| text(*a, src).to_string()),
            );
            let name = call.child_by_field_name("name")?;
            let arguments = call.child_by_field_name("arguments")?;
            vec![Edit {
                path: None,
                start: name.start_byte(),
                end: arguments.end_byte(),
                text: format!("{}({})", rewrite.method, args.join(", ")),
            }]
        }
        _ => return None,
    };
    Some(edits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Index;

    /// Applies the first quick fix of the first diagnostic of `rule`.
    fn fixed(src: &str, rule: Rule) -> String {
        let index = Index::default();
        let diag = diagnostics(&index, src)
            .into_iter()
            .find(|d| matches!(&d.code, Code::Rule(r, _) if *r == rule))
            .unwrap_or_else(|| panic!("no {rule:?} in {src}"));
        let fix = quick_fixes(&index, src, &diag, |_| None)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("no fix for {rule:?}"));
        crate::refactor::apply_edits(src, &fix.edits)
    }

    #[test]
    fn print_r_fix_should_pass_true() {
        assert_eq!(
            fixed("<?php $s = print_r($a);", Rule::OutputFunctionResultUsed),
            "<?php $s = print_r($a, true);"
        );
    }

    #[test]
    fn rand_fix_should_swap_bounds() {
        assert_eq!(
            fixed("<?php $r = rand(10, 1);", Rule::RandArgumentsReversed),
            "<?php $r = rand(1, 10);"
        );
    }

    #[test]
    fn continue_fix_should_target_the_loop() {
        assert_eq!(
            fixed(
                "<?php foreach ($a as $x) { switch ($x) { case 1: continue; } }",
                Rule::ContinueInSwitch
            ),
            "<?php foreach ($a as $x) { switch ($x) { case 1: continue 2; } }"
        );
    }

    #[test]
    fn interpolation_fix_should_move_the_dollar() {
        assert_eq!(
            fixed("<?php $s = \"a ${b}\";", Rule::DollarBraceInterpolation),
            "<?php $s = \"a {$b}\";"
        );
    }

    #[test]
    fn nullable_fix_should_add_question_mark() {
        assert_eq!(
            fixed(
                "<?php function f(Foo $a = null) {}",
                Rule::ImplicitlyNullableParameter
            ),
            "<?php function f(?Foo $a = null) {}"
        );
    }

    #[test]
    fn nullable_fix_should_extend_union() {
        assert_eq!(
            fixed(
                "<?php function f(A|B $a = null) {}",
                Rule::ImplicitlyNullableParameter
            ),
            "<?php function f(A|B|null $a = null) {}"
        );
    }

    #[test]
    fn rewritten_value_fix_should_drop_first_write() {
        assert_eq!(
            fixed(
                "<?php\n$a['k'] = 1;\n$a['k'] = 2;\n",
                Rule::ValueImmediatelyRewritten
            ),
            "<?php\n$a['k'] = 2;\n"
        );
    }

    #[test]
    fn short_ternary_fix() {
        assert_eq!(
            fixed("<?php $a = $b ? $b : $c;", Rule::TernaryToShort),
            "<?php $a = $b ?: $c;"
        );
    }

    #[test]
    fn negated_condition_fix() {
        assert_eq!(
            fixed(
                "<?php $a = $b > 1 ? false : true;",
                Rule::TernaryToCondition
            ),
            "<?php $a = !($b > 1);"
        );
    }

    #[test]
    fn coalesce_fix() {
        assert_eq!(
            fixed(
                "<?php $a = isset($b['k']) ? $b['k'] : 0;",
                Rule::IssetToCoalesce
            ),
            "<?php $a = $b['k'] ?? 0;"
        );
    }

    #[test]
    fn nullsafe_fix() {
        assert_eq!(
            fixed(
                "<?php $a = $u !== null ? $u->name() : null;",
                Rule::NullsafeCanBeUsed
            ),
            "<?php $a = $u?->name();"
        );
    }

    #[test]
    fn combine_isset_fix() {
        assert_eq!(
            fixed("<?php $a = isset($b) && isset($c);", Rule::CombineIsset),
            "<?php $a = isset($b, $c);"
        );
    }

    #[test]
    fn boolean_comparison_fix_should_negate_false() {
        assert_eq!(
            fixed("<?php $a = $b == false;", Rule::BooleanComparison),
            "<?php $a = !$b;"
        );
    }

    #[test]
    fn array_push_fix() {
        assert_eq!(
            fixed("<?php array_push($a, 1);", Rule::ArrayPushSingle),
            "<?php $a[] = 1;"
        );
    }

    #[test]
    fn pow_fix_should_parenthesize_complex_operands() {
        assert_eq!(
            fixed("<?php $a = pow($b + 1, 2);", Rule::PowToOperator),
            "<?php $a = ($b + 1) ** 2;"
        );
    }

    #[test]
    fn long_array_fix() {
        assert_eq!(
            fixed("<?php $a = array(1, array(2));", Rule::LongArraySyntax),
            "<?php $a = [1, array(2)];"
        );
    }

    #[test]
    fn closing_tag_fix() {
        assert_eq!(
            fixed("<?php\necho 1;\n?>\n", Rule::RedundantClosingTag),
            "<?php\necho 1;\n"
        );
    }

    #[test]
    fn arrow_function_fix() {
        assert_eq!(
            fixed(
                "<?php $f = static function (int $x) use ($y): int { return $x + $y; };",
                Rule::ClosureToArrowFunction
            ),
            "<?php $f = static fn(int $x): int => $x + $y;"
        );
    }

    #[test]
    fn str_contains_fix_should_negate_identity_with_false() {
        assert_eq!(
            fixed(
                "<?php $a = strpos($h, 'x') === false;",
                Rule::StrFunctionCanBeUsed
            ),
            "<?php $a = !str_contains($h, 'x');"
        );
    }

    #[test]
    fn inline_local_fix() {
        assert_eq!(
            fixed(
                "<?php function f() { $x = g(); return $x; }",
                Rule::UnnecessaryLocalVariable
            ),
            "<?php function f() { return g(); }"
        );
    }

    #[test]
    fn break_argument_fix() {
        assert_eq!(
            fixed("<?php while (1) { break 1; }", Rule::RedundantBreakArgument),
            "<?php while (1) { break; }"
        );
    }

    #[test]
    fn in_array_fix_should_respect_strict_flag() {
        assert_eq!(
            fixed(
                "<?php $a = in_array($x, ['a'], true);",
                Rule::InArraySingleElement
            ),
            "<?php $a = $x === 'a';"
        );
    }

    #[test]
    fn dirname_fix_should_count_levels() {
        assert_eq!(
            fixed(
                "<?php $a = dirname(dirname(dirname($p)));",
                Rule::NestedDirname
            ),
            "<?php $a = dirname($p, 3);"
        );
    }

    #[test]
    fn min_max_fix_should_flatten() {
        assert_eq!(
            fixed("<?php $a = max($b, max($c, $d));", Rule::NestedMinMax),
            "<?php $a = max($b, $c, $d);"
        );
    }

    #[test]
    fn elseif_fix() {
        assert_eq!(
            fixed("<?php if ($a) {} else if ($b) {}", Rule::ElseIf),
            "<?php if ($a) {} elseif ($b) {}"
        );
    }

    #[test]
    fn visibility_fix_should_go_before_static() {
        assert_eq!(
            fixed(
                "<?php class A { static function f() {} }",
                Rule::MissingVisibility
            ),
            "<?php class A { public static function f() {} }"
        );
    }

    #[test]
    fn modifier_order_fix() {
        assert_eq!(
            fixed(
                "<?php class A { static public function f() {} }",
                Rule::ModifierOrder
            ),
            "<?php class A { public static function f() {} }"
        );
    }

    #[test]
    fn new_parentheses_fix() {
        assert_eq!(
            fixed("<?php $a = new Foo;", Rule::NewWithoutParentheses),
            "<?php $a = new Foo();"
        );
    }

    #[test]
    fn long_cast_fix() {
        assert_eq!(
            fixed("<?php $a = (integer) $b;", Rule::LongTypeCast),
            "<?php $a = (int) $b;"
        );
    }

    #[test]
    fn redundant_override_fix_should_remove_method() {
        assert_eq!(
            fixed(
                "<?php\nclass A extends B\n{\n    public function f($a) { return parent::f($a); }\n}\n",
                Rule::RedundantMethodOverride
            ),
            "<?php\nclass A extends B\n{\n}\n"
        );
    }

    #[test]
    fn fqn_fix_should_use_short_name() {
        assert_eq!(
            fixed(
                "<?php namespace App; $a = new \\App\\Post();",
                Rule::UnnecessaryFullyQualifiedName
            ),
            "<?php namespace App; $a = new Post();"
        );
    }

    #[test]
    fn assertion_fix_should_keep_message() {
        assert_eq!(
            fixed(
                "<?php $this->assertEquals(2, count($x), 'msg');",
                Rule::AssertCount
            ),
            "<?php $this->assertCount(2, $x, 'msg');"
        );
    }

    #[test]
    fn instance_of_assertion_fix() {
        assert_eq!(
            fixed(
                "<?php $this->assertTrue($x instanceof Foo);",
                Rule::AssertInstanceOf
            ),
            "<?php $this->assertInstanceOf(Foo::class, $x);"
        );
    }
}
