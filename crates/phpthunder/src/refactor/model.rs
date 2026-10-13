//! Refactoring results: parameter info and rename plans.

use crate::inspection::Edit;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Parameter info for the call around the caret (`⌘P` / typing `(` or `,`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureHelp {
    /// Declaration header of the callee, e.g. `public function find(int $id): ?User`.
    pub label: String,
    /// Each parameter as written in the declaration.
    pub params: Vec<String>,
    /// Index of the parameter the caret is in (clamped for variadics).
    pub active: usize,
}

/// Result of a rename: text edits per file, plus a file rename when a class
/// is renamed in a file named after it (PSR-4).
#[derive(Clone, Debug, Default)]
pub struct RenamePlan {
    /// Edits grouped by file, each sorted by offset.
    pub edits: BTreeMap<PathBuf, Vec<Edit>>,
    /// `(old, new)` path when the class file should be renamed too.
    pub file_rename: Option<(PathBuf, PathBuf)>,
}

/// A property declared in the class at the caret (declared or promoted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassProperty {
    /// Name without `$`.
    pub name: String,
    /// Declared type as written (`?string`, `array`), if any.
    pub ty: Option<String>,
    /// Declared `static`.
    pub is_static: bool,
    /// Declared `readonly` (no setter is generated).
    pub is_readonly: bool,
    /// Has a default value or is constructor-promoted, so a constructor
    /// need not set it.
    pub is_initialized: bool,
}

/// What code generation needs to know about the class around the caret.
/// Built from the buffer's syntax tree, so unsaved edits count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassOutline {
    /// FQN of the class.
    pub fqn: String,
    /// Byte offset of the closing `}` of the class body; members are
    /// inserted there.
    pub body_end: usize,
    /// Properties in declaration order.
    pub properties: Vec<ClassProperty>,
    /// Names of the methods the class declares itself, lowercased.
    pub methods: Vec<String>,
}

impl ClassOutline {
    /// Whether the class declares method `name` (case-insensitive, as PHP).
    pub fn declares(&self, name: &str) -> bool {
        self.methods.iter().any(|m| m.eq_ignore_ascii_case(name))
    }
}
