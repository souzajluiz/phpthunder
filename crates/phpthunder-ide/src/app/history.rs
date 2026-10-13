//! Where the user has been and what they copied: bookmarks, the last edit
//! locations, recent locations, the clipboard history, and the jumps that
//! need the index (super method, type declaration, test ↔ subject).

use super::*;
use crate::palette::{Item, Target};
use crate::tr;
use iced::Task;
use iced::widget::text_editor::Position;
use phpthunder::symbol::{Kind, MemberTag};
use std::path::{Path, PathBuf};

/// Most edit locations remembered.
const EDIT_HISTORY: usize = 100;
/// Most clipboard entries remembered.
const CLIPBOARD_HISTORY: usize = 30;

impl App {
    /// Remembers the caret line of the active document as the latest edit
    /// location; an edit next to the previous one replaces it, so typing a
    /// paragraph leaves one entry. Called after every text change.
    pub(crate) fn record_edit_location(&mut self) {
        let Some(doc) = self.doc() else {
            return;
        };
        let (path, line) = (doc.path.clone(), doc.cursor().line);
        if let Some(last) = self.edit_locations.last_mut()
            && last.0 == path
            && last.1.abs_diff(line) <= 1
        {
            last.1 = line;
        } else {
            self.edit_locations.push((path, line));
            if self.edit_locations.len() > EDIT_HISTORY {
                self.edit_locations.remove(0);
            }
        }
        self.edit_back = 0;
    }

    /// `⇧⌘⌫`: goes to the latest edit location; pressing again steps to
    /// earlier ones.
    pub(crate) fn jump_to_last_edit(&mut self) -> Task<Message> {
        let n = self.edit_locations.len();
        if n == 0 {
            return Task::none();
        }
        let (path, line) = self.edit_locations[n - 1 - self.edit_back.min(n - 1)].clone();
        self.edit_back = (self.edit_back + 1).min(n - 1);
        self.record_nav();
        self.open_at_line(path, line)
    }

    /// Opens `path` (or focuses its tab) with the caret at the start of
    /// `line`.
    pub(crate) fn open_at_line(&mut self, path: PathBuf, line: usize) -> Task<Message> {
        if self.doc().is_some_and(|d| d.path == path) {
            return self.place_cursor(Position { line, column: 0 });
        }
        self.open(
            path,
            Some(phpthunder::syntax::Pos {
                line: line as u32,
                col: 0,
            }),
        )
    }

    /// `⇧⌘E`: recent edit and navigation locations, newest first, each
    /// with its line of code.
    pub(crate) fn open_recent_locations(&mut self) -> Task<Message> {
        let mut seen = std::collections::HashSet::new();
        let jumps = self
            .nav_back
            .iter()
            .map(|l| (l.path.clone(), l.pos.line as usize));
        let all: Vec<(PathBuf, usize)> = self
            .edit_locations
            .iter()
            .cloned()
            .chain(jumps)
            .rev()
            .filter(|loc| seen.insert(loc.clone()))
            .take(50)
            .collect();
        let items = all
            .into_iter()
            .map(|(path, line)| Item {
                badge: "↺",
                title: self.line_preview(&path, line),
                matched: Vec::new(),
                detail: format!("{}:{}", self.relative(&path), line + 1),
                target: Target::Location(path, line),
            })
            .collect();
        self.open_choice(tr!("action-recent-locations"), items)
    }

    /// The trimmed text of `line` in `path`, from the open tab or the disk.
    fn line_preview(&self, path: &Path, line: usize) -> String {
        let from_doc = self
            .docs
            .iter()
            .find(|d| d.path == path)
            .filter(|d| line < d.line_count())
            .map(|d| d.line_text(line).trim().to_string());
        from_doc
            .or_else(|| {
                std::fs::read_to_string(path)
                    .ok()
                    .and_then(|t| t.lines().nth(line).map(|l| l.trim().to_string()))
            })
            .unwrap_or_default()
    }

    /// Distraction-free mode: hides the rail, sidebar, panels and status
    /// bar, and brings them back when turned off.
    pub(crate) fn toggle_zen(&mut self) -> Task<Message> {
        match self.zen.take() {
            Some((sidebar, bottom, right)) => {
                self.sidebar = sidebar;
                self.bottom = bottom;
                self.right = right;
            }
            None => {
                self.zen = Some((self.sidebar, self.bottom, self.right));
                self.sidebar = false;
                self.bottom = None;
                self.right = None;
            }
        }
        Task::none()
    }

    /// `⌥Space`: shows the source of the declaration at the caret in a
    /// popup, without leaving the file.
    pub(crate) fn quick_definition(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let Some(def) = self.lang.definitions(&doc.text, offset).into_iter().next() else {
            self.notice = Some(tr!("notice-no-declaration"));
            return Task::none();
        };
        let source = self
            .docs
            .iter()
            .find(|d| d.path == def.path)
            .map(|d| d.text.clone())
            .or_else(|| std::fs::read_to_string(&def.path).ok())
            .unwrap_or_default();
        let span = &def.symbol.span;
        let (first, last) = (span.start.line as usize, span.end.line as usize);
        let lines: Vec<&str> = source
            .lines()
            .skip(first)
            .take(last.min(first + 24) - first + 1)
            .collect();
        let indent = lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start().len())
            .min()
            .unwrap_or(0);
        let mut code: Vec<String> = lines
            .iter()
            .map(|l| l.get(indent..).unwrap_or(l.trim_start()).to_string())
            .collect();
        if last > first + 24 {
            code.push("…".to_string());
        }
        let pos = doc.cursor();
        let col = doc.visual_column(pos);
        self.hover = Some(Hover {
            line: pos.line,
            col,
            signature: code.join("\n"),
            location: format!("{}:{}", self.relative(&def.path), first + 1),
            doc: String::new(),
            problems: Vec::new(),
        });
        Task::none()
    }

    /// `F3`: adds or removes a bookmark on the caret line.
    pub(crate) fn toggle_bookmark(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let (path, line) = (doc.path.clone(), doc.cursor().line);
        let lines = self.bookmarks.entry(path.clone()).or_default();
        if !lines.remove(&line) {
            lines.insert(line);
        }
        if lines.is_empty() {
            self.bookmarks.remove(&path);
        }
        Task::none()
    }

    /// `⌘F3`: every bookmark, by file and line.
    pub(crate) fn open_bookmarks(&mut self) -> Task<Message> {
        let marks: Vec<(PathBuf, usize)> = self
            .bookmarks
            .iter()
            .flat_map(|(p, lines)| lines.iter().map(move |l| (p.clone(), *l)))
            .collect();
        if marks.is_empty() {
            self.notice = Some(tr!("notice-no-bookmarks"));
            return Task::none();
        }
        let items = marks
            .into_iter()
            .map(|(path, line)| Item {
                badge: "◆",
                title: self.line_preview(&path, line),
                matched: Vec::new(),
                detail: format!("{}:{}", self.relative(&path), line + 1),
                target: Target::Location(path, line),
            })
            .collect();
        self.open_choice(tr!("action-bookmarks"), items)
    }

    /// Records the selection of the active document as the newest
    /// clipboard entry (called on copy and cut, before the editor does it).
    pub(crate) fn remember_selection(&mut self) {
        let Some(text) = self
            .doc()
            .and_then(|d| d.selected_text())
            .map(str::to_string)
        else {
            return;
        };
        self.clipboard_history.retain(|t| *t != text);
        self.clipboard_history.insert(0, text);
        self.clipboard_history.truncate(CLIPBOARD_HISTORY);
    }

    /// `⇧⌘V`: the clipboard history, newest first; picking an entry pastes
    /// it at the caret.
    pub(crate) fn open_clipboard_history(&mut self) -> Task<Message> {
        if self.clipboard_history.is_empty() {
            self.notice = Some(tr!("notice-clipboard-empty"));
            return Task::none();
        }
        let items = self
            .clipboard_history
            .iter()
            .map(|text| {
                let first = text.lines().next().unwrap_or_default().trim();
                let more = text.lines().count();
                Item {
                    badge: "⧉",
                    title: first.chars().take(80).collect(),
                    matched: Vec::new(),
                    detail: if more > 1 {
                        tr!("clipboard-lines", count = more)
                    } else {
                        String::new()
                    },
                    target: Target::Paste(text.clone()),
                }
            })
            .collect();
        self.open_choice(tr!("action-paste-from-history"), items)
    }

    /// `⌥⇧⌘C`: copies a reference to the symbol at the caret (`Class`,
    /// `Class::method`, `function`), or `path:line` when there is none.
    pub(crate) fn copy_reference(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let reference = match self.lang.definitions(&doc.text, offset).into_iter().next() {
            Some(def) => {
                let s = &def.symbol;
                match MemberTag::of(s.kind) {
                    Some(MemberTag::Property) => format!("{}::${}", s.fqn, s.name),
                    Some(_) => format!("{}::{}", s.fqn, s.name),
                    None => s.fqn.clone(),
                }
            }
            None => format!("{}:{}", self.relative(&doc.path), doc.cursor().line + 1),
        };
        self.notice = Some(tr!("notice-copied", text = reference.as_str()));
        iced::clipboard::write(reference)
    }

    /// `⌘U`: from a method, goes to the method it overrides or implements
    /// in a supertype.
    pub(crate) fn goto_super_method(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let method = self
            .lang
            .file_symbols(&doc.path)
            .into_iter()
            .filter(|s| s.kind == Kind::Method)
            .filter(|s| s.name_start <= offset)
            .max_by_key(|s| s.name_start);
        let target =
            method.and_then(|m| self.lang.super_member(&m.fqn, MemberTag::Method, &m.name));
        match target {
            Some((path, symbol)) => {
                self.record_nav();
                self.open(path, Some(symbol.name_span.start))
            }
            None => {
                self.notice = Some(tr!("notice-no-super-method"));
                Task::none()
            }
        }
    }

    /// `⇧⌘B`: goes to the class of the value at the caret: a property's
    /// or function's declared type, or the class itself.
    pub(crate) fn goto_type_declaration(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let class = self
            .lang
            .definitions(&doc.text, offset)
            .into_iter()
            .next()
            .and_then(|def| {
                let s = def.symbol;
                if s.kind.is_class_like() {
                    Some(s.fqn)
                } else {
                    s.ty.class.or(s.ty.elem)
                }
            });
        match class.and_then(|c| self.lang.class_declaration(&c)) {
            Some((path, symbol)) => {
                self.record_nav();
                self.open(path, Some(symbol.name_span.start))
            }
            None => {
                self.notice = Some(tr!("notice-no-type-declaration"));
                Task::none()
            }
        }
    }

    /// `⇧⌘T`: switches between a class and its test (`FooTest.php`, or a
    /// Pest `FooTest.php`/`Foo.test.php`), searching the project files.
    pub(crate) fn goto_test(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let stem = doc
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let wanted: Vec<String> = match stem.strip_suffix("Test") {
            Some(subject) => vec![format!("{subject}.php")],
            None => vec![format!("{stem}Test.php"), format!("{stem}.test.php")],
        };
        let here = doc.path.clone();
        let found: Vec<PathBuf> = self
            .project_files
            .iter()
            .filter(|p| **p != here)
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| wanted.iter().any(|w| n.to_string_lossy() == *w))
            })
            .cloned()
            .collect();
        match found.len() {
            0 => {
                self.notice = Some(tr!("notice-no-test", name = stem));
                Task::none()
            }
            1 => {
                self.record_nav();
                self.open(found[0].clone(), None)
            }
            _ => {
                let items = found
                    .into_iter()
                    .map(|path| Item {
                        badge: "T",
                        title: crate::app::file_name(&path),
                        matched: Vec::new(),
                        detail: self.relative(&path),
                        target: Target::File(path),
                    })
                    .collect();
                self.open_choice(tr!("action-go-to-test"), items)
            }
        }
    }
}
