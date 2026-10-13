//! Project file tree, loaded lazily one directory at a time.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Directory and file names hidden from the tree and from project file lists.
const HIDDEN: &[&str] = &[
    ".git",
    ".idea",
    ".vscode",
    ".DS_Store",
    "node_modules",
    "target",
    ".codegraph",
];

/// A file or directory in the project tree.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Absolute path on disk.
    pub path: PathBuf,
    /// File name shown in the tree.
    pub name: String,
    /// Whether the entry is a directory (expandable).
    pub is_dir: bool,
}

/// A visible row of the tree.
pub struct Row<'a> {
    /// The file or directory shown on this row.
    pub entry: &'a Entry,
    /// Nesting level below the root, used for indentation.
    pub depth: usize,
    /// Whether this directory is currently expanded.
    pub expanded: bool,
}

/// Sidebar file tree; directories are read only when first expanded.
#[derive(Default)]
pub struct FileTree {
    /// Project root, or `None` before a folder is opened.
    pub root: Option<PathBuf>,
    /// Cached directory listings, keyed by directory path.
    children: HashMap<PathBuf, Vec<Entry>>,
    /// Directories currently shown expanded.
    expanded: HashSet<PathBuf>,
}

/// Lists `dir`, skipping `HIDDEN` names; directories first, then case-insensitive by name.
///
/// Unreadable directories yield an empty list.
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
            Some(Entry {
                path: e.path(),
                name,
                is_dir,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

impl FileTree {
    /// Builds the tree for a newly opened project folder.
    ///
    /// Reads the root eagerly and pre-expands `src/` and `app/` when present.
    pub fn open(root: PathBuf) -> Self {
        let mut tree = Self {
            root: Some(root.clone()),
            ..Default::default()
        };
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

    /// Reads `dir` from disk (refreshing any cached listing) and marks it expanded.
    fn expand(&mut self, dir: &Path) {
        self.children.insert(dir.to_path_buf(), read_dir(dir));
        self.expanded.insert(dir.to_path_buf());
    }

    /// Rereads `dir` if it is loaded, after files in it were created or
    /// renamed (e.g. by a class rename).
    pub fn refresh(&mut self, dir: &Path) {
        if self.children.contains_key(dir) {
            self.children.insert(dir.to_path_buf(), read_dir(dir));
        }
    }

    /// Collapses `dir` if expanded, otherwise expands it, re-reading it from disk.
    ///
    /// Called when a directory row is clicked in the sidebar.
    pub fn toggle(&mut self, dir: &Path) {
        if self.expanded.contains(dir) {
            self.expanded.remove(dir);
        } else {
            self.expand(dir);
        }
    }

    /// Expands every ancestor of `path` so it becomes visible.
    pub fn reveal(&mut self, path: &Path) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let mut dirs: Vec<PathBuf> = path
            .ancestors()
            .skip(1)
            .take_while(|a| a.starts_with(&root))
            .map(Path::to_path_buf)
            .collect();
        dirs.reverse();
        for dir in dirs {
            if !self.expanded.contains(&dir) {
                self.expand(&dir);
            }
        }
    }

    /// Flattens the expanded part of the tree into visible rows, in display order.
    ///
    /// Called each time the sidebar is rendered.
    pub fn rows(&self) -> Vec<Row<'_>> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            self.collect(root, 0, &mut out);
        }
        out
    }

    /// Appends `dir`'s children to `out`, recursing into expanded subdirectories.
    fn collect<'a>(&'a self, dir: &Path, depth: usize, out: &mut Vec<Row<'a>>) {
        for entry in self.children.get(dir).into_iter().flatten() {
            let expanded = entry.is_dir && self.expanded.contains(&entry.path);
            out.push(Row {
                entry,
                depth,
                expanded,
            });
            if expanded {
                self.collect(&entry.path, depth + 1, out);
            }
        }
    }
}

/// Current git branch, read from `.git/HEAD`.
pub fn git_branch(root: &Path) -> Option<String> {
    let head = std::fs::read_to_string(root.join(".git/HEAD")).ok()?;
    head.trim()
        .strip_prefix("ref: refs/heads/")
        .map(str::to_string)
}

/// Every file under `root`, skipping hidden tool directories.
pub fn list_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
        {
            let name = entry.file_name();
            if HIDDEN.contains(&name.to_string_lossy().as_ref()) {
                continue;
            }
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(entry.path()),
                Ok(t) if t.is_file() => out.push(entry.path()),
                _ => {}
            }
        }
    }
    out.sort();
    out
}
