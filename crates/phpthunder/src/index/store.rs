//! Project-wide symbol index.
//!
//! Two indexes: a declaration index (key -> declarations, rebuilt per file from
//! stubs) and a word index (identifier -> files containing it), used to narrow
//! find-usages to candidate files before any parsing happens. Each file is
//! indexed independently and keyed by a content hash, so unchanged files are
//! never reparsed.

use super::*;
use crate::symbol::{Kind, MemberTag, Symbol};
use crate::syntax::LineIndex;
use crate::syntax::parse;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Hash of a file's content, compared by [`Index::is_current`] so the initial
/// scan does not reparse unchanged files. Only stable within one process; never
/// persist it.
pub fn content_hash(src: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    h.finish()
}

/// Parses one file and computes everything the index stores for it.
/// Pure function of the content, so it runs in parallel off the index lock.
pub fn index_source(src: &str) -> FileEntry {
    let tree = parse(src);
    let lines = LineIndex::new(src);
    FileEntry {
        hash: content_hash(src),
        symbols: crate::symbol::build(tree.root_node(), src, &lines),
        words: words(src),
    }
}

/// Lowercased identifiers in `src` (the word index entries for one file).
pub fn words(src: &str) -> HashSet<String> {
    let bytes = src.as_bytes();
    let is_ident = |b: u8| b == b'_' || b.is_ascii_alphanumeric() || b >= 0x80;
    let mut out = HashSet::new();
    let mut i = 0;
    while i < bytes.len() {
        if is_ident(bytes[i]) {
            let start = i;
            while i < bytes.len() && is_ident(bytes[i]) {
                i += 1;
            }
            if !bytes[start].is_ascii_digit() {
                out.insert(src[start..i].to_lowercase());
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Declaration-index key for member `name` of class `owner`.
///
/// The tag prefix keeps methods, properties, and constants apart. The owner and
/// method names are case-insensitive, as in PHP; property and constant names
/// stay case-sensitive.
pub fn member_key(owner: &str, tag: MemberTag, name: &str) -> String {
    match tag {
        // Method names are case-insensitive in PHP; properties and constants are not.
        MemberTag::Method => format!(
            "m:{}::{}",
            owner.to_ascii_lowercase(),
            name.to_ascii_lowercase()
        ),
        MemberTag::Property => format!("p:{}::{}", owner.to_ascii_lowercase(), name),
        MemberTag::Constant => format!("k:{}::{}", owner.to_ascii_lowercase(), name),
    }
}

/// Declaration-index key for a class-like; case-insensitive, as PHP class names are.
pub fn class_key(fqn: &str) -> String {
    format!("c:{}", fqn.to_ascii_lowercase())
}

/// Declaration-index key for a function; case-insensitive, as PHP function names are.
pub fn function_key(fqn: &str) -> String {
    format!("f:{}", fqn.to_ascii_lowercase())
}

/// Key a stub is stored under; must agree with the keys the lookups build
/// ([`class_key`], [`function_key`], [`member_key`]). Global constants are
/// keyed case-sensitively.
fn symbol_key(sym: &Symbol) -> String {
    match sym.kind {
        Kind::Class | Kind::Interface | Kind::Trait | Kind::Enum => class_key(&sym.fqn),
        Kind::Function => function_key(&sym.fqn),
        Kind::Constant => format!("g:{}", sym.fqn),
        Kind::Method => member_key(&sym.fqn, MemberTag::Method, &sym.name),
        Kind::Property => member_key(&sym.fqn, MemberTag::Property, &sym.name),
        Kind::ClassConstant | Kind::EnumCase => {
            member_key(&sym.fqn, MemberTag::Constant, &sym.name)
        }
    }
}

/// Lowercased last FQN segment of a top-level symbol; key of the name index used by completion and workspace symbols.
fn short_name(sym: &Symbol) -> String {
    sym.fqn
        .rsplit('\\')
        .next()
        .unwrap_or(&sym.fqn)
        .to_ascii_lowercase()
}

/// Project-wide declaration and word index.
///
/// Per-file entries are the source of truth; the lookup maps are derived from
/// them and kept in sync by [`Index::update`] and [`Index::remove`]. Holds no
/// lock itself: the server and [`crate::index::index_files`] wrap it in a
/// `RwLock`.
#[derive(Default)]
pub struct Index {
    /// Indexed files and their entries.
    files: HashMap<PathBuf, FileEntry>,
    /// Declaration index: [`symbol_key`] -> declarations stored under it.
    keys: HashMap<String, Vec<SymRef>>,
    /// Lowercase short name of top-level symbols, ordered for prefix queries.
    names: BTreeMap<String, Vec<SymRef>>,
    /// Word index: lowercased identifier -> files that contain it.
    words: HashMap<String, HashSet<PathBuf>>,
    /// Lazily built set of every namespace declaring a class (lowercased,
    /// with all parent namespaces); cleared whenever a file changes.
    namespaces: parking_lot::Mutex<Option<std::sync::Arc<HashSet<String>>>>,
    /// Lowercased owner -> files declaring members of a class they do not
    /// declare themselves (migration columns of a table, see
    /// [`crate::symbol::TABLE_PREFIX`]); [`Index::members`] reads them too.
    augments: HashMap<String, HashSet<PathBuf>>,
    /// Lazily built set of lowercased FQNs that some indexed class extends
    /// or implements; cleared whenever a file changes.
    extended: parking_lot::Mutex<Option<std::sync::Arc<HashSet<String>>>>,
}

impl Index {
    /// Number of indexed files.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// True when `path` is indexed with content hash `hash`, so it need not be reparsed.
    pub fn is_current(&self, path: &Path, hash: u64) -> bool {
        self.files.get(path).is_some_and(|e| e.hash == hash)
    }

    /// Installs `entry` as the content of `path`.
    ///
    /// Drops the previous entry first, so stale declarations and words disappear.
    /// Called on every open, edit, close, and watched-file change, and in batches
    /// by the initial scan.
    pub fn update(&mut self, path: PathBuf, entry: FileEntry) {
        self.remove(&path);
        *self.namespaces.get_mut() = None;
        *self.extended.get_mut() = None;
        let declared: HashSet<String> = entry
            .symbols
            .iter()
            .filter(|s| s.kind.is_class_like())
            .map(|s| s.fqn.to_ascii_lowercase())
            .collect();
        for owner in entry
            .symbols
            .iter()
            .filter(|s| s.kind.is_member())
            .map(|s| s.fqn.to_ascii_lowercase())
            .filter(|o| !declared.contains(o))
        {
            self.augments.entry(owner).or_default().insert(path.clone());
        }
        for (i, sym) in entry.symbols.iter().enumerate() {
            let r = (path.clone(), i);
            self.keys
                .entry(symbol_key(sym))
                .or_default()
                .push(r.clone());
            if !sym.kind.is_member() {
                self.names.entry(short_name(sym)).or_default().push(r);
            }
        }
        for w in &entry.words {
            self.words
                .entry(w.clone())
                .or_default()
                .insert(path.clone());
        }
        self.files.insert(path, entry);
    }

    /// Lowercased FQNs that some indexed class extends, implements or uses
    /// as a trait. A member missing on one of these may live in a subclass,
    /// so inspections do not report it. Cached until the next change.
    pub fn extended_classes(&self) -> std::sync::Arc<HashSet<String>> {
        let mut cache = self.extended.lock();
        if let Some(set) = cache.as_ref() {
            return set.clone();
        }
        let set: HashSet<String> = self
            .files
            .values()
            .flat_map(|e| e.symbols.iter())
            .filter(|s| s.kind.is_class_like())
            .flat_map(|s| s.supers.iter().map(|x| x.to_ascii_lowercase()))
            .collect();
        let set = std::sync::Arc::new(set);
        *cache = Some(set.clone());
        set
    }

    /// Every namespace that contains an indexed class, lowercased, parents
    /// included (`app`, `app\models`). Inspections use it to tell a
    /// namespace import from a missing class, and to report missing
    /// classes only in namespaces the project knows. Cached until the next
    /// change.
    pub fn namespaces(&self) -> std::sync::Arc<HashSet<String>> {
        let mut cache = self.namespaces.lock();
        if let Some(set) = cache.as_ref() {
            return set.clone();
        }
        let mut set = HashSet::new();
        for entry in self.files.values() {
            for s in entry.symbols.iter().filter(|s| s.kind.is_class_like()) {
                let mut ns = s.fqn.to_ascii_lowercase();
                while let Some((parent, _)) = ns.rsplit_once('\\') {
                    ns = parent.to_string();
                    if !set.insert(ns.clone()) {
                        break;
                    }
                }
            }
        }
        let set = std::sync::Arc::new(set);
        *cache = Some(set.clone());
        set
    }

    /// Drops `path` and everything derived from it; a no-op if it is not indexed. Called when a file is deleted or unreadable.
    pub fn remove(&mut self, path: &Path) {
        *self.namespaces.get_mut() = None;
        *self.extended.get_mut() = None;
        let Some(old) = self.files.remove(path) else {
            return;
        };
        self.augments.retain(|_, files| {
            files.remove(path);
            !files.is_empty()
        });
        for sym in &old.symbols {
            prune(&mut self.keys, &symbol_key(sym), path);
            if !sym.kind.is_member() {
                let key = short_name(sym);
                if let Some(v) = self.names.get_mut(&key) {
                    v.retain(|(p, _)| p != path);
                    if v.is_empty() {
                        self.names.remove(&key);
                    }
                }
            }
        }
        for w in &old.words {
            if let Some(set) = self.words.get_mut(w) {
                set.remove(path);
                if set.is_empty() {
                    self.words.remove(w);
                }
            }
        }
    }

    /// Every indexed file with its whole entry (for the on-disk cache).
    pub fn entries(&self) -> impl Iterator<Item = (&Path, &FileEntry)> {
        self.files.iter().map(|(p, e)| (p.as_path(), e))
    }

    /// Paths of every indexed file.
    pub fn paths(&self) -> Vec<PathBuf> {
        self.files.keys().cloned().collect()
    }

    /// Every indexed file with its symbols.
    pub fn files(&self) -> impl Iterator<Item = (&Path, &[Symbol])> {
        self.files
            .iter()
            .map(|(p, e)| (p.as_path(), e.symbols.as_slice()))
    }

    /// Resolves a [`SymRef`]; `None` if its file was removed or re-indexed with fewer symbols.
    pub fn get(&self, r: &SymRef) -> Option<&Symbol> {
        self.files.get(&r.0)?.symbols.get(r.1)
    }

    /// Symbols declared in `path`, in source order; empty if the file is not indexed. Backs document symbols.
    pub fn file_symbols(&self, path: &Path) -> &[Symbol] {
        self.files
            .get(path)
            .map(|e| e.symbols.as_slice())
            .unwrap_or(&[])
    }

    /// All declarations stored under a raw key built with [`class_key`], [`function_key`], or [`member_key`], with their files.
    pub fn lookup(&self, key: &str) -> Vec<(&Path, &Symbol)> {
        self.keys
            .get(key)
            .into_iter()
            .flatten()
            .filter_map(|r| Some((r.0.as_path(), self.get(r)?)))
            .collect()
    }

    /// Class-like declarations named `fqn` (case-insensitive, no leading `\`); several when declared in more than one file.
    pub fn classes(&self, fqn: &str) -> Vec<(&Path, &Symbol)> {
        self.lookup(&class_key(fqn))
    }

    /// Declarations of the first candidate FQN that exists (PHP fallback order:
    /// namespaced first, then global). Candidates come from
    /// [`crate::syntax::NameContext::resolve_function`].
    pub fn functions(&self, candidates: &[String]) -> Vec<(&Path, &Symbol)> {
        candidates
            .iter()
            .map(|c| self.lookup(&function_key(c)))
            .find(|v| !v.is_empty())
            .unwrap_or_default()
    }

    /// Breadth-first walk over a class and all its supertypes.
    ///
    /// Starts with `fqn` itself, then parents, interfaces, traits, and `@mixin`
    /// classes; cycles are cut. Classes missing from the index still appear but
    /// contribute no supertypes.
    pub fn hierarchy(&self, fqn: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut order = Vec::new();
        let mut queue = VecDeque::from([fqn.to_string()]);
        while let Some(cls) = queue.pop_front() {
            if !seen.insert(cls.to_ascii_lowercase()) {
                continue;
            }
            for (_, sym) in self.classes(&cls) {
                queue.extend(sym.supers.iter().cloned());
            }
            order.push(cls);
        }
        order
    }

    /// Member lookup that walks the inheritance chain, nearest declaration first.
    pub fn find_member(&self, class: &str, tag: MemberTag, name: &str) -> Vec<(&Path, &Symbol)> {
        self.hierarchy(class)
            .iter()
            .map(|cls| self.lookup(&member_key(cls, tag, name)))
            .find(|v| !v.is_empty())
            .unwrap_or_default()
    }

    /// All members visible on `class`, overriding members hide inherited ones.
    pub fn members(&self, class: &str) -> Vec<(&Path, &Symbol)> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for cls in self.hierarchy(class) {
            let mut files: Vec<&Path> = self.classes(&cls).into_iter().map(|(p, _)| p).collect();
            if let Some(extra) = self.augments.get(&cls.to_ascii_lowercase()) {
                let mut extra: Vec<&Path> = extra.iter().map(PathBuf::as_path).collect();
                extra.sort();
                files.extend(extra);
            }
            files.dedup();
            for path in files {
                for sym in self.file_symbols(path) {
                    let Some(tag) = MemberTag::of(sym.kind) else {
                        continue;
                    };
                    if !sym.fqn.eq_ignore_ascii_case(&cls) {
                        continue;
                    }
                    // Key without the owner, so an override hides the inherited member.
                    if seen.insert(member_key("", tag, &sym.name)) {
                        out.push((path, sym));
                    }
                }
            }
        }
        out
    }

    /// Top-level symbols whose short name starts with `prefix` (case-insensitive).
    pub fn top_level_with_prefix(&self, prefix: &str, limit: usize) -> Vec<(&Path, &Symbol)> {
        let lower = prefix.to_ascii_lowercase();
        self.names
            .range(lower.clone()..)
            .take_while(|(k, _)| k.starts_with(&lower))
            .flat_map(|(_, refs)| refs.iter())
            .filter_map(|r| Some((r.0.as_path(), self.get(r)?)))
            .take(limit)
            .collect()
    }

    /// Every top-level symbol (classes, functions, constants), in short-name
    /// order. Completion matches them with camel humps and middle words,
    /// which a prefix range cannot answer.
    pub fn top_level(&self) -> impl Iterator<Item = (&Path, &Symbol)> {
        self.names
            .values()
            .flatten()
            .filter_map(|r| Some((r.0.as_path(), self.get(r)?)))
    }

    /// Top-level symbols whose short name contains `query` (workspace symbols).
    pub fn search(&self, query: &str, limit: usize) -> Vec<(&Path, &Symbol)> {
        let lower = query.to_ascii_lowercase();
        self.names
            .iter()
            .filter(|(k, _)| k.contains(&lower))
            .flat_map(|(_, refs)| refs.iter())
            .filter_map(|r| Some((r.0.as_path(), self.get(r)?)))
            .take(limit)
            .collect()
    }

    /// Files that contain the identifier `word` (case-insensitive).
    pub fn files_with_word(&self, word: &str) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = self
            .words
            .get(&word.to_lowercase())
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        v.sort();
        v
    }
}

/// Removes `path`'s references under `key`, dropping the key once empty so removed files leave no stale entries.
fn prune(map: &mut HashMap<String, Vec<SymRef>>, key: &str, path: &Path) {
    if let Some(v) = map.get_mut(key) {
        v.retain(|(p, _)| p != path);
        if v.is_empty() {
            map.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_replaces_previous_entries() {
        let mut idx = Index::default();
        let p = PathBuf::from("/a.php");
        idx.update(p.clone(), index_source("<?php class Foo {}"));
        assert_eq!(idx.classes("Foo").len(), 1);
        idx.update(p.clone(), index_source("<?php class Bar {}"));
        assert!(idx.classes("Foo").is_empty());
        assert_eq!(idx.classes("bar").len(), 1);
        assert!(idx.files_with_word("foo").is_empty());
        assert_eq!(idx.files_with_word("BAR"), vec![p.clone()]);
        idx.remove(&p);
        assert_eq!(idx.file_count(), 0);
        assert!(idx.top_level_with_prefix("", 10).is_empty());
    }

    #[test]
    fn members_follow_inheritance() {
        let mut idx = Index::default();
        idx.update(
            "/base.php".into(),
            index_source("<?php class Base { function a() {} function b() {} }"),
        );
        idx.update(
            "/child.php".into(),
            index_source("<?php class Child extends Base { function B() {} }"),
        );
        let b = idx.find_member("Child", MemberTag::Method, "b");
        assert_eq!(b[0].0, Path::new("/child.php"));
        let a = idx.find_member("child", MemberTag::Method, "A");
        assert_eq!(a[0].0, Path::new("/base.php"));
        let names: Vec<_> = idx
            .members("Child")
            .iter()
            .map(|(_, s)| s.name.clone())
            .collect();
        assert_eq!(names, ["B", "a"]);
    }
}
