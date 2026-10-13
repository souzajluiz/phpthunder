//! Type-aware navigation: go to implementation and the type hierarchy.

use super::*;
use crate::palette::{Item, Mode, Palette, Target};
use crate::tr;
use iced::Task;
use iced::widget::{Id, operation};
use phpthunder::symbol::{Kind, Symbol};
use std::path::PathBuf;

/// Short label for a symbol in pickers: `Class` or `Class::method()`.
fn symbol_title(s: &Symbol) -> String {
    let owner = s.fqn.rsplit('\\').next().unwrap_or(&s.fqn);
    match s.kind {
        Kind::Method => format!("{owner}::{}()", s.name),
        _ => owner.to_string(),
    }
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
                self.close_overlays();
                let title = tr!("implementations-title", name = symbol_title(s));
                self.palette = Some(Palette {
                    mode: Mode::Choose,
                    query: String::new(),
                    items: items.clone(),
                    selected: 0,
                    fixed: Some((title, items)),
                });
                operation::focus(Id::new(crate::palette::INPUT_ID))
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
        self.bottom = Some(Bottom::Hierarchy);
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
}
