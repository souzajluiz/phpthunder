//! Project file tree, loaded lazily one directory at a time.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const HIDDEN: &[&str] = &[".git", ".idea", ".vscode", ".DS_Store", "node_modules", "target", ".codegraph"];

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
}

/// A visible row of the tree.
pub struct Row<'a> {
    pub entry: &'a Entry,
    pub depth: usize,
    pub expanded: bool,
}

#[derive(Default)]
pub struct FileTree {
    pub root: Option<PathBuf>,
    children: HashMap<PathBuf, Vec<Entry>>,
    expanded: HashSet<PathBuf>,
}

fn read_dir(dir: &Path) -> Vec<Entry> {
    let mut entries: Vec<Entry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if HIDDEN.contains(&name.as_str()) {
                return None;
            }
            let is_dir = e.file_type().ok()?.is_dir();
            Some(Entry { path: e.path(), name, is_dir })
        })
        .collect();
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    entries
}

impl FileTree {
    pub fn open(root: PathBuf) -> Self {
        let mut tree = Self { root: Some(root.clone()), ..Default::default() };
        tree.expand(&root);
        // Common source folders start expanded, like a fresh checkout in most IDEs.
        for name in ["src", "app"] {
            let dir = root.join(name);
            if dir.is_dir() {
                tree.expand(&dir);
            }
        }
        tree
    }

    fn expand(&mut self, dir: &Path) {
        self.children.insert(dir.to_path_buf(), read_dir(dir));
        self.expanded.insert(dir.to_path_buf());
    }

    pub fn toggle(&mut self, dir: &Path) {
        if self.expanded.contains(dir) {
            self.expanded.remove(dir);
        } else {
            self.expand(dir);
        }
    }

    /// Expands every ancestor of `path` so it becomes visible.
    pub fn reveal(&mut self, path: &Path) {
        let Some(root) = self.root.clone() else { return };
        let mut dirs: Vec<PathBuf> = path.ancestors().skip(1).take_while(|a| a.starts_with(&root)).map(Path::to_path_buf).collect();
        dirs.reverse();
        for dir in dirs {
            if !self.expanded.contains(&dir) {
                self.expand(&dir);
            }
        }
    }

    pub fn rows(&self) -> Vec<Row<'_>> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            self.collect(root, 0, &mut out);
        }
        out
    }

    fn collect<'a>(&'a self, dir: &Path, depth: usize, out: &mut Vec<Row<'a>>) {
        for entry in self.children.get(dir).into_iter().flatten() {
            let expanded = entry.is_dir && self.expanded.contains(&entry.path);
            out.push(Row { entry, depth, expanded });
            if expanded {
                self.collect(&entry.path, depth + 1, out);
            }
        }
    }
}

/// Current git branch, read from `.git/HEAD`.
pub fn git_branch(root: &Path) -> Option<String> {
    let head = std::fs::read_to_string(root.join(".git/HEAD")).ok()?;
    head.trim().strip_prefix("ref: refs/heads/").map(str::to_string)
}
