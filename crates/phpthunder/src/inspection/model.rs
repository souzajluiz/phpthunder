//! Inspection results: diagnostics, quick fixes and the text edits they apply.

use crate::syntax::Span;
use std::path::PathBuf;

/// How serious a [`Diagnostic`] is; decides squiggle color and ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Code that fails at runtime (undefined class or function).
    Error,
    /// Code that is suspicious but may work (unknown member, unused import).
    Warning,
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
    UnusedImport,
    /// A variable read in a function but never assigned there.
    UndefinedVariable(String),
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
}
