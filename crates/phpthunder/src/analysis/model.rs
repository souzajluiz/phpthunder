//! Analysis results: reference targets, declarations, usages, completion candidates.

use crate::symbol::{Kind, MemberTag, Symbol};
use crate::syntax::Span;
use std::path::{Path, PathBuf};

/// What an identifier refers to, before it is looked up in the index.
///
/// Produced by [`Analyzer::target_of`] from the identifier's syntax position and
/// consumed by [`resolve`]. Names are already fully qualified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A class, interface, trait, or enum, by fully qualified name.
    Class(String),
    /// Candidate FQNs in lookup order.
    Function(Vec<String>),
    /// A method, property, or constant reached through `class`.
    Member {
        /// FQN of the class the member is accessed on; lookup also walks its supertypes.
        class: String,
        /// Which member namespace to search (methods, properties, or constants).
        tag: MemberTag,
        /// Member name as written (properties without `$`).
        name: String,
    },
}

/// A resolved declaration, owned so it can outlive the index lock.
#[derive(Clone, Debug)]
pub struct Definition {
    /// File that declares the symbol.
    pub path: PathBuf,
    /// Declaration stub, cloned out of the index.
    pub symbol: Symbol,
}

impl Definition {
    /// Key `(file, name start byte)` that identifies a declaration.
    ///
    /// [`find_references`] compares these instead of whole symbols to decide whether
    /// an occurrence resolves to one of the searched declarations.
    pub(crate) fn identity(&self) -> (&Path, usize) {
        (&self.path, self.symbol.name_start)
    }
}

/// One occurrence reported by [`find_references`].
#[derive(Clone, Debug)]
pub struct Usage {
    /// File containing the occurrence.
    pub path: PathBuf,
    /// Range of the identifier, in LSP coordinates.
    pub span: Span,
    /// True when the occurrence is the declaration itself (for LSP `includeDeclaration`).
    pub is_declaration: bool,
}

/// A completion proposal, independent of LSP types; the server turns it into a `CompletionItem`.
#[derive(Clone, Debug)]
pub struct Candidate {
    /// Text shown in the list and used for filtering.
    pub label: String,
    /// Declaration kind; local variables use [`Kind::Property`] with detail `variable`.
    pub kind: Kind,
    /// Signature, FQN, or a short description shown next to the label.
    pub detail: String,
    /// Text that replaces the typed prefix; methods and functions end with `(`.
    pub insert_text: String,
    /// `use` statement to add, as (byte offset, text).
    pub import: Option<(usize, String)>,
}
