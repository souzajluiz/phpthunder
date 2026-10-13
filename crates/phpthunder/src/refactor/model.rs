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
