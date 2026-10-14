//! Inspection results: diagnostics, quick fixes and the text edits they apply.

use super::Rule;
use crate::syntax::Span;
use std::path::PathBuf;

/// How serious a [`Diagnostic`] is; decides squiggle color and ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Code that fails at runtime (undefined class or function).
    Error,
    /// Code that is suspicious but may work (unknown member, unused import).
    Warning,
    /// Style or simplification hints: shown subtly, never counted as
    /// problems in the status bar.
    Weak,
}

/// Machine-readable reason for a diagnostic, used to offer quick fixes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Code {
    /// A class name that resolves to nothing; carries the resolved FQN.
    UndefinedClass(String),
    /// A function call that resolves to nothing; carries the name as written.
    UndefinedFunction(String),
    /// A method missing on a fully known class hierarchy.
    UndefinedMethod {
        /// FQN of the class the method was looked up on.
        class: String,
        /// Method name as written.
        name: String,
        /// Whether it was called statically (`Foo::bar()`).
        is_static: bool,
    },
    /// A property missing on a fully known class hierarchy.
    UndefinedProperty {
        /// FQN of the class the property was looked up on.
        class: String,
        /// Property name without `$`.
        name: String,
    },
    /// A `use` import whose alias never appears in the file.
    UnusedImport(String),
    /// A variable read in a function but never assigned there.
    UndefinedVariable(String),
    /// A variable assigned in a function but never read there.
    UnusedVariable(String),
    /// A function with a declared return type whose end can be reached
    /// without returning; carries the declared type.
    MissingReturn(String),
    /// A returned value that cannot match the declared return type.
    ReturnTypeMismatch {
        /// Declared return type.
        expected: String,
        /// What is returned (`null`, `string`, `array`, a class, `nothing`).
        found: String,
    },
    /// A string naming a Laravel key (route, view, config key, disk, …)
    /// the project does not declare.
    UnknownLaravelKey {
        /// What the string names.
        kind: crate::laravel::KeyKind,
        /// The key as written.
        name: String,
    },
    /// `orderBy(…, 'desc')` that `orderByDesc(…)` says more simply.
    SimplifyOrderBy,
    /// A statement after `return`, `throw`, `break`, `continue` or `exit`.
    UnreachableCode,
    /// An array literal key written twice; carries the key.
    DuplicateArrayKey(String),
    /// An assignment used as an `if`/`while` condition.
    AssignmentInCondition,
    /// `== null` or `!= null`, which also match `0`, `''` and `[]`.
    LooseNullComparison,
    /// A `catch` block with nothing in it.
    EmptyCatch,
    /// A variable or property assigned to itself.
    SelfAssignment,
    /// A private method or property (`$name`) nothing in its class uses.
    UnusedPrivateMember(String),
    /// A control statement whose body is only `;`.
    EmptyBody,
    /// A `case` value repeated in one `switch`; carries the value.
    DuplicateCase(String),
    /// An `elseif` condition repeating an earlier branch.
    DuplicateCondition,
    /// A ternary whose two branches are the same.
    IdenticalBranches,
    /// A comparison or operation with the same operand on both sides.
    SameOperands,
    /// A closure `use` variable the closure never reads; carries `$name`.
    UnusedClosureUse(String),
    /// A method, property or constant declared twice; carries its name.
    DuplicateMember(String),
    /// An abstract method in a class not declared abstract.
    AbstractInConcreteClass,
    /// `$this` in a static method.
    ThisInStatic,
    /// An `else` after an `if` branch that always leaves.
    RedundantElse,
    /// A rule-based inspection (see [`Rule`]) with its message detail.
    Rule(Rule, String),
    /// Reported by an external analyser; carries its name (e.g. `PHPStan`).
    External(String),
}

/// One problem in a file. Byte offsets are for applying fixes; `span` is for
/// display (LSP positions).
#[derive(Clone, Debug)]
pub struct Diagnostic {
    /// Display range (line, UTF-16 column).
    pub span: Span,
    /// Start byte offset in the file.
    pub start: usize,
    /// End byte offset in the file.
    pub end: usize,
    /// Error or warning.
    pub severity: Severity,
    /// Human-readable message.
    pub message: String,
    /// Reason, for quick fixes.
    pub code: Code,
}

/// A text replacement, in the current file (`path: None`) or another one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    /// File to edit; `None` means the file the fix was computed for.
    pub path: Option<PathBuf>,
    /// Start byte offset of the replaced range.
    pub start: usize,
    /// End byte offset of the replaced range.
    pub end: usize,
    /// Replacement text.
    pub text: String,
}

/// A named set of edits offered for a diagnostic (`⌥↵` menu entry).
#[derive(Clone, Debug)]
pub struct Fix {
    /// Menu label, e.g. "Import class App\Models\User".
    pub title: String,
    /// Edits applied together as one undo step.
    pub edits: Vec<Edit>,
    /// What the fix does, with its data; lets front ends phrase the title
    /// in their own language.
    pub kind: FixKind,
}

/// What a [`Fix`] does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FixKind {
    /// Adds `use <FQN>;`.
    ImportClass(String),
    /// Deletes an unused `use`.
    RemoveImport,
    /// Adds an empty method to a class.
    AddMethod {
        /// Class FQN.
        class: String,
        /// Method name.
        name: String,
    },
    /// Keeps the assigned expression but drops the unused variable
    /// (`$x = f();` becomes `f();`).
    RemoveAssignment,
    /// Rewrites `orderBy(…, 'desc')` as `orderByDesc(…)`.
    UseOrderByDesc,
    /// Switches a Blade echo between `{{ }}` and `{!! !!}`.
    ToggleBladeEcho {
        /// Whether the result is the raw (unescaped) form.
        to_raw: bool,
    },
    /// Turns a validation rule string into a rule array.
    RulesToArray,
    /// Turns a rule array of plain strings into a rule string.
    RulesToString,
    /// Turns `==`/`!=` into `===`/`!==`.
    UseStrictComparison,
    /// Deletes the statement or declaration the diagnostic is on.
    RemoveStatement,
    /// Drops an `else` after an `if` that always leaves, keeping its body.
    RemoveRedundantElse,
    /// The quick fix of a rule-based inspection.
    Rule(Rule),
    /// Adds a property to a class.
    AddProperty {
        /// Class FQN.
        class: String,
        /// Property name without `$`.
        name: String,
    },
}
