//! Project-wide symbol index.
//!
//! Two indexes:
//! - a *declaration index*: key -> declarations, rebuilt per file from stubs;
//! - a *word index*: identifier -> files containing it, used to narrow
//!   "find usages" to candidate files before any parsing happens.
//!
//! Each file is indexed independently and keyed by a content hash, so a file
//! whose content did not change is never reparsed.

use crate::stubs::{self, Kind, MemberTag, Symbol};
use crate::syntax::parse;
use crate::text::LineIndex;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

pub struct FileEntry {
    pub hash: u64,
    pub symbols: Vec<Symbol>,
    pub words: HashSet<String>,
}

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
        symbols: stubs::build(tree.root_node(), src, &lines),
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

pub fn class_key(fqn: &str) -> String {
    format!("c:{}", fqn.to_ascii_lowercase())
}

pub fn function_key(fqn: &str) -> String {
    format!("f:{}", fqn.to_ascii_lowercase())
}

fn symbol_key(sym: &Symbol) -> String {
    match sym.kind {
        k if k.is_class_like() => class_key(&sym.fqn),
        Kind::Function => function_key(&sym.fqn),
        Kind::Constant => format!("g:{}", sym.fqn),
        k => member_key(&sym.fqn, MemberTag::of(k).unwrap(), &sym.name),
    }
}

fn short_name(sym: &Symbol) -> String {
    sym.fqn
        .rsplit('\\')
        .next()
        .unwrap_or(&sym.fqn)
        .to_ascii_lowercase()
}

/// Reference to a symbol stored in the index.
pub type SymRef = (PathBuf, usize);

#[derive(Default)]
pub struct Index {
    files: HashMap<PathBuf, FileEntry>,
    keys: HashMap<String, Vec<SymRef>>,
    /// Lowercase short name of top-level symbols, ordered for prefix queries.
    names: BTreeMap<String, Vec<SymRef>>,
    words: HashMap<String, HashSet<PathBuf>>,
}

impl Index {
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn is_current(&self, path: &Path, hash: u64) -> bool {
        self.files.get(path).is_some_and(|e| e.hash == hash)
    }

    pub fn update(&mut self, path: PathBuf, entry: FileEntry) {
        self.remove(&path);
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

    pub fn remove(&mut self, path: &Path) {
        let Some(old) = self.files.remove(path) else {
            return;
        };
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

    /// Every indexed file with its symbols.
    pub fn files(&self) -> impl Iterator<Item = (&Path, &[Symbol])> {
        self.files
            .iter()
            .map(|(p, e)| (p.as_path(), e.symbols.as_slice()))
    }

    pub fn get(&self, r: &SymRef) -> Option<&Symbol> {
        self.files.get(&r.0)?.symbols.get(r.1)
    }

    pub fn file_symbols(&self, path: &Path) -> &[Symbol] {
        self.files
            .get(path)
            .map(|e| e.symbols.as_slice())
            .unwrap_or(&[])
    }

    pub fn lookup(&self, key: &str) -> Vec<(&Path, &Symbol)> {
        self.keys
            .get(key)
            .into_iter()
            .flatten()
            .filter_map(|r| Some((r.0.as_path(), self.get(r)?)))
            .collect()
    }

    pub fn classes(&self, fqn: &str) -> Vec<(&Path, &Symbol)> {
        self.lookup(&class_key(fqn))
    }

    /// First candidate FQN that has a declaration wins (PHP fallback order).
    pub fn functions(&self, candidates: &[String]) -> Vec<(&Path, &Symbol)> {
        candidates
            .iter()
            .map(|c| self.lookup(&function_key(c)))
            .find(|v| !v.is_empty())
            .unwrap_or_default()
    }

    /// Breadth-first walk over a class and all its supertypes.
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
            files.dedup();
            for path in files {
                for sym in self.file_symbols(path) {
                    if !sym.kind.is_member() || !sym.fqn.eq_ignore_ascii_case(&cls) {
                        continue;
                    }
                    // Key without the owner, so an override hides the inherited member.
                    let tag = MemberTag::of(sym.kind).unwrap();
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
