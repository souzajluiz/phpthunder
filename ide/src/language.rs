//! In-process language services: the phpthunder index and analysis, without
//! a separate server process or JSON-RPC.

use phpthunder::analysis::{self, Candidate, Definition, Usage};
use phpthunder::index::{Index, index_source};
use phpthunder::stubs::Symbol;
use phpthunder::workspace;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub struct Language {
    index: Arc<RwLock<Index>>,
}

#[derive(Clone, Debug)]
pub struct IndexReport {
    pub files: usize,
    pub elapsed: Duration,
}

impl Language {
    /// Scans and indexes a project folder. Runs on a blocking thread.
    pub async fn index_project(self, root: PathBuf) -> IndexReport {
        tokio::task::spawn_blocking(move || {
            let started = Instant::now();
            *self.index.write().unwrap() = Index::default();
            let files = workspace::php_files(&root);
            workspace::index_files(&self.index, &files, |_, _| {});
            IndexReport {
                files: files.len(),
                elapsed: started.elapsed(),
            }
        })
        .await
        .expect("indexing thread panicked")
    }

    /// Re-indexes one file from its current (possibly unsaved) text.
    pub fn update_file(&self, path: &Path, text: &str) {
        let entry = index_source(text);
        self.index
            .write()
            .unwrap()
            .update(path.to_path_buf(), entry);
    }

    /// Visits every indexed symbol under one read lock.
    pub fn for_each_symbol(&self, mut f: impl FnMut(&Path, &Symbol)) {
        let index = self.index.read().unwrap();
        for (path, symbols) in index.files() {
            for s in symbols {
                f(path, s);
            }
        }
    }

    pub fn file_symbols(&self, path: &Path) -> Vec<Symbol> {
        self.index.read().unwrap().file_symbols(path).to_vec()
    }

    pub fn definitions(&self, text: &str, offset: usize) -> Vec<Definition> {
        analysis::definitions_at(&self.index.read().unwrap(), text, offset)
    }

    pub fn complete(&self, text: &str, offset: usize) -> Vec<Candidate> {
        analysis::complete(&self.index.read().unwrap(), text, offset)
    }

    /// Find usages; `open` holds the text of open editors, which wins over disk.
    pub async fn usages(
        self,
        text: String,
        offset: usize,
        open: HashMap<PathBuf, String>,
    ) -> Option<(Definition, Vec<Usage>)> {
        tokio::task::spawn_blocking(move || {
            let index = self.index.read().unwrap();
            let defs = analysis::definitions_at(&index, &text, offset);
            let first = defs.first()?.clone();
            let usages = analysis::find_references(&index, &defs, |p| {
                open.get(p)
                    .cloned()
                    .or_else(|| std::fs::read_to_string(p).ok())
            });
            Some((first, usages))
        })
        .await
        .ok()
        .flatten()
    }
}
