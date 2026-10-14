//! Type-aware navigation: go to implementation, the type and method
//! hierarchies, callers, related files, and the type at the caret.

use super::*;
use crate::palette::{Item, Target};
use crate::tr;
use iced::Task;
use phpthunder::symbol::{Kind, Symbol};
use phpthunder::syntax::Pos;
use std::path::PathBuf;

/// Short label for a symbol in pickers: `Class` or `Class::method()`.
fn symbol_title(s: &Symbol) -> String {
    let owner = s.fqn.rsplit('\\').next().unwrap_or(&s.fqn);
    match s.kind {
        Kind::Method => format!("{owner}::{}()", s.name),
        _ => owner.to_string(),
    }
}

/// The innermost function or method of `symbols` whose declaration
/// contains `pos`; `None` at the top level of a file.
pub(crate) fn enclosing_function(symbols: &[Symbol], pos: Pos) -> Option<&Symbol> {
    let key = |p: Pos| (p.line, p.col);
    symbols
        .iter()
        .filter(|s| matches!(s.kind, Kind::Method | Kind::Function) && !s.is_virtual)
        .filter(|s| key(s.span.start) <= key(pos) && key(pos) < key(s.span.end))
        .max_by_key(|s| key(s.span.start))
}

impl App {
    /// Jumps to subclasses/implementors of the class at the caret, or to
    /// overrides of the method at the caret (`⌥⌘B`). Several results open a
    /// picker; none shows a notice.
    pub(crate) fn goto_implementation(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let Some(def) = self.lang.definitions(&doc.text, offset).into_iter().next() else {
            self.notice = Some(tr!("notice-caret-class-or-method"));
            return Task::none();
        };
        let s = &def.symbol;
        let found: Vec<(PathBuf, Symbol)> = match s.kind {
            k if k.is_class_like() => self.lang.subtypes(&s.fqn),
            Kind::Method => self.lang.overriding_methods(&s.fqn, &s.name),
            _ => Vec::new(),
        };
        match found.len() {
            0 => {
                self.notice = Some(tr!("notice-no-implementations", name = symbol_title(s)));
                Task::none()
            }
            1 => {
                let Some((path, symbol)) = found.into_iter().next() else {
                    return Task::none();
                };
                self.record_nav();
                self.open(path, Some(symbol.name_span.start))
            }
            _ => {
                let root = self.tree.root.clone();
                let items: Vec<Item> = found
                    .into_iter()
                    .map(|(path, symbol)| {
                        let rel = root
                            .as_ref()
                            .and_then(|r| path.strip_prefix(r).ok())
                            .unwrap_or(&path)
                            .display()
                            .to_string();
                        Item {
                            badge: if symbol.kind == Kind::Method {
                                "m"
                            } else {
                                "C"
                            },
                            title: symbol_title(&symbol),
                            matched: Vec::new(),
                            detail: format!("{rel}:{}", symbol.name_span.start.line + 1),
                            target: Target::Symbol(path, symbol.name_span.start),
                        }
                    })
                    .collect();
                self.open_choice(tr!("implementations-title", name = symbol_title(s)), items)
            }
        }
    }

    /// Fills the Hierarchy panel for the class at the caret (or around it):
    /// supertypes above, subtypes below (`⌃H`).
    pub(crate) fn show_type_hierarchy(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let at_caret = self
            .lang
            .definitions(&doc.text, offset)
            .into_iter()
            .find(|d| d.symbol.kind.is_class_like());
        let class = match at_caret {
            Some(def) => def.symbol.fqn,
            None => match phpthunder::refactor::enclosing_class_fqn(&doc.text, offset) {
                Some(fqn) => fqn,
                None => {
                    self.notice = Some(tr!("notice-caret-in-class"));
                    return Task::none();
                }
            },
        };
        let root = self.tree.root.clone();
        let row = |depth: usize, fqn: &str, decl: Option<(PathBuf, Symbol)>| {
            let (ns, short) = fqn.rsplit_once('\\').unwrap_or(("", fqn));
            let file = decl.as_ref().map(|(p, _)| {
                root.as_ref()
                    .and_then(|r| p.strip_prefix(r).ok())
                    .unwrap_or(p)
                    .display()
                    .to_string()
            });
            HierarchyRow {
                depth,
                label: short.to_string(),
                detail: [
                    ns.to_string(),
                    file.unwrap_or_else(|| tr!("hierarchy-not-indexed")),
                ]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
                target: decl.map(|(p, s)| (p, s.name_span.start)),
            }
        };
        let supers = self.lang.supertypes(&class);
        let mut rows: Vec<HierarchyRow> = supers
            .into_iter()
            .rev()
            .enumerate()
            .map(|(depth, (fqn, decl))| row(depth, &fqn, decl))
            .collect();
        let class_depth = rows.len().saturating_sub(1);
        for (path, sub) in self.lang.subtypes(&class) {
            let fqn = sub.fqn.clone();
            rows.push(row(class_depth + 1, &fqn, Some((path, sub))));
        }
        let short = class.rsplit('\\').next().unwrap_or(&class);
        self.hierarchy = Some(Hierarchy {
            title: tr!("hierarchy-title", name = short),
            rows,
        });
        self.show_panel(Bottom::Hierarchy);
        Task::none()
    }

    /// Opens the declaration of the `i`-th hierarchy row.
    pub(crate) fn open_hierarchy_row(&mut self, i: usize) -> Task<Message> {
        let Some(target) = self
            .hierarchy
            .as_ref()
            .and_then(|h| h.rows.get(i))
            .and_then(|r| r.target.clone())
        else {
            return Task::none();
        };
        self.record_nav();
        self.open(target.0, Some(target.1))
    }

    /// The method at the caret, or the one the caret is inside.
    fn method_at_caret(&self) -> Option<Symbol> {
        let doc = self.doc()?;
        let offset = doc.offset(doc.cursor());
        self.lang
            .definitions(&doc.text, offset)
            .into_iter()
            .map(|d| d.symbol)
            .find(|s| s.kind == Kind::Method)
            .or_else(|| {
                let symbols = self.lang.file_symbols(&doc.path);
                enclosing_function(&symbols, doc.utf16_pos(doc.cursor()))
                    .filter(|s| s.kind == Kind::Method)
                    .cloned()
            })
    }

    /// `⇧⌘H`: the class hierarchy of the method at the caret (supertypes
    /// above, subtypes below), each row telling whether that class declares
    /// the method and opening that declaration when it does.
    pub(crate) fn show_method_hierarchy(&mut self) -> Task<Message> {
        let Some(method) = self.method_at_caret() else {
            self.notice = Some(tr!("notice-caret-in-method"));
            return Task::none();
        };
        let (class, name) = (method.fqn.clone(), method.name.clone());
        let root = self.tree.root.clone();
        let relative = |p: &PathBuf| {
            root.as_ref()
                .and_then(|r| p.strip_prefix(r).ok())
                .unwrap_or(p)
                .display()
                .to_string()
        };
        let row = |depth: usize, fqn: &str, decl: Option<(PathBuf, Symbol)>| {
            let short = fqn.rsplit('\\').next().unwrap_or(fqn);
            match self.lang.declared_method(fqn, &name) {
                Some((path, m)) => HierarchyRow {
                    depth,
                    label: format!("{short}::{}()", m.name),
                    detail: format!("{}:{}", relative(&path), m.name_span.start.line + 1),
                    target: Some((path, m.name_span.start)),
                },
                None => HierarchyRow {
                    depth,
                    label: short.to_string(),
                    detail: tr!("method-hierarchy-inherited"),
                    target: decl.map(|(p, s)| (p, s.name_span.start)),
                },
            }
        };
        let mut rows: Vec<HierarchyRow> = self
            .lang
            .supertypes(&class)
            .into_iter()
            .rev()
            .enumerate()
            .map(|(depth, (fqn, decl))| row(depth, &fqn, decl))
            .collect();
        let class_depth = rows.len().saturating_sub(1);
        for (path, sub) in self.lang.subtypes(&class) {
            let fqn = sub.fqn.clone();
            rows.push(row(class_depth + 1, &fqn, Some((path, sub))));
        }
        self.hierarchy = Some(Hierarchy {
            title: tr!("method-hierarchy-title", name = format!("{}()", qualified(&method))),
            rows,
        });
        self.show_panel(Bottom::Hierarchy);
        Task::none()
    }

    /// `⌃⌥H`: the usages of the function or method at the caret, listed by
    /// the function each call sits in ([`App::show_usages`] reads
    /// `callers_search`).
    pub(crate) fn call_hierarchy(&mut self) -> Task<Message> {
        let task = self.find_usages();
        self.callers_search = true;
        task
    }

    /// `⌃⌘↑`: files related to the active one: its tests or its subject,
    /// and the declarations of its class's parents and interfaces. One
    /// result opens directly; several open a picker.
    pub(crate) fn goto_related(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let mut items: Vec<Item> = self
            .test_counterparts(&doc.path)
            .into_iter()
            .map(|path| Item {
                badge: "T",
                title: crate::app::file_name(&path),
                matched: Vec::new(),
                detail: self.relative(&path),
                target: Target::File(path),
            })
            .collect();
        let offset = doc.offset(doc.cursor());
        if let Some(class) = phpthunder::refactor::enclosing_class_fqn(&doc.text, offset) {
            for (fqn, decl) in self.lang.supertypes(&class).into_iter().skip(1) {
                if let Some((path, symbol)) = decl {
                    items.push(Item {
                        badge: "C",
                        title: fqn.rsplit('\\').next().unwrap_or(&fqn).to_string(),
                        matched: Vec::new(),
                        detail: self.relative(&path),
                        target: Target::Symbol(path, symbol.name_span.start),
                    });
                }
            }
        }
        match items.len() {
            0 => {
                self.notice = Some(tr!("notice-nothing-related"));
                Task::none()
            }
            1 => {
                self.record_nav();
                match items.remove(0).target {
                    Target::File(path) => self.open(path, None),
                    Target::Symbol(path, pos) => self.open(path, Some(pos)),
                    _ => Task::none(),
                }
            }
            _ => self.open_choice(tr!("action-related"), items),
        }
    }

    /// `⌃⇧P`: the inferred type of the expression at the caret, as a notice.
    pub(crate) fn expression_type_info(&mut self) -> Task<Message> {
        let Some(doc) = self.doc().filter(|d| d.is_php()) else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let declared = || {
            self.lang
                .definitions(&doc.text, offset)
                .into_iter()
                .next()
                .and_then(|d| d.symbol.ty.class)
        };
        self.notice = Some(
            match self.lang.expression_type(&doc.text, offset).or_else(declared) {
                Some(ty) => tr!("notice-expression-type", ty = ty),
                None => tr!("notice-type-unknown"),
            },
        );
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Symbols of a file with a class method (lines 2–3) and a function
    /// (lines 5–7).
    fn symbols() -> Vec<Symbol> {
        phpthunder::index::index_source(
            "<?php\nclass A {\n    function m() {\n    }\n}\nfunction f() {\n    return 1;\n}\n",
        )
        .symbols
    }

    #[test]
    fn enclosing_function_should_find_the_method() {
        let symbols = symbols();
        let found = enclosing_function(&symbols, Pos { line: 3, col: 2 });
        assert_eq!(found.map(|s| s.name.as_str()), Some("m"));
    }

    #[test]
    fn enclosing_function_should_find_the_function() {
        let symbols = symbols();
        let found = enclosing_function(&symbols, Pos { line: 6, col: 4 });
        assert_eq!(found.map(|s| s.name.as_str()), Some("f"));
    }

    #[test]
    fn enclosing_function_should_be_none_outside_functions() {
        let symbols = symbols();
        assert!(enclosing_function(&symbols, Pos { line: 4, col: 0 }).is_none());
    }
}
