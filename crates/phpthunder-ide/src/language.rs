//! In-process language services: the phpthunder index and analysis, without
//! a separate server process or JSON-RPC.

use crate::tr;
use parking_lot::RwLock;
use phpthunder::analysis::{self, Candidate, Definition, Usage};
use phpthunder::index::{self, Index, index_source};
use phpthunder::inspection::{self, Diagnostic, Fix};
use phpthunder::refactor::{self, RenamePlan, SignatureHelp};
use phpthunder::symbol::Symbol;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Cache entries applied per write lock: small enough that queries from
/// the UI thread wait milliseconds, not the whole cache load.
const CACHE_BATCH: usize = 512;

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

/// Stage of a running project index, shown with its count in the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexPhase {
    /// Walking the project and reading the cache file.
    Starting,
    /// Applying cached entries from the last run.
    Cache,
    /// Parsing files that changed since the cache was written.
    Scan,
}

/// Live progress of [`Language::index_project`], written by the indexing
/// thread and read by the status bar on every redraw; atomics, so neither
/// side ever waits for the other.
#[derive(Debug, Default)]
pub struct IndexProgress {
    /// [`IndexPhase`] as a number (0 starting, 1 cache, 2 scan).
    phase: AtomicU8,
    /// Items done in the current phase.
    done: AtomicUsize,
    /// Items in the current phase.
    total: AtomicUsize,
}

impl IndexProgress {
    /// Enters `phase` with `total` items, none done yet.
    fn begin(&self, phase: IndexPhase, total: usize) {
        self.done.store(0, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
        self.phase.store(phase as u8, Ordering::Relaxed);
    }

    /// Records `done` items finished in the current phase.
    fn advance(&self, done: usize) {
        self.done.store(done, Ordering::Relaxed);
    }

    /// Current phase, items done and total, for the status bar.
    pub fn snapshot(&self) -> (IndexPhase, usize, usize) {
        let phase = match self.phase.load(Ordering::Relaxed) {
            1 => IndexPhase::Cache,
            2 => IndexPhase::Scan,
            _ => IndexPhase::Starting,
        };
        (
            phase,
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }
}

/// Reads a file for analysis, preferring the unsaved editor buffer in `open`.
fn load_with(open: &HashMap<PathBuf, String>, path: &Path) -> Option<String> {
    open.get(path)
        .cloned()
        .or_else(|| std::fs::read_to_string(path).ok())
}

impl Language {
    /// Scans and indexes a project folder plus the PHP built-in stubs, on a
    /// blocking thread, reporting into `progress` as it goes. Called when a
    /// folder is opened. The index lock is only held for short batches, so
    /// the UI keeps answering while a large project loads.
    ///
    /// # Errors
    /// Returns a message if the indexing thread panicked.
    pub async fn index_project(
        self,
        root: PathBuf,
        progress: Arc<IndexProgress>,
    ) -> Result<IndexReport, String> {
        tokio::task::spawn_blocking(move || {
            let started = Instant::now();
            *self.index.write() = Index::default();
            // Built-ins make "undefined function/class" inspections trustworthy
            // and let go-to-declaration open `strlen`; a write failure only
            // costs those features.
            if let Ok(path) = index::write_builtins() {
                self.update_file(&path, index::BUILTINS_SOURCE);
            }
            // The cache fills the index first; the scan then reparses only
            // files whose content changed since it was written. The project
            // walk is independent of the cache, so it runs alongside.
            let cache = crate::project::index_cache_path(&root);
            let mut unchanged = std::collections::HashSet::new();
            let files = std::thread::scope(|scope| {
                let walk = scope.spawn(|| index::php_files(&root));
                if let Some(entries) = cache.as_deref().and_then(index::load_cache) {
                    progress.begin(IndexPhase::Cache, entries.len());
                    let mut entries = entries.into_iter();
                    let mut done = 0;
                    loop {
                        let batch: Vec<_> = entries.by_ref().take(CACHE_BATCH).collect();
                        if batch.is_empty() {
                            break;
                        }
                        done += batch.len();
                        let mut idx = self.index.write();
                        for (path, stamp, entry) in batch {
                            if stamp.is_some() && stamp == index::file_stamp(&path) {
                                unchanged.insert(path.clone());
                            }
                            idx.update(path, entry);
                        }
                        drop(idx);
                        progress.advance(done);
                    }
                }
                walk.join().unwrap_or_default()
            });
            let changed: Vec<PathBuf> = files
                .iter()
                .filter(|p| !unchanged.contains(*p))
                .cloned()
                .collect();
            progress.begin(IndexPhase::Scan, changed.len());
            index::index_files(&self.index, &changed, |done, _| progress.advance(done));
            let mut removed = 0;
            let indexed = {
                // Files deleted since the cache was written.
                let keep: std::collections::HashSet<&PathBuf> = files.iter().collect();
                let builtins = index::write_builtins().ok();
                let mut idx = self.index.write();
                let paths = idx.paths();
                for path in &paths {
                    if !keep.contains(path) && Some(path) != builtins.as_ref() {
                        idx.remove(path);
                        removed += 1;
                    }
                }
                // Changed files that made it into the index: unreadable ones
                // (binary or non-UTF-8 data shipped as `.php`) never do, and
                // must not force a cache rewrite on every start.
                let present: std::collections::HashSet<&PathBuf> = paths.iter().collect();
                changed.iter().filter(|p| present.contains(p)).count()
            };
            // An untouched project keeps its cache: rewriting it costs about
            // as much as loading it.
            if let Some(cache) = cache
                && (indexed > 0 || removed > 0)
            {
                // A write failure only costs the next start its head start.
                let _ = index::save_cache(&self.index.read(), &cache);
            }
            IndexReport {
                files: files.len(),
                elapsed: started.elapsed(),
            }
        })
        .await
        .map_err(|e| tr!("error-indexing", error = e.to_string()))
    }

    /// Re-indexes one file from its current (possibly unsaved) text. Called
    /// after every edit so completion and inspections see the latest code.
    pub fn update_file(&self, path: &Path, text: &str) {
        let entry = index_source(text);
        self.index.write().update(path.to_path_buf(), entry);
    }

    /// Drops `path` from the index (after a file rename).
    pub fn remove_file(&self, path: &Path) {
        self.index.write().remove(path);
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

    /// Columns or relations for the Eloquent string at `offset`
    /// (`where('…')`, `with('…')`), with where the replaced text starts.
    pub fn eloquent_completions(
        &self,
        text: &str,
        offset: usize,
    ) -> Option<(usize, Vec<Candidate>)> {
        phpthunder::laravel::eloquent_completions(&self.index.read(), text, offset)
    }

    /// Laravel gutter markers for a file at `rel` (path from the project
    /// root): route actions, runnable migrations, seeders and commands.
    pub fn gutter_markers(&self, text: &str, rel: &str) -> Vec<phpthunder::laravel::GutterMarker> {
        phpthunder::laravel::gutter_markers(&self.index.read(), text, rel)
    }

    /// Whether `offset` is inside an Eloquent column or relation string.
    pub fn in_eloquent_string(&self, text: &str, offset: usize) -> bool {
        phpthunder::laravel::in_eloquent_string(&self.index.read(), text, offset)
    }

    /// Declaration of the column (migration) or relation (method) named by
    /// the Eloquent string at `offset`.
    pub fn eloquent_definition(
        &self,
        text: &str,
        offset: usize,
    ) -> Option<(std::path::PathBuf, phpthunder::syntax::Pos)> {
        phpthunder::laravel::eloquent_definition(&self.index.read(), text, offset)
    }

    /// Inspections for one buffer, run after edits settle.
    pub fn diagnostics(&self, text: &str) -> Vec<Diagnostic> {
        inspection::diagnostics(&self.index.read(), text)
    }

    /// Quick fixes for `diagnostic` (computed on the same `text`), for the
    /// `⌥↵` menu. `open` supplies unsaved buffers of other files.
    pub fn quick_fixes(
        &self,
        text: &str,
        diagnostic: &Diagnostic,
        open: &HashMap<PathBuf, String>,
    ) -> Vec<Fix> {
        inspection::quick_fixes(&self.index.read(), text, diagnostic, |p| load_with(open, p))
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
        .map_err(|e| tr!("error-rename", error = e.to_string()))?
    }

    /// Plans a parameter change of the function or method at `offset`,
    /// rewriting every call; runs on a blocking thread.
    ///
    /// # Errors
    /// Returns why the signature cannot be changed.
    pub async fn change_signature(
        self,
        text: String,
        offset: usize,
        params: Vec<refactor::ParamChange>,
        open: HashMap<PathBuf, String>,
    ) -> Result<RenamePlan, String> {
        tokio::task::spawn_blocking(move || {
            refactor::change_signature(&self.index.read(), &text, offset, &params, |p| {
                load_with(&open, p)
            })
        })
        .await
        .map_err(|e| tr!("error-rename", error = e.to_string()))?
    }

    /// Plans moving the class in `path` to `namespace` (and `new_path`);
    /// runs on a blocking thread.
    ///
    /// # Errors
    /// Returns why the class cannot be moved.
    pub async fn move_class(
        self,
        path: PathBuf,
        text: String,
        namespace: String,
        new_path: Option<PathBuf>,
        open: HashMap<PathBuf, String>,
    ) -> Result<RenamePlan, String> {
        tokio::task::spawn_blocking(move || {
            refactor::move_class(
                &self.index.read(),
                &path,
                &text,
                &namespace,
                new_path,
                |p| load_with(&open, p),
            )
        })
        .await
        .map_err(|e| tr!("error-rename", error = e.to_string()))?
    }

    /// `text` with unused imports removed and import blocks sorted.
    pub fn optimize_imports(&self, text: &str) -> String {
        refactor::optimize_imports(&self.index.read(), text)
    }

    /// Abstract/interface methods `class` still has to implement.
    pub fn missing_implementations(&self, class: &str) -> Vec<Symbol> {
        refactor::missing_implementations(&self.index.read(), class)
    }

    /// Parent methods the class in `outline` can override (`⌘N`).
    pub fn overridable_methods(&self, outline: &refactor::ClassOutline) -> Vec<Symbol> {
        refactor::overridable_methods(&self.index.read(), outline)
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

    /// The member `name` of kind `tag` that `class`'s nearest supertype
    /// declares (not `class` itself): what a method overrides.
    pub fn super_member(
        &self,
        class: &str,
        tag: phpthunder::symbol::MemberTag,
        name: &str,
    ) -> Option<(PathBuf, Symbol)> {
        let index = self.index.read();
        index.hierarchy(class).iter().skip(1).find_map(|cls| {
            index
                .lookup(&phpthunder::index::member_key(cls, tag, name))
                .into_iter()
                .next()
                .map(|(p, s)| (p.to_path_buf(), s.clone()))
        })
    }

    /// Where class-like `fqn` is declared, if indexed.
    pub fn class_declaration(&self, fqn: &str) -> Option<(PathBuf, Symbol)> {
        let index = self.index.read();
        index
            .classes(fqn)
            .into_iter()
            .next()
            .map(|(p, s)| (p.to_path_buf(), s.clone()))
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
