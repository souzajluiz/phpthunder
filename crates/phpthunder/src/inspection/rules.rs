//! Catalogue of the rule-based inspections: each [`Rule`] has a stable
//! slug (used as the language-server code and translation key), a severity,
//! an English message, and optionally a quick fix.
//!
//! Rules that need data beyond their position carry it as the diagnostic's
//! `detail` (a name, a count, a replacement); front ends pass it to their
//! translations as `$detail`.

use super::*;
use tree_sitter::Node;

/// One rule-based inspection. See [`Rule::slug`] for its stable name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rule {
    // Probable bugs.
    /// A non-empty `case` that falls through into the next one.
    MissingBreak,
    /// A `match` arm condition repeated; the later arm never matches.
    DuplicateMatchCondition,
    /// An optional parameter before a required one (deprecated in PHP 8).
    OptionalBeforeRequired,
    /// Division or modulo by the literal `0`.
    DivisionByZero,
    /// `foreach ($a as $a)`: the loop variable overwrites the array.
    ForeachOverwritesArray,
    /// A nested `foreach` reusing the outer loop's key or value variable.
    ForeachVariableConflict,
    /// `var_export`/`print_r` result used without `return: true`.
    OutputFunctionResultUsed,
    /// `rand(10, 1)`: bounds in the wrong order.
    RandArgumentsReversed,
    /// An expression statement with no effect (`$a == 1;`).
    ExpressionResultUnused,
    /// `continue` targeting a `switch`, which acts like `break`.
    ContinueInSwitch,
    /// A `catch` that only rethrows what it caught.
    ExceptionImmediatelyRethrown,
    /// An unparenthesized nested ternary (fatal since PHP 8).
    NestedTernary,
    /// `"${var}"` interpolation (deprecated in PHP 8.2).
    DollarBraceInterpolation,
    /// `Type $x = null` without `?` (deprecated in PHP 8.4).
    ImplicitlyNullableParameter,
    /// A magic method that is not public.
    MagicMethodNotPublic,
    /// A magic method whose `static` modifier is wrong.
    MagicMethodStatic,
    /// A function mixing `return;` and `return value;`.
    InconsistentReturn,
    /// `strpos`/`array_search` used as a boolean, where `0` reads as false.
    SearchResultAsBoolean,
    /// A `sprintf`-like call whose argument count differs from its format.
    FormatArgumentsMismatch,
    /// A promoted constructor property assigned again in the constructor.
    RedundantPromotedAssignment,
    /// A value written and overwritten by the next statement.
    ValueImmediatelyRewritten,
    /// A removed cast (`(unset)`, `(real)`).
    RemovedCast,

    // Simplifications.
    /// `$a ? $a : $b`, shorter as `$a ?: $b`.
    TernaryToShort,
    /// `$cond ? true : false`, which is `$cond` itself.
    TernaryToCondition,
    /// `isset($a) ? $a : $b`, shorter as `$a ?? $b`.
    IssetToCoalesce,
    /// `$a !== null ? $a->b : null`, shorter as `$a?->b`.
    NullsafeCanBeUsed,
    /// `isset($a) && isset($b)`, shorter as `isset($a, $b)`.
    CombineIsset,
    /// `$a == true`, simpler as `$a`.
    BooleanComparison,
    /// `array_push($a, $x)` with one value, simpler as `$a[] = $x`.
    ArrayPushSingle,
    /// `pow($a, $b)`, simpler as `$a ** $b`.
    PowToOperator,
    /// `array(…)`, shorter as `[…]`.
    LongArraySyntax,
    /// A `?>` closing a file of pure PHP.
    RedundantClosingTag,
    /// A closure that only returns an expression; an arrow function.
    ClosureToArrowFunction,
    /// `get_class($a)`, simpler as `$a::class`.
    GetClassToClassConstant,
    /// `strpos(…) !== false` and friends; `str_contains` says it.
    StrFunctionCanBeUsed,
    /// `$x = expr; return $x;`: return the expression directly.
    UnnecessaryLocalVariable,
    /// The `@` error-silencing operator.
    SilenceOperator,
    /// `(bool)` of something already boolean.
    UnnecessaryBoolCast,
    /// `break 1;` / `continue 1;`.
    RedundantBreakArgument,
    /// `in_array($x, [$y])`, simpler as a comparison.
    InArraySingleElement,
    /// `$a[count($a)] = $x`, simpler as `$a[] = $x`.
    ArrayAppendUsingCount,
    /// `dirname(dirname($p))`, simpler as `dirname($p, 2)`.
    NestedDirname,
    /// `max($a, max($b, $c))`, simpler as `max($a, $b, $c)`.
    NestedMinMax,
    /// A `match` with only a `default` arm.
    MatchOnlyDefault,
    /// A stray `;`.
    UnnecessarySemicolon,
    /// A parameter the function never reads.
    UnusedParameter,

    // Code style (PSR-12).
    /// `else if`, written `elseif` by PSR-12.
    ElseIf,
    /// `var $x;`, written `public $x;`.
    VarKeyword,
    /// A member declared without visibility.
    MissingVisibility,
    /// Modifiers out of PSR-12 order (`static public`).
    ModifierOrder,
    /// `new Foo` without parentheses.
    NewWithoutParentheses,
    /// `(integer)`, `(boolean)`, `(double)`: long cast names.
    LongTypeCast,
    /// More than one class-like declaration in a file.
    MultipleClassesInFile,
    /// A `@param` tag naming no parameter.
    PhpDocUnknownParameter,

    // PHPUnit.
    /// `assertEquals(true, $x)`, clearer as `assertTrue($x)`.
    AssertEqualsLiteral,
    /// `assertEquals(3, count($x))`, clearer as `assertCount(3, $x)`.
    AssertCount,
    /// `assertTrue(array_key_exists($k, $a))`, clearer as `assertArrayHasKey`.
    AssertArrayHasKey,
    /// `assertTrue(empty($x))`, clearer as `assertEmpty($x)`.
    AssertEmpty,
    /// `assertTrue(in_array($x, $a))`, clearer as `assertContains($x, $a)`.
    AssertContains,
    /// `assertTrue($x instanceof Foo)`, clearer as `assertInstanceOf`.
    AssertInstanceOf,
    /// `assertEquals($actual, 'expected')`: expected value comes first.
    AssertArgumentsOrder,

    // Index-aware checks.
    /// A concrete class missing abstract or interface methods.
    MissingImplementation,
    /// A class extending a `final` class.
    ExtendsFinalClass,
    /// A method overriding a `final` one.
    OverridesFinalMethod,
    /// `extends`/`implements` naming the wrong kind (a class implemented,
    /// an interface extended by a class, …).
    WrongSupertypeKind,
    /// `new` on an abstract class, interface, trait or enum.
    InstantiateAbstract,
    /// An override with weaker visibility than the method it overrides.
    WeakerAccess,
    /// An override that adds or drops `static`.
    StaticMismatch,
    /// An override whose parameters cannot accept every call the parent can.
    SignatureMismatch,
    /// A call passing fewer arguments than the callee requires.
    TooFewArguments,
    /// A call passing more arguments than the callee declares.
    TooManyArguments,
    /// A private or protected member used where it is not accessible.
    InaccessibleMember,
    /// `Foo::bar()` on an instance method, outside `Foo`'s hierarchy.
    StaticCallToInstanceMethod,
    /// `Foo::BAR` naming no constant or case.
    UndefinedClassConstant,
    /// The result of a `void` function used as a value.
    VoidResultUsed,
    /// A `catch` that an earlier `catch` of a parent class already handles.
    CatchOrder,
    /// A `match` over enum cases with no `default` that misses cases.
    EnumMatchNotExhaustive,
    /// A constructor not calling its parent's constructor.
    MissingParentConstructorCall,
    /// A readonly property written outside its class.
    ReadonlyModified,
    /// A PHPUnit data provider naming no method.
    UndefinedDataProvider,
}

impl Rule {
    /// Stable kebab-case name: the language-server diagnostic code and the
    /// suffix of the translation keys `diagnostic-<slug>` / `fix-<slug>`.
    pub fn slug(self) -> &'static str {
        match self {
            Rule::MissingBreak => "missing-break",
            Rule::DuplicateMatchCondition => "duplicate-match-condition",
            Rule::OptionalBeforeRequired => "optional-before-required",
            Rule::DivisionByZero => "division-by-zero",
            Rule::ForeachOverwritesArray => "foreach-overwrites-array",
            Rule::ForeachVariableConflict => "foreach-variable-conflict",
            Rule::OutputFunctionResultUsed => "output-function-result-used",
            Rule::RandArgumentsReversed => "rand-arguments-reversed",
            Rule::ExpressionResultUnused => "expression-result-unused",
            Rule::ContinueInSwitch => "continue-in-switch",
            Rule::ExceptionImmediatelyRethrown => "exception-immediately-rethrown",
            Rule::NestedTernary => "nested-ternary",
            Rule::DollarBraceInterpolation => "dollar-brace-interpolation",
            Rule::ImplicitlyNullableParameter => "implicitly-nullable-parameter",
            Rule::MagicMethodNotPublic => "magic-method-not-public",
            Rule::MagicMethodStatic => "magic-method-static",
            Rule::InconsistentReturn => "inconsistent-return",
            Rule::SearchResultAsBoolean => "search-result-as-boolean",
            Rule::FormatArgumentsMismatch => "format-arguments-mismatch",
            Rule::RedundantPromotedAssignment => "redundant-promoted-assignment",
            Rule::ValueImmediatelyRewritten => "value-immediately-rewritten",
            Rule::RemovedCast => "removed-cast",
            Rule::TernaryToShort => "ternary-to-short",
            Rule::TernaryToCondition => "ternary-to-condition",
            Rule::IssetToCoalesce => "isset-to-coalesce",
            Rule::NullsafeCanBeUsed => "nullsafe-can-be-used",
            Rule::CombineIsset => "combine-isset",
            Rule::BooleanComparison => "boolean-comparison",
            Rule::ArrayPushSingle => "array-push-single",
            Rule::PowToOperator => "pow-to-operator",
            Rule::LongArraySyntax => "long-array-syntax",
            Rule::RedundantClosingTag => "redundant-closing-tag",
            Rule::ClosureToArrowFunction => "closure-to-arrow-function",
            Rule::GetClassToClassConstant => "get-class-to-class-constant",
            Rule::StrFunctionCanBeUsed => "str-function-can-be-used",
            Rule::UnnecessaryLocalVariable => "unnecessary-local-variable",
            Rule::SilenceOperator => "silence-operator",
            Rule::UnnecessaryBoolCast => "unnecessary-bool-cast",
            Rule::RedundantBreakArgument => "redundant-break-argument",
            Rule::InArraySingleElement => "in-array-single-element",
            Rule::ArrayAppendUsingCount => "array-append-using-count",
            Rule::NestedDirname => "nested-dirname",
            Rule::NestedMinMax => "nested-min-max",
            Rule::MatchOnlyDefault => "match-only-default",
            Rule::UnnecessarySemicolon => "unnecessary-semicolon",
            Rule::UnusedParameter => "unused-parameter",
            Rule::ElseIf => "else-if",
            Rule::VarKeyword => "var-keyword",
            Rule::MissingVisibility => "missing-visibility",
            Rule::ModifierOrder => "modifier-order",
            Rule::NewWithoutParentheses => "new-without-parentheses",
            Rule::LongTypeCast => "long-type-cast",
            Rule::MultipleClassesInFile => "multiple-classes-in-file",
            Rule::PhpDocUnknownParameter => "phpdoc-unknown-parameter",
            Rule::AssertEqualsLiteral => "assert-equals-literal",
            Rule::AssertCount => "assert-count",
            Rule::AssertArrayHasKey => "assert-array-has-key",
            Rule::AssertEmpty => "assert-empty",
            Rule::AssertContains => "assert-contains",
            Rule::AssertInstanceOf => "assert-instance-of",
            Rule::AssertArgumentsOrder => "assert-arguments-order",
            Rule::MissingImplementation => "missing-implementation",
            Rule::ExtendsFinalClass => "extends-final-class",
            Rule::OverridesFinalMethod => "overrides-final-method",
            Rule::WrongSupertypeKind => "wrong-supertype-kind",
            Rule::InstantiateAbstract => "instantiate-abstract",
            Rule::WeakerAccess => "weaker-access",
            Rule::StaticMismatch => "static-mismatch",
            Rule::SignatureMismatch => "signature-mismatch",
            Rule::TooFewArguments => "too-few-arguments",
            Rule::TooManyArguments => "too-many-arguments",
            Rule::InaccessibleMember => "inaccessible-member",
            Rule::StaticCallToInstanceMethod => "static-call-to-instance-method",
            Rule::UndefinedClassConstant => "undefined-class-constant",
            Rule::VoidResultUsed => "void-result-used",
            Rule::CatchOrder => "catch-order",
            Rule::EnumMatchNotExhaustive => "enum-match-not-exhaustive",
            Rule::MissingParentConstructorCall => "missing-parent-constructor-call",
            Rule::ReadonlyModified => "readonly-modified",
            Rule::UndefinedDataProvider => "undefined-data-provider",
        }
    }

    /// How the rule's findings are shown: definite runtime failures are
    /// errors, likely mistakes warnings, style and simplifications weak.
    pub fn severity(self) -> Severity {
        match self {
            Rule::NestedTernary
            | Rule::MissingImplementation
            | Rule::ExtendsFinalClass
            | Rule::OverridesFinalMethod
            | Rule::WrongSupertypeKind
            | Rule::InstantiateAbstract
            | Rule::WeakerAccess
            | Rule::StaticMismatch
            | Rule::SignatureMismatch
            | Rule::TooFewArguments
            | Rule::InaccessibleMember
            | Rule::StaticCallToInstanceMethod
            | Rule::UndefinedClassConstant
            | Rule::ReadonlyModified
            | Rule::UndefinedDataProvider
            | Rule::RemovedCast
            | Rule::MagicMethodStatic
            | Rule::MagicMethodNotPublic => Severity::Error,
            Rule::MissingBreak
            | Rule::DuplicateMatchCondition
            | Rule::OptionalBeforeRequired
            | Rule::DivisionByZero
            | Rule::ForeachOverwritesArray
            | Rule::ForeachVariableConflict
            | Rule::OutputFunctionResultUsed
            | Rule::RandArgumentsReversed
            | Rule::ExpressionResultUnused
            | Rule::ContinueInSwitch
            | Rule::DollarBraceInterpolation
            | Rule::ImplicitlyNullableParameter
            | Rule::SearchResultAsBoolean
            | Rule::FormatArgumentsMismatch
            | Rule::ValueImmediatelyRewritten
            | Rule::PhpDocUnknownParameter
            | Rule::TooManyArguments
            | Rule::VoidResultUsed
            | Rule::CatchOrder
            | Rule::EnumMatchNotExhaustive => Severity::Warning,
            _ => Severity::Weak,
        }
    }

    /// Whether findings mark code that can simply go (editors fade it).
    pub fn unnecessary(self) -> bool {
        matches!(
            self,
            Rule::UnusedParameter
                | Rule::RedundantClosingTag
                | Rule::UnnecessarySemicolon
                | Rule::RedundantPromotedAssignment
                | Rule::RedundantBreakArgument
        )
    }

    /// The English message; `detail` is the diagnostic's extra data.
    pub fn message(self, detail: &str) -> String {
        match self {
            Rule::MissingBreak => "Case falls through to the next one; add break".into(),
            Rule::DuplicateMatchCondition => {
                format!("Duplicate match condition {detail} never matches")
            }
            Rule::OptionalBeforeRequired => {
                format!("Optional parameter {detail} before a required one")
            }
            Rule::DivisionByZero => "Division by zero".into(),
            Rule::ForeachOverwritesArray => {
                format!("{detail} is both the iterated array and a loop variable")
            }
            Rule::ForeachVariableConflict => {
                format!("{detail} overwrites the outer foreach variable")
            }
            Rule::OutputFunctionResultUsed => {
                format!("{detail}() prints and returns null unless its second argument is true")
            }
            Rule::RandArgumentsReversed => "Minimum is greater than maximum".into(),
            Rule::ExpressionResultUnused => "Expression result is not used".into(),
            Rule::ContinueInSwitch => {
                "continue inside switch acts like break; use continue 2".into()
            }
            Rule::ExceptionImmediatelyRethrown => "Exception is caught only to be rethrown".into(),
            Rule::NestedTernary => {
                "Nested ternary without parentheses is a fatal error since PHP 8".into()
            }
            Rule::DollarBraceInterpolation => {
                "${var} string interpolation is deprecated; use {$var}".into()
            }
            Rule::ImplicitlyNullableParameter => {
                format!(
                    "Implicitly nullable parameter {detail} is deprecated; make the type nullable"
                )
            }
            Rule::MagicMethodNotPublic => format!("Magic method {detail} must be public"),
            Rule::MagicMethodStatic => {
                format!("Magic method {detail} has the wrong static modifier")
            }
            Rule::InconsistentReturn => "Function returns a value on some paths only".into(),
            Rule::SearchResultAsBoolean => {
                format!("{detail}() can return 0, which reads as false; compare with !== false")
            }
            Rule::FormatArgumentsMismatch => {
                format!("Format expects {detail} arguments")
            }
            Rule::RedundantPromotedAssignment => "Promoted property is already assigned".into(),
            Rule::ValueImmediatelyRewritten => {
                format!("{detail} is overwritten before it is read")
            }
            Rule::RemovedCast => format!("The {detail} cast was removed in PHP 8"),
            Rule::TernaryToShort => "Ternary can be shortened to ?:".into(),
            Rule::TernaryToCondition => "Ternary can be replaced with its condition".into(),
            Rule::IssetToCoalesce => "isset() ternary can be replaced with ??".into(),
            Rule::NullsafeCanBeUsed => "Null check can be replaced with ?->".into(),
            Rule::CombineIsset => "isset() calls can be combined".into(),
            Rule::BooleanComparison => "Comparison with a boolean can be simplified".into(),
            Rule::ArrayPushSingle => "array_push() with one value can be $array[] =".into(),
            Rule::PowToOperator => "pow() can be replaced with **".into(),
            Rule::LongArraySyntax => "array() can be replaced with []".into(),
            Rule::RedundantClosingTag => "Redundant closing tag".into(),
            Rule::ClosureToArrowFunction => "Closure can be an arrow function".into(),
            Rule::GetClassToClassConstant => "get_class() can be replaced with ::class".into(),
            Rule::StrFunctionCanBeUsed => format!("Can be replaced with {detail}()"),
            Rule::UnnecessaryLocalVariable => {
                format!("Local variable {detail} is returned right away")
            }
            Rule::SilenceOperator => "Error silencing with @".into(),
            Rule::UnnecessaryBoolCast => "Unnecessary (bool) cast".into(),
            Rule::RedundantBreakArgument => "Redundant argument 1".into(),
            Rule::InArraySingleElement => "in_array() with one element can be a comparison".into(),
            Rule::ArrayAppendUsingCount => "$array[count($array)] can be $array[]".into(),
            Rule::NestedDirname => "Nested dirname() calls can use the levels argument".into(),
            Rule::NestedMinMax => format!("Nested {detail}() calls can be merged"),
            Rule::MatchOnlyDefault => "match has only a default arm".into(),
            Rule::UnnecessarySemicolon => "Unnecessary semicolon".into(),
            Rule::UnusedParameter => format!("Unused parameter {detail}"),
            Rule::ElseIf => "else if should be written elseif".into(),
            Rule::VarKeyword => "var should be replaced with public".into(),
            Rule::MissingVisibility => format!("{detail} has no visibility"),
            Rule::ModifierOrder => "Modifiers out of order".into(),
            Rule::NewWithoutParentheses => "new without parentheses".into(),
            Rule::LongTypeCast => format!("Use the short cast ({detail})"),
            Rule::MultipleClassesInFile => "File declares more than one class".into(),
            Rule::PhpDocUnknownParameter => format!("@param {detail} names no parameter"),
            Rule::AssertEqualsLiteral => format!("Can be {detail}()"),
            Rule::AssertCount => "Can be assertCount()".into(),
            Rule::AssertArrayHasKey => format!("Can be {detail}()"),
            Rule::AssertEmpty => format!("Can be {detail}()"),
            Rule::AssertContains => format!("Can be {detail}()"),
            Rule::AssertInstanceOf => format!("Can be {detail}()"),
            Rule::AssertArgumentsOrder => "Expected value should come first".into(),
            Rule::MissingImplementation => format!("Class must implement {detail}"),
            Rule::ExtendsFinalClass => format!("Cannot extend final class {detail}"),
            Rule::OverridesFinalMethod => format!("Cannot override final method {detail}"),
            Rule::WrongSupertypeKind => format!("{detail} cannot be used here"),
            Rule::InstantiateAbstract => format!("Cannot instantiate {detail}"),
            Rule::WeakerAccess => {
                format!("Visibility must be {detail} or weaker, as in the parent")
            }
            Rule::StaticMismatch => format!("Override of {detail} changes static"),
            Rule::SignatureMismatch => format!("Declaration must be compatible with {detail}"),
            Rule::TooFewArguments => format!("Expects at least {detail} arguments"),
            Rule::TooManyArguments => format!("Expects at most {detail} arguments"),
            Rule::InaccessibleMember => format!("Member {detail} is not accessible here"),
            Rule::StaticCallToInstanceMethod => {
                format!("Non-static method {detail} called statically")
            }
            Rule::UndefinedClassConstant => format!("Undefined class constant {detail}"),
            Rule::VoidResultUsed => format!("{detail} returns void; its result is always null"),
            Rule::CatchOrder => format!("Already caught by the earlier catch of {detail}"),
            Rule::EnumMatchNotExhaustive => format!("match misses enum cases: {detail}"),
            Rule::MissingParentConstructorCall => "Missing parent constructor call".into(),
            Rule::ReadonlyModified => format!("Readonly property {detail} cannot be modified here"),
            Rule::UndefinedDataProvider => format!("Data provider {detail} not found"),
        }
    }

    /// The English quick-fix label, or `None` when the rule has no fix.
    pub fn fix_title(self) -> Option<&'static str> {
        Some(match self {
            Rule::OptionalBeforeRequired => return None,
            Rule::OutputFunctionResultUsed => "Pass true to return the output",
            Rule::RandArgumentsReversed => "Swap arguments",
            Rule::ContinueInSwitch => "Replace with continue 2",
            Rule::DollarBraceInterpolation => "Use {$var}",
            Rule::ImplicitlyNullableParameter => "Make the type nullable",
            Rule::MagicMethodNotPublic => "Make public",
            Rule::RedundantPromotedAssignment => "Remove assignment",
            Rule::ValueImmediatelyRewritten => "Remove first assignment",
            Rule::TernaryToShort => "Replace with ?:",
            Rule::TernaryToCondition => "Replace with condition",
            Rule::IssetToCoalesce => "Replace with ??",
            Rule::NullsafeCanBeUsed => "Replace with ?->",
            Rule::CombineIsset => "Combine isset() calls",
            Rule::BooleanComparison => "Simplify",
            Rule::ArrayPushSingle => "Replace with $array[] =",
            Rule::PowToOperator => "Replace with **",
            Rule::LongArraySyntax => "Replace with []",
            Rule::RedundantClosingTag => "Remove closing tag",
            Rule::ClosureToArrowFunction => "Convert to arrow function",
            Rule::GetClassToClassConstant => "Replace with ::class",
            Rule::StrFunctionCanBeUsed => "Replace",
            Rule::UnnecessaryLocalVariable => "Inline variable",
            Rule::UnnecessaryBoolCast => "Remove cast",
            Rule::RedundantBreakArgument => "Remove argument",
            Rule::InArraySingleElement => "Replace with comparison",
            Rule::ArrayAppendUsingCount => "Replace with $array[]",
            Rule::NestedDirname => "Use the levels argument",
            Rule::NestedMinMax => "Merge calls",
            Rule::UnnecessarySemicolon => "Remove semicolon",
            Rule::ElseIf => "Replace with elseif",
            Rule::VarKeyword => "Replace with public",
            Rule::MissingVisibility => "Add public",
            Rule::ModifierOrder => "Reorder modifiers",
            Rule::NewWithoutParentheses => "Add parentheses",
            Rule::LongTypeCast => "Use short cast",
            Rule::AssertEqualsLiteral
            | Rule::AssertCount
            | Rule::AssertArrayHasKey
            | Rule::AssertEmpty
            | Rule::AssertContains
            | Rule::AssertInstanceOf => "Replace assertion",
            Rule::AssertArgumentsOrder => "Swap arguments",
            Rule::MissingImplementation => "Implement methods",
            Rule::WeakerAccess => "Use the parent's visibility",
            _ => return None,
        })
    }
}

/// A finding of a rule-based inspection, before it becomes a [`Diagnostic`].
/// Its quick fix is computed later from the same range (see
/// [`quick_fixes`]).
pub(crate) struct Finding {
    /// Start byte of the reported range.
    pub start: usize,
    /// End byte of the reported range.
    pub end: usize,
    /// Which rule fired.
    pub rule: Rule,
    /// Extra data for the message (a name, a count).
    pub detail: String,
}

/// Collects rule findings; the inspection modules push into it.
#[derive(Default)]
pub(crate) struct Findings {
    /// Findings in discovery order.
    pub list: Vec<Finding>,
}

impl Findings {
    /// Records that `rule` fired on `node`.
    pub fn add(&mut self, node: Node, rule: Rule, detail: impl Into<String>) {
        self.add_range(node.start_byte(), node.end_byte(), rule, detail);
    }

    /// Records that `rule` fired on bytes `start..end` (part of a node,
    /// such as one tag of a doc comment).
    pub fn add_range(&mut self, start: usize, end: usize, rule: Rule, detail: impl Into<String>) {
        self.list.push(Finding {
            start,
            end,
            rule,
            detail: detail.into(),
        });
    }

    /// Turns every finding into a diagnostic, positioned with `lines`.
    pub fn report(self, src: &str, lines: &crate::syntax::LineIndex, out: &mut Vec<Diagnostic>) {
        for f in self.list {
            out.push(Diagnostic {
                span: lines.span(src, f.start, f.end),
                start: f.start,
                end: f.end,
                severity: f.rule.severity(),
                message: f.rule.message(&f.detail),
                code: Code::Rule(f.rule, f.detail),
            });
        }
    }
}

/// Every named node under `root` (itself included), in document order.
pub(crate) fn descendants(root: Node) -> Vec<Node> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        out.push(n);
        let mut c = n.walk();
        let children: Vec<Node> = n.named_children(&mut c).collect();
        stack.extend(children.into_iter().rev());
    }
    out
}

/// Lower-case name of the function a call expression calls, without a
/// leading `\`; `None` for dynamic calls (`$f()`).
pub(crate) fn call_name(call: Node, src: &str) -> Option<String> {
    if call.kind() != "function_call_expression" {
        return None;
    }
    let f = call.child_by_field_name("function")?;
    if !matches!(f.kind(), "name" | "qualified_name") {
        return None;
    }
    let name = crate::syntax::text(f, src).trim_start_matches('\\');
    // A namespaced call (`Foo\bar()`) is not the global function.
    (!name.contains('\\')).then(|| name.to_ascii_lowercase())
}

/// Argument expressions of a call (`arguments` children, names and spread
/// excluded). `None` when any argument is named or spread, since positions
/// then mean nothing.
pub(crate) fn call_args<'a>(call: Node<'a>) -> Option<Vec<Node<'a>>> {
    let args = call.child_by_field_name("arguments")?;
    let mut c = args.walk();
    let mut out = Vec::new();
    for arg in args.named_children(&mut c) {
        if arg.kind() == "comment" {
            continue;
        }
        if arg.kind() != "argument" || arg.child_by_field_name("name").is_some() {
            return None;
        }
        let expr = arg.named_child(u32::try_from(arg.named_child_count().checked_sub(1)?).ok()?)?;
        if arg.child(0).is_some_and(|t| t.kind() == "...") || expr.kind() == "variadic_unpacking" {
            return None;
        }
        out.push(expr);
    }
    Some(out)
}

/// `node` without wrapping parentheses.
pub(crate) fn unparen(mut node: Node) -> Node {
    while node.kind() == "parenthesized_expression" {
        match node.named_child(0) {
            Some(inner) => node = inner,
            None => break,
        }
    }
    node
}

/// Whether `node` always yields a `bool`: comparisons, logical operators,
/// `!`, `isset`, `empty`, `instanceof`, `true`/`false`.
pub(crate) fn is_boolean(node: Node, src: &str) -> bool {
    let node = unparen(node);
    match node.kind() {
        "boolean" => true,
        "unary_op_expression" => crate::syntax::text(node, src).starts_with('!'),
        "binary_expression" => node.child_by_field_name("operator").is_some_and(|op| {
            matches!(
                crate::syntax::text(op, src).to_ascii_lowercase().as_str(),
                "==" | "==="
                    | "!="
                    | "!=="
                    | "<>"
                    | "<"
                    | ">"
                    | "<="
                    | ">="
                    | "&&"
                    | "||"
                    | "and"
                    | "or"
                    | "xor"
                    | "instanceof"
            )
        }),
        "function_call_expression" => call_name(node, src).is_some_and(|n| {
            matches!(
                n.as_str(),
                "isset"
                    | "empty"
                    | "is_null"
                    | "is_array"
                    | "is_string"
                    | "is_int"
                    | "is_bool"
                    | "is_numeric"
                    | "in_array"
                    | "array_key_exists"
                    | "str_contains"
                    | "str_starts_with"
                    | "str_ends_with"
            )
        }),
        _ => false,
    }
}

/// Whether evaluating `node` cannot have side effects: no calls,
/// assignments, increments, `new`, `include`, `yield` or `print`.
pub(crate) fn is_pure(node: Node) -> bool {
    descendants(node).iter().all(|n| {
        !matches!(
            n.kind(),
            "function_call_expression"
                | "member_call_expression"
                | "nullsafe_member_call_expression"
                | "scoped_call_expression"
                | "assignment_expression"
                | "reference_assignment_expression"
                | "augmented_assignment_expression"
                | "update_expression"
                | "object_creation_expression"
                | "include_expression"
                | "include_once_expression"
                | "require_expression"
                | "require_once_expression"
                | "yield_expression"
                | "print_intrinsic"
                | "throw_expression"
                | "exit_statement"
                | "clone_expression"
                | "shell_command_expression"
        )
    })
}

/// Whether `stmt` ends its `case` (or block): it always leaves via
/// `break`, `continue`, `return`, `throw`, `goto` or `exit`.
pub(crate) fn ends_flow(stmt: Node, src: &str) -> bool {
    match stmt.kind() {
        "break_statement" | "continue_statement" | "return_statement" | "goto_statement"
        | "exit_statement" => true,
        "compound_statement" | "colon_block" => {
            let mut c = stmt.walk();
            stmt.named_children(&mut c)
                .filter(|n| n.kind() != "comment")
                .last()
                .is_some_and(|last| ends_flow(last, src))
        }
        "if_statement" => {
            let mut c = stmt.walk();
            let mut has_else = false;
            let branches = stmt
                .children_by_field_name("alternative", &mut c)
                .all(|alt| {
                    has_else |= alt.kind() == "else_clause";
                    alt.child_by_field_name("body")
                        .is_some_and(|b| ends_flow(b, src))
                });
            has_else
                && branches
                && stmt
                    .child_by_field_name("body")
                    .is_some_and(|b| ends_flow(b, src))
        }
        "expression_statement" => {
            stmt.named_child(0).is_some_and(|e| {
                call_name(e, src).is_some_and(|n| NEVER_RETURNS.contains(&n.as_str()))
            }) || always_exits(stmt, src, &|_| false)
        }
        _ => always_exits(stmt, src, &|_| false),
    }
}

/// Global functions that never return: they throw or end the request.
pub(crate) const NEVER_RETURNS: &[&str] = &["abort", "dd", "exit", "die"];
