//! Application state and update logic.

use crate::document::Document;
use crate::language::{IndexReport, Language};
use crate::theme::{CHAR_WIDTH, CODE_LINE, EDITOR_PAD_LEFT, EDITOR_PAD_TOP};
use crate::tree::{git_branch, FileTree};
use iced::keyboard::{self, key::Named, Key};
use iced::widget::operation::{self, AbsoluteOffset};
use iced::widget::scrollable::Viewport;
use iced::widget::text_editor::{Action, Binding, Edit, KeyPress, Position};
use iced::widget::Id;
use iced::{Point, Subscription, Task};
use phpthunder::analysis::{Candidate, Definition, Usage};
use phpthunder::stubs::{Kind, Symbol};
use phpthunder::text::Pos;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

pub const EDITOR_ID: &str = "editor";
pub const EDITOR_SCROLL_ID: &str = "editor-scroll";
const HOVER_DELAY: Duration = Duration::from_millis(450);

#[derive(Debug, Clone)]
pub enum Message {
    OpenFolderDialog,
    FolderPicked(Option<PathBuf>),
    Indexed(IndexReport),
    ToggleDir(PathBuf),
    OpenFile(PathBuf),
    FileLoaded(PathBuf, Result<String, String>, Option<Pos>),
    SelectTab(usize),
    CloseTab(usize),
    Edit(Action),
    Scrolled(Viewport),
    Save,
    TriggerCompletion,
    CompletionMove(i32),
    CompletionAccept,
    CompletionPick(usize),
    CloseOverlays,
    MouseMoved(Point),
    MouseLeft,
    HoverFire(u64),
    GotoDefinition,
    FindUsages,
    UsagesFound(Option<(Definition, Vec<Usage>)>),
    OpenUsage(usize),
    CloseUsages,
    ToggleSidebar,
    Key(keyboard::Event),
}

pub enum Indexing {
    Idle,
    Running,
    Done(IndexReport),
}

pub struct Completion {
    pub items: Vec<Candidate>,
    pub selected: usize,
    /// Byte offset where the replaced prefix starts.
    pub start: usize,
    /// Visual position of the prefix start, for placing the popup.
    pub line: usize,
    pub col: usize,
}

pub struct Hover {
    pub line: usize,
    pub col: usize,
    pub signature: String,
    pub location: String,
    pub doc: String,
}

pub struct UsageRow {
    pub path: PathBuf,
    pub pos: Pos,
    pub label: String,
    pub preview: String,
    pub is_declaration: bool,
}

pub enum Usages {
    Searching(String),
    Found { title: String, rows: Vec<UsageRow>, selected: Option<usize> },
}

pub struct App {
    pub lang: Language,
    pub tree: FileTree,
    pub branch: Option<String>,
    pub docs: Vec<Document>,
    pub active: Option<usize>,
    pub indexing: Indexing,
    pub sidebar: bool,
    pub viewport_height: f32,
    pub completion: Option<Completion>,
    pub hover: Option<Hover>,
    hover_generation: u64,
    hover_at: Option<(usize, usize)>,
    pub usages: Option<Usages>,
    pub notice: Option<String>,
}

fn is_ident(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Editor key bindings: IDE shortcuts first, then iced's defaults.
pub fn bindings(key: KeyPress, completion_open: bool) -> Option<Binding<Message>> {
    let m = key.modifiers;
    let custom = |msg| Some(Binding::Custom(msg));
    match key.key.as_ref() {
        Key::Named(Named::F12) if m.shift() => return custom(Message::FindUsages),
        Key::Named(Named::F12) => return custom(Message::GotoDefinition),
        Key::Character("b") if m.command() => return custom(Message::GotoDefinition),
        Key::Character("s") if m.command() => return custom(Message::Save),
        Key::Named(Named::Space) if m.control() => return custom(Message::TriggerCompletion),
        Key::Character(" ") if m.control() => return custom(Message::TriggerCompletion),
        _ => {}
    }
    if completion_open {
        match key.key.as_ref() {
            Key::Named(Named::ArrowDown) => return custom(Message::CompletionMove(1)),
            Key::Named(Named::ArrowUp) => return custom(Message::CompletionMove(-1)),
            Key::Named(Named::Enter | Named::Tab) => return custom(Message::CompletionAccept),
            Key::Named(Named::Escape) => return custom(Message::CloseOverlays),
            _ => {}
        }
    }
    match key.key.as_ref() {
        Key::Named(Named::Tab) if !m.shift() => {
            Some(Binding::Sequence(vec![Binding::Insert(' '); 4]))
        }
        Key::Named(Named::Escape) => custom(Message::CloseOverlays),
        _ => Binding::from_key_press(key),
    }
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let app = Self {
            lang: Language::default(),
            tree: FileTree::default(),
            branch: None,
            docs: Vec::new(),
            active: None,
            indexing: Indexing::Idle,
            sidebar: true,
            viewport_height: 700.0,
            completion: None,
            hover: None,
            hover_generation: 0,
            hover_at: None,
            usages: None,
            notice: None,
        };
        let folder = std::env::args().nth(1).map(PathBuf::from).filter(|p| p.is_dir());
        let task = match folder {
            Some(dir) => Task::done(Message::FolderPicked(Some(dir))),
            None => Task::none(),
        };
        (app, task)
    }

    pub fn title(&self) -> String {
        match (&self.tree.root, self.active.map(|i| &self.docs[i])) {
            (Some(root), Some(doc)) => format!("{} — {} — phpthunder", doc.title(), folder_name(root)),
            (Some(root), None) => format!("{} — phpthunder", folder_name(root)),
            _ => "phpthunder".into(),
        }
    }

    pub fn doc(&self) -> Option<&Document> {
        self.active.and_then(|i| self.docs.get(i))
    }

    fn doc_mut(&mut self) -> Option<&mut Document> {
        self.active.and_then(|i| self.docs.get_mut(i))
    }

    pub fn subscription(&self) -> Subscription<Message> {
        keyboard::listen().map(Message::Key)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenFolderDialog => Task::perform(
                async { rfd::AsyncFileDialog::new().pick_folder().await.map(|h| h.path().to_path_buf()) },
                Message::FolderPicked,
            ),
            Message::FolderPicked(None) => Task::none(),
            Message::FolderPicked(Some(root)) => {
                let root = root.canonicalize().unwrap_or(root);
                self.tree = FileTree::open(root.clone());
                self.branch = git_branch(&root);
                self.docs.clear();
                self.active = None;
                self.close_overlays();
                self.usages = None;
                self.indexing = Indexing::Running;
                Task::perform(self.lang.clone().index_project(root), Message::Indexed)
            }
            Message::Indexed(report) => {
                self.indexing = Indexing::Done(report);
                // Edits made while the scan ran win over what it read from disk.
                for doc in self.docs.iter().filter(|d| d.dirty && d.is_php()) {
                    self.lang.update_file(&doc.path, &doc.text);
                }
                Task::none()
            }
            Message::ToggleDir(dir) => {
                self.tree.toggle(&dir);
                Task::none()
            }
            Message::OpenFile(path) => self.open(path, None),
            Message::FileLoaded(path, result, target) => match result {
                Ok(text) => {
                    if !self.docs.iter().any(|d| d.path == path) {
                        self.docs.push(Document::new(path.clone(), text));
                    }
                    self.open(path, target)
                }
                Err(e) => {
                    self.notice = Some(format!("Could not open {}: {e}", path.display()));
                    Task::none()
                }
            },
            Message::SelectTab(i) => self.select(i),
            Message::CloseTab(i) => {
                if i < self.docs.len() {
                    self.docs.remove(i);
                    self.close_overlays();
                    let next = match self.active {
                        _ if self.docs.is_empty() => None,
                        Some(a) if a > i => Some(a - 1),
                        Some(a) => Some(a.min(self.docs.len() - 1)),
                        None => None,
                    };
                    self.active = None;
                    if let Some(n) = next {
                        return self.select(n);
                    }
                }
                Task::none()
            }
            Message::Edit(action) => self.edit(action),
            Message::Scrolled(viewport) => {
                self.viewport_height = viewport.bounds().height;
                let y = viewport.absolute_offset().y;
                if let Some(doc) = self.doc_mut() {
                    doc.scroll_y = y;
                }
                self.hover = None;
                Task::none()
            }
            Message::Save => self.save(),
            Message::TriggerCompletion => {
                self.refresh_completion(true);
                Task::none()
            }
            Message::CompletionMove(delta) => {
                if let Some(c) = &mut self.completion {
                    let n = c.items.len() as i32;
                    c.selected = ((c.selected as i32 + delta).rem_euclid(n.max(1))) as usize;
                }
                Task::none()
            }
            Message::CompletionPick(i) => {
                if let Some(c) = &mut self.completion {
                    c.selected = i;
                }
                self.accept_completion()
            }
            Message::CompletionAccept => self.accept_completion(),
            Message::CloseOverlays => {
                self.close_overlays();
                Task::none()
            }
            Message::MouseMoved(p) => self.mouse_moved(p),
            Message::MouseLeft => {
                self.hover_at = None;
                self.hover = None;
                Task::none()
            }
            Message::HoverFire(generation) => {
                if generation == self.hover_generation && self.completion.is_none() {
                    self.show_hover();
                }
                Task::none()
            }
            Message::GotoDefinition => self.goto_definition(),
            Message::FindUsages => self.find_usages(),
            Message::UsagesFound(found) => {
                self.show_usages(found);
                Task::none()
            }
            Message::OpenUsage(i) => {
                let Some(Usages::Found { rows, selected, .. }) = &mut self.usages else { return Task::none() };
                let Some(row) = rows.get(i) else { return Task::none() };
                *selected = Some(i);
                let (path, pos) = (row.path.clone(), row.pos);
                self.open(path, Some(pos))
            }
            Message::CloseUsages => {
                self.usages = None;
                Task::none()
            }
            Message::ToggleSidebar => {
                self.sidebar = !self.sidebar;
                Task::none()
            }
            Message::Key(event) => self.global_key(event),
        }
    }

    fn global_key(&mut self, event: keyboard::Event) -> Task<Message> {
        let keyboard::Event::KeyPressed { key, modifiers, .. } = event else { return Task::none() };
        match key.as_ref() {
            Key::Character("o") if modifiers.command() => self.update(Message::OpenFolderDialog),
            Key::Character("s") if modifiers.command() => self.save(),
            Key::Character("w") if modifiers.command() => match self.active {
                Some(i) => self.update(Message::CloseTab(i)),
                None => Task::none(),
            },
            Key::Character("1") if modifiers.command() => self.update(Message::ToggleSidebar),
            Key::Named(Named::Escape) => {
                if self.completion.is_none() && self.hover.is_none() {
                    self.usages = None;
                }
                self.close_overlays();
                Task::none()
            }
            _ => Task::none(),
        }
    }

    fn close_overlays(&mut self) {
        self.completion = None;
        self.hover = None;
        self.notice = None;
    }

    /// Opens (or focuses) a file, optionally placing the cursor at `target`.
    fn open(&mut self, path: PathBuf, target: Option<Pos>) -> Task<Message> {
        let Some(i) = self.docs.iter().position(|d| d.path == path) else {
            let p = path.clone();
            return Task::perform(
                async move { tokio::fs::read_to_string(&p).await.map_err(|e| e.to_string()) },
                move |result| Message::FileLoaded(path.clone(), result, target),
            );
        };
        self.tree.reveal(&path);
        let select = self.select(i);
        let Some(target) = target else { return select };
        let doc = &mut self.docs[i];
        let position = doc.position_from_utf16(target);
        doc.move_to(position);
        // Put the target line a third of the way down the viewport.
        let y = (EDITOR_PAD_TOP + position.line as f32 * CODE_LINE - self.viewport_height / 3.0).max(0.0);
        doc.scroll_y = y;
        Task::batch([select, scroll_to(y), operation::focus(Id::new(EDITOR_ID))])
    }

    fn select(&mut self, i: usize) -> Task<Message> {
        if i >= self.docs.len() {
            return Task::none();
        }
        if self.active != Some(i) {
            self.close_overlays();
        }
        self.active = Some(i);
        let y = self.docs[i].scroll_y;
        Task::batch([scroll_to(y), operation::focus(Id::new(EDITOR_ID))])
    }

    fn edit(&mut self, action: Action) -> Task<Message> {
        let inserted = match &action {
            Action::Edit(Edit::Insert(c)) => Some(*c),
            _ => None,
        };
        let deleted = matches!(action, Action::Edit(Edit::Backspace | Edit::Delete));
        let clicked = matches!(action, Action::Click(_) | Action::Drag(_));
        let Some(doc) = self.doc_mut() else { return Task::none() };
        let changed = doc.perform(action);
        if changed && doc.is_php() {
            let (path, text) = (doc.path.clone(), doc.text.clone());
            self.lang.update_file(&path, &text);
        }
        self.hover = None;
        self.notice = None;

        if self.completion.is_some() {
            match (inserted, deleted) {
                (Some(c), _) if is_ident(c) || c == '$' => self.refresh_completion(false),
                (None, true) => self.refresh_completion(false),
                _ => self.completion = None,
            }
        } else if let Some(c) = inserted {
            if self.should_auto_complete(c) {
                self.refresh_completion(false);
            }
        }
        if clicked { Task::none() } else { self.reveal_cursor() }
    }

    fn should_auto_complete(&self, c: char) -> bool {
        let Some(doc) = self.doc() else { return false };
        if !doc.is_php() {
            return false;
        }
        let offset = doc.offset(doc.cursor());
        let before = &doc.text[..offset];
        match c {
            '>' => before.ends_with("->"),
            ':' => before.ends_with("::"),
            '$' => true,
            c if is_ident(c) => {
                // Start of a new word only, so the popup does not flicker per key.
                let word = phpthunder::analysis::ident_start(&doc.text, offset);
                offset - word == c.len_utf8()
            }
            _ => false,
        }
    }

    fn refresh_completion(&mut self, explicit: bool) {
        let Some(doc) = self.doc() else { return };
        let offset = doc.offset(doc.cursor());
        let mut start = phpthunder::analysis::ident_start(&doc.text, offset);
        if start > 0 && doc.text.as_bytes()[start - 1] == b'$' {
            start -= 1;
        }
        let items = self.lang.complete(&doc.text, offset);
        if items.is_empty() {
            self.completion = None;
            if explicit {
                self.notice = Some("No suggestions".into());
            }
            return;
        }
        let pos = doc.position_of(start);
        let (line, col) = (pos.line, doc.visual_column(pos));
        let selected = self.completion.as_ref().map(|c| c.selected).unwrap_or(0).min(items.len() - 1);
        self.completion = Some(Completion { items, selected, start, line, col });
    }

    fn accept_completion(&mut self) -> Task<Message> {
        let Some(completion) = self.completion.take() else { return Task::none() };
        let Some(item) = completion.items.get(completion.selected).cloned() else { return Task::none() };
        let Some(doc) = self.doc_mut() else { return Task::none() };
        let offset = doc.offset(doc.cursor());
        let mut start = completion.start.min(offset);
        // A variable candidate already carries `$`; a member never does.
        if !item.insert_text.starts_with('$') && doc.text[start..offset].starts_with('$') {
            start += 1;
        }
        doc.replace(start, offset, &item.insert_text);
        let after = doc.cursor();
        if let Some((at, text)) = &item.import {
            if *at <= start {
                doc.replace(*at, *at, text);
                let shift = text.matches('\n').count();
                doc.move_to(Position { line: after.line + shift, column: after.column });
            }
        }
        let (path, text) = (doc.path.clone(), doc.text.clone());
        self.lang.update_file(&path, &text);
        self.reveal_cursor()
    }

    fn reveal_cursor(&self) -> Task<Message> {
        let Some(doc) = self.doc() else { return Task::none() };
        let top = EDITOR_PAD_TOP + doc.cursor().line as f32 * CODE_LINE;
        let view = self.viewport_height;
        if top < doc.scroll_y {
            scroll_to((top - CODE_LINE).max(0.0))
        } else if top + CODE_LINE * 2.0 > doc.scroll_y + view {
            scroll_to(top + CODE_LINE * 2.0 - view)
        } else {
            Task::none()
        }
    }

    fn mouse_moved(&mut self, p: Point) -> Task<Message> {
        let line = ((p.y - EDITOR_PAD_TOP) / CODE_LINE).floor();
        let col = ((p.x - EDITOR_PAD_LEFT) / CHAR_WIDTH).floor();
        let at = (line >= 0.0 && col >= 0.0).then(|| (line as usize, col as usize));
        if at == self.hover_at {
            return Task::none();
        }
        self.hover_at = at;
        self.hover = None;
        self.hover_generation += 1;
        let generation = self.hover_generation;
        Task::perform(tokio::time::sleep(HOVER_DELAY), move |_| Message::HoverFire(generation))
    }

    fn show_hover(&mut self) {
        let (Some((line, col)), Some(doc)) = (self.hover_at, self.doc()) else { return };
        if !doc.is_php() || line >= doc.line_count() {
            return;
        }
        let pos = doc.position_at_visual(line, col);
        if doc.visual_column(pos) != col {
            return; // past the end of the line
        }
        let offset = doc.offset(pos);
        let Some(def) = self.lang.definitions(&doc.text, offset).into_iter().next() else { return };
        let s = &def.symbol;
        let start = phpthunder::analysis::ident_start(&doc.text, (offset + 1).min(doc.text.len()));
        let start_pos = doc.position_of(start);
        self.hover = Some(Hover {
            line,
            col: doc.visual_column(start_pos),
            signature: s.signature.clone(),
            location: format!("{} · {}:{}", qualified(s), file_name(&def.path), s.name_span.start.line + 1),
            doc: s.doc.clone(),
        });
    }

    fn goto_definition(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else { return Task::none() };
        let offset = doc.offset(doc.cursor());
        match self.lang.definitions(&doc.text, offset).into_iter().next() {
            Some(def) => {
                self.close_overlays();
                self.open(def.path, Some(def.symbol.name_span.start))
            }
            None => {
                self.notice = Some("No declaration found".into());
                Task::none()
            }
        }
    }

    fn find_usages(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else { return Task::none() };
        let offset = doc.offset(doc.cursor());
        let open: HashMap<PathBuf, String> =
            self.docs.iter().filter(|d| d.dirty).map(|d| (d.path.clone(), d.text.clone())).collect();
        let word_start = phpthunder::analysis::ident_start(&doc.text, offset);
        let word_end = doc.text[offset..]
            .find(|c: char| !is_ident(c))
            .map(|i| offset + i)
            .unwrap_or(doc.text.len());
        let name = doc.text[word_start..word_end].to_string();
        let text = doc.text.clone();
        self.usages = Some(Usages::Searching(name));
        self.close_overlays();
        Task::perform(self.lang.clone().usages(text, offset, open), Message::UsagesFound)
    }

    fn show_usages(&mut self, found: Option<(Definition, Vec<Usage>)>) {
        let Some((def, usages)) = found else {
            self.usages = None;
            self.notice = Some("Nothing to search: place the cursor on a symbol".into());
            return;
        };
        let mut cache: HashMap<PathBuf, String> = HashMap::new();
        let mut files = std::collections::HashSet::new();
        let rows: Vec<UsageRow> = usages
            .iter()
            .map(|u| {
                files.insert(u.path.clone());
                let text = cache.entry(u.path.clone()).or_insert_with(|| {
                    self.docs
                        .iter()
                        .find(|d| d.path == u.path)
                        .map(|d| d.text.clone())
                        .or_else(|| std::fs::read_to_string(&u.path).ok())
                        .unwrap_or_default()
                });
                let preview = text.lines().nth(u.span.start.line as usize).unwrap_or("").trim().to_string();
                UsageRow {
                    path: u.path.clone(),
                    pos: u.span.start,
                    label: format!("{}:{}", file_name(&u.path), u.span.start.line + 1),
                    preview,
                    is_declaration: u.is_declaration,
                }
            })
            .collect();
        let s = &def.symbol;
        let name = match s.kind {
            Kind::Method => format!("{}()", qualified(s)),
            _ => qualified(s),
        };
        let count = rows.iter().filter(|r| !r.is_declaration).count();
        let title = format!(
            "{name} — {count} usage{} in {} file{}",
            if count == 1 { "" } else { "s" },
            files.len(),
            if files.len() == 1 { "" } else { "s" }
        );
        self.usages = Some(Usages::Found { title, rows, selected: None });
    }

    fn save(&mut self) -> Task<Message> {
        let Some(doc) = self.doc_mut() else { return Task::none() };
        match std::fs::write(&doc.path, &doc.text) {
            Ok(()) => {
                doc.mark_saved();
                Task::none()
            }
            Err(e) => {
                let msg = format!("Could not save {}: {e}", doc.title());
                self.notice = Some(msg);
                Task::none()
            }
        }
    }

    /// Namespace › Class › member for the cursor position.
    pub fn breadcrumb(&self) -> Vec<String> {
        let Some(doc) = self.doc() else { return Vec::new() };
        let line = doc.cursor().line as u32;
        let symbols = self.lang.file_symbols(&doc.path);
        let contains = |s: &Symbol| s.span.start.line <= line && line <= s.span.end.line;
        let mut out = Vec::new();
        if let Some(class) = symbols.iter().find(|s| s.kind.is_class_like() && contains(s)) {
            if let Some((ns, short)) = class.fqn.rsplit_once('\\') {
                out.push(ns.to_string());
                out.push(short.to_string());
            } else {
                out.push(class.fqn.clone());
            }
            if let Some(m) = symbols.iter().find(|s| s.kind == Kind::Method && !s.is_virtual && contains(s)) {
                out.push(format!("{}()", m.name));
            }
        } else if let Some(f) = symbols.iter().find(|s| s.kind == Kind::Function && contains(s)) {
            out.push(format!("{}()", f.fqn));
        }
        out
    }
}

fn scroll_to(y: f32) -> Task<Message> {
    operation::scroll_to(Id::new(EDITOR_SCROLL_ID), AbsoluteOffset { x: None, y: Some(y) })
}

pub fn file_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn folder_name(path: &std::path::Path) -> String {
    file_name(path)
}

fn qualified(s: &Symbol) -> String {
    if s.kind.is_member() {
        let owner = s.fqn.rsplit('\\').next().unwrap_or(&s.fqn);
        format!("{owner}::{}", s.name)
    } else {
        s.fqn.clone()
    }
}
