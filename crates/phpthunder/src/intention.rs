//! Context actions offered at the caret (`⌥↵`) when no problem is
//! reported there: rewrites between equivalent forms of the same code
//! (quotes, interpolation, `if`/ternary/`match`, closures, compound
//! assignments, braces, array layout) and small code generators
//! (`: void`, promoted properties, `strict_types`, imports, `#[\Override]`).
//!
//! Each action is a [`Fix`] whose kind is [`FixKind::Intention`], so front
//! ends apply it like a quick fix and phrase its title from the [`Action`].

mod classes;
mod control;
mod docs;
mod expressions;

use crate::index::Index;
use crate::inspection::{Edit, Fix, FixKind};
use crate::syntax::{parse, text};
use tree_sitter::Node;

/// One context action. See [`Action::slug`] for its stable name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    /// `'text'` to `"text"`.
    ToDoubleQuotes,
    /// `"text"` to `'text'`.
    ToSingleQuotes,
    /// `'Hi ' . $name . '!'` to `"Hi {$name}!"`.
    ConcatToInterpolation,
    /// `$a < $b` to `$b > $a`.
    FlipBinary,
    /// `if (c) { A } else { B }` to `if (!c) { B } else { A }`.
    InvertIf,
    /// `if (a) { if (b) { … } }` to `if (a && b) { … }`.
    MergeNestedIf,
    /// `if (a && b) { … }` to nested `if`s.
    SplitAndCondition,
    /// Wraps a control statement's single-statement body in braces.
    AddBraces,
    /// Drops braces around a single-statement body.
    RemoveBraces,
    /// `if`/`else` returning or assigning to a ternary.
    IfToTernary,
    /// A returned or assigned ternary to `if`/`else`.
    TernaryToIf,
    /// A `switch` whose cases each return or assign to a `match`.
    SwitchToMatch,
    /// `fn ($x) => …` to `function ($x) use (…) { return …; }`.
    ArrowToClosure,
    /// Declares `: void` on a function that returns nothing.
    AddVoidReturnType,
    /// A constructor parameter assigned to a property to a promoted one.
    PromoteProperty,
    /// Adds `declare(strict_types=1);`.
    AddStrictTypes,
    /// A qualified class name to an import plus its short name.
    ImportClass,
    /// Adds `#[\Override]` to a method that overrides another.
    AddOverride,
    /// Adds `final` to a class.
    MakeFinal,
    /// Puts each array element on its own line.
    SplitArray,
    /// Puts a multi-line array on one line.
    JoinArray,
    /// `==` to `===`, `!=` to `!==`.
    StrictComparison,
    /// `$a = $a + $b` to `$a += $b`.
    ToCompoundAssignment,
    /// `$a += $b` to `$a = $a + $b`.
    ExpandCompoundAssignment,
    /// `$a = $a + 1` / `$a += 1` to `$a++`.
    ToIncrement,
    /// `!($a && $b)` to `!$a || !$b`, and back.
    DeMorgan,
    /// A PHPDoc block for the function, method or property at the caret.
    GeneratePhpDoc,
    /// PHP 8.4 `get`/`set` hooks for a typed property.
    AddPropertyHooks,
}

impl Action {
    /// Stable kebab-case name; front ends translate `intention-<slug>`.
    pub fn slug(self) -> &'static str {
        match self {
            Action::ToDoubleQuotes => "to-double-quotes",
            Action::ToSingleQuotes => "to-single-quotes",
            Action::ConcatToInterpolation => "concat-to-interpolation",
            Action::FlipBinary => "flip-binary",
            Action::InvertIf => "invert-if",
            Action::MergeNestedIf => "merge-nested-if",
            Action::SplitAndCondition => "split-and-condition",
            Action::AddBraces => "add-braces",
            Action::RemoveBraces => "remove-braces",
            Action::IfToTernary => "if-to-ternary",
            Action::TernaryToIf => "ternary-to-if",
            Action::SwitchToMatch => "switch-to-match",
            Action::ArrowToClosure => "arrow-to-closure",
            Action::AddVoidReturnType => "add-void-return-type",
            Action::PromoteProperty => "promote-property",
            Action::AddStrictTypes => "add-strict-types",
            Action::ImportClass => "import-class",
            Action::AddOverride => "add-override",
            Action::MakeFinal => "make-final",
            Action::SplitArray => "split-array",
            Action::JoinArray => "join-array",
            Action::StrictComparison => "strict-comparison",
            Action::ToCompoundAssignment => "to-compound-assignment",
            Action::ExpandCompoundAssignment => "expand-compound-assignment",
            Action::ToIncrement => "to-increment",
            Action::DeMorgan => "de-morgan",
            Action::GeneratePhpDoc => "generate-phpdoc",
            Action::AddPropertyHooks => "add-property-hooks",
        }
    }

    /// The English menu label.
    pub fn title(self) -> &'static str {
        match self {
            Action::ToDoubleQuotes => "Convert to double-quoted string",
            Action::ToSingleQuotes => "Convert to single-quoted string",
            Action::ConcatToInterpolation => "Replace concatenation with interpolation",
            Action::FlipBinary => "Flip operands",
            Action::InvertIf => "Invert if condition",
            Action::MergeNestedIf => "Merge nested if",
            Action::SplitAndCondition => "Split into nested if",
            Action::AddBraces => "Add braces",
            Action::RemoveBraces => "Remove braces",
            Action::IfToTernary => "Replace if with ternary",
            Action::TernaryToIf => "Replace ternary with if",
            Action::SwitchToMatch => "Replace switch with match",
            Action::ArrowToClosure => "Convert to closure",
            Action::AddVoidReturnType => "Add void return type",
            Action::PromoteProperty => "Promote to constructor property",
            Action::AddStrictTypes => "Add strict_types declaration",
            Action::ImportClass => "Import class",
            Action::AddOverride => "Add #[\\Override]",
            Action::MakeFinal => "Make class final",
            Action::SplitArray => "Put elements on separate lines",
            Action::JoinArray => "Put elements on one line",
            Action::StrictComparison => "Use strict comparison",
            Action::ToCompoundAssignment => "Use compound assignment",
            Action::ExpandCompoundAssignment => "Expand compound assignment",
            Action::ToIncrement => "Use increment",
            Action::DeMorgan => "Apply De Morgan's law",
            Action::GeneratePhpDoc => "Generate PHPDoc",
            Action::AddPropertyHooks => "Add property hooks",
        }
    }
}

/// Context passed to every intention provider.
pub(crate) struct Context<'t> {
    /// The project index (for actions that look at other files).
    pub index: &'t Index,
    /// The file's source.
    pub src: &'t str,
    /// The file's syntax tree.
    pub tree: &'t tree_sitter::Tree,
    /// The file's syntax tree root.
    pub root: Node<'t>,
    /// Caret (or selection start) byte offset.
    pub at: usize,
}

impl<'t> Context<'t> {
    /// Innermost node at the caret and its ancestors, innermost first.
    pub fn ancestors(&self) -> Vec<Node<'t>> {
        let mut out = Vec::new();
        let mut cur = self.root.descendant_for_byte_range(self.at, self.at);
        while let Some(n) = cur {
            out.push(n);
            cur = n.parent();
        }
        out
    }

    /// The innermost ancestor of one of `kinds`, without crossing a
    /// function, class or (when `stop_at_statement`) statement boundary.
    pub fn enclosing(&self, kinds: &[&str], stop_at_statement: bool) -> Option<Node<'t>> {
        for n in self.ancestors() {
            if kinds.contains(&n.kind()) {
                return Some(n);
            }
            let boundary = matches!(
                n.kind(),
                "function_definition"
                    | "method_declaration"
                    | "anonymous_function"
                    | "arrow_function"
                    | "class_declaration"
                    | "compound_statement"
            ) || (stop_at_statement && n.kind().ends_with("_statement"));
            if boundary {
                return None;
            }
        }
        None
    }

    /// Source text of `node`.
    pub fn text(&self, node: Node) -> &'t str {
        text(node, self.src)
    }
}

/// An intention result replacing one range with `text`.
pub(crate) fn rewrite(action: Action, start: usize, end: usize, text: impl Into<String>) -> Fix {
    Fix {
        title: action.title().into(),
        edits: vec![Edit {
            path: None,
            start,
            end,
            text: text.into(),
        }],
        kind: FixKind::Intention(action),
    }
}

/// An intention result applying several edits.
pub(crate) fn rewrite_all(action: Action, edits: Vec<Edit>) -> Fix {
    Fix {
        title: action.title().into(),
        edits,
        kind: FixKind::Intention(action),
    }
}

/// An intention result replacing `node`.
pub(crate) fn replace(action: Action, node: Node, text: impl Into<String>) -> Fix {
    rewrite(action, node.start_byte(), node.end_byte(), text)
}

/// Leading whitespace of the line containing `byte`.
pub(crate) fn indent_at(src: &str, byte: usize) -> &str {
    let line_start = src[..byte].rfind('\n').map_or(0, |i| i + 1);
    let line = &src[line_start..];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// One indentation step, matching the file (a tab when lines start with
/// tabs, four spaces otherwise).
pub(crate) fn indent_unit(src: &str) -> &'static str {
    if src.lines().any(|l| l.starts_with('\t')) {
        "\t"
    } else {
        "    "
    }
}

/// Context actions available at `start` (with an optional selection up to
/// `end`) in `src`, in menu order.
///
/// Called by the IDE's `⌥↵` menu and the language server's code actions.
pub fn intentions(index: &Index, src: &str, start: usize, end: usize) -> Vec<Fix> {
    let tree = parse(src);
    let ctx = Context {
        index,
        src,
        tree: &tree,
        root: tree.root_node(),
        at: start.min(end),
    };
    let mut out = Vec::new();
    expressions::collect(&ctx, &mut out);
    control::collect(&ctx, &mut out);
    classes::collect(&ctx, &mut out);
    docs::collect(&ctx, &mut out);
    out
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// Applies the intention `action` at the caret marker in `marked`
    /// (`‸`, or the first `|` when there is none), removed first; `None`
    /// when the intention is not offered.
    pub fn apply(marked: &str, action: Action) -> Option<String> {
        apply_with(&Index::default(), marked, action)
    }

    /// Like [`apply`], with an index.
    pub fn apply_with(index: &Index, marked: &str, action: Action) -> Option<String> {
        let marker = if marked.contains('‸') { "‸" } else { "|" };
        let at = marked.find(marker).expect("marker");
        let src = marked.replacen(marker, "", 1);
        let fix = intentions(index, &src, at, at)
            .into_iter()
            .find(|f| f.kind == FixKind::Intention(action))?;
        Some(crate::refactor::apply_edits(&src, &fix.edits))
    }
}
