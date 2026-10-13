//! In-process language services: the phpthunder index and analysis, without
//! a separate server process or JSON-RPC.

use parking_lot::RwLock;
use phpthunder::analysis::{self, Candidate, Definition, Usage};
use phpthunder::diagnostics::{self, Diagnostic, Fix};
use phpthunder::index::{Index, index_source};
use phpthunder::refactor::{self, RenamePlan, SignatureHelp};
use phpthunder::stubs::Symbol;
use phpthunder::{builtins, workspace};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Handle to the shared symbol index; cheap to clone into background tasks.
///
/// Owned by the app and used for completion, navigation, palettes,
/// inspections and refactorings.
#[derive(Clone, Default)]
pub struct Language {
    /// Project index, written by indexing and edits and read by every query.
    index: Arc<RwLock<Index>>,
}

/// Summary of a finished project indexing run, shown in the status bar.
#[derive(Clone, Debug)]
pub struct IndexReport {
    /// Number of PHP files indexed (project and vendor, not built-ins).
    pub files: usize,
    /// Wall-clock time indexing took.
    pub elapsed: Duration,
}

/// Reads a file for analysis, preferring the unsaved editor buffer in `open`.
fn load_with(open: &HashMap<PathBuf, String>, path: &Path) -> Option<String> {
    open.get(path)
        .cloned()
        .or_else(|| std::fs::read_to_string(path).ok())
}

impl Language {
    /// Scans and indexes a project folder plus the PHP built-in stubs, on a
    /// blocking thread. Called when a folder is opened.
    ///
    /// # Errors
    /// Returns a message if the indexing thread panicked.
    pub async fn index_project(self, root: PathBuf) -> Result<IndexReport, String> {
        tokio::task::spawn_blocking(move || {
            let started = Instant::now();
            *self.index.write() = Index::default();
            // Built-ins make "undefined function/class" inspections trustworthy
            // and let go-to-declaration open `strlen`; a write failure only
            // costs those features.
            if let Ok(path) = builtins::ensure_written() {
                self.update_file(&path, builtins::SOURCE);
            }
            let files = workspace::php_files(&root);
            workspace::index_files(&self.index, &files, |_, _| {});
            IndexReport {
                files: files.len(),
                elapsed: started.elapsed(),
            }
        })
        .await
        .map_err(|e| format!("indexing failed: {e}"))
    }

    /// Re-indexes one file from its current (possibly unsaved) text. Called
    /// after every edit so completion and inspections see the latest code.
    pub fn update_file(&self, path: &Path, text: &str) {
        let entry = index_source(text);
        self.index.write().update(path.to_path_buf(), entry);
    }

    /// Visits every indexed symbol under one read lock (palettes, structure).
    pub fn for_each_symbol(&self, mut f: impl FnMut(&Path, &Symbol)) {
        let index = self.index.read();
        for (path, symbols) in index.files() {
            for s in symbols {
                f(path, s);
            }
        }
    }

    /// Indexed symbols declared in `path`, for the breadcrumb and the file
    /// structure popup. Clones so the read lock is released before returning.
    pub fn file_symbols(&self, path: &Path) -> Vec<Symbol> {
        self.index.read().file_symbols(path).to_vec()
    }

    /// Declarations of the symbol at byte `offset` in `text`, for
    /// go-to-declaration and hover. Empty when nothing resolves.
    pub fn definitions(&self, text: &str, offset: usize) -> Vec<Definition> {
        analysis::definitions_at(&self.index.read(), text, offset)
    }

    /// Completion candidates at byte `offset` in `text`, for the completion popup.
    pub fn complete(&self, text: &str, offset: usize) -> Vec<Candidate> {
        analysis::complete(&self.index.read(), text, offset)
    }

    /// Inspections for one buffer, run after edits settle.
    pub fn diagnostics(&self, text: &str) -> Vec<Diagnostic> {
        diagnostics::diagnostics(&self.index.read(), text)
    }

    /// Quick fixes for `diagnostic` (computed on the same `text`), for the
    /// `⌥↵` menu. `open` supplies unsaved buffers of other files.
    pub fn quick_fixes(
        &self,
        text: &str,
        diagnostic: &Diagnostic,
        open: &HashMap<PathBuf, String>,
    ) -> Vec<Fix> {
        diagnostics::quick_fixes(&self.index.read(), text, diagnostic, |p| load_with(open, p))
    }

    /// Parameter info for the call around `offset`, shown above the caret.
    pub fn signature_help(&self, text: &str, offset: usize) -> Option<SignatureHelp> {
        refactor::signature_help(&self.index.read(), text, offset)
    }

    /// Plans a rename of the symbol at `offset`, reading other files from
    /// `open` or disk. Runs on a blocking thread since it scans the project.
    ///
    /// # Errors
    /// Returns the reason the symbol cannot be renamed.
    pub async fn rename(
        self,
        path: PathBuf,
        text: String,
        offset: usize,
        new_name: String,
        open: HashMap<PathBuf, String>,
    ) -> Result<RenamePlan, String> {
        tokio::task::spawn_blocking(move || {
            refactor::rename(&self.index.read(), &path, &text, offset, &new_name, |p| {
                load_with(&open, p)
            })
        })
        .await
        .map_err(|e| format!("rename failed: {e}"))?
    }

    /// `text` with unused imports removed and import blocks sorted.
    pub fn optimize_imports(&self, text: &str) -> String {
        refactor::optimize_imports(&self.index.read(), text)
    }

    /// Abstract/interface methods `class` still has to implement.
    pub fn missing_implementations(&self, class: &str) -> Vec<Symbol> {
        refactor::missing_implementations(&self.index.read(), class)
    }

    /// Classes extending or implementing `class`, for go to implementation
    /// and the type hierarchy.
    pub fn subtypes(&self, class: &str) -> Vec<(PathBuf, Symbol)> {
        refactor::subtypes(&self.index.read(), class)
    }

    /// Overrides of `method` declared on subtypes of `class`.
    pub fn overriding_methods(&self, class: &str, method: &str) -> Vec<(PathBuf, Symbol)> {
        refactor::overriding_methods(&self.index.read(), class, method)
    }

    /// `class` and its supertypes, nearest first, each with its declaration
    /// when indexed. Used by the type hierarchy panel.
    pub fn supertypes(&self, class: &str) -> Vec<(String, Option<(PathBuf, Symbol)>)> {
        let index = self.index.read();
        index
            .hierarchy(class)
            .into_iter()
            .map(|fqn| {
                let decl = index
                    .classes(&fqn)
                    .into_iter()
                    .next()
                    .map(|(p, s)| (p.to_path_buf(), s.clone()));
                (fqn, decl)
            })
            .collect()
    }

    /// Find usages; `open` holds the text of open editors, which wins over disk.
    pub async fn usages(
        self,
        text: String,
        offset: usize,
        open: HashMap<PathBuf, String>,
    ) -> Option<(Definition, Vec<Usage>)> {
        tokio::task::spawn_blocking(move || {
            let index = self.index.read();
            let defs = analysis::definitions_at(&index, &text, offset);
            let first = defs.first()?.clone();
            let usages = analysis::find_references(&index, &defs, |p| load_with(&open, p));
            Some((first, usages))
        })
        .await
        .ok()
        .flatten()
    }
}
