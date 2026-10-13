//! Application state, messages, and the update loop.

use crate::document::Document;
use crate::find::{FindBar, SearchDialog, SearchResults, Toggle};
use crate::keymap::{self, Cmd};
use crate::language::{IndexReport, Language};
use crate::palette::Palette;
use crate::theme::{CHAR_WIDTH, CODE_LINE, EDITOR_PAD_LEFT, EDITOR_PAD_TOP};
use crate::tree::{FileTree, git_branch};
use iced::event::{self, Event};
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::widget::Id;
use iced::widget::operation::{self, AbsoluteOffset};
use iced::widget::scrollable::Viewport;
use iced::widget::text_editor::{Action, Binding, Edit, KeyPress, Position};
use iced::{Point, Subscription, Task};
use phpthunder::analysis::{Candidate, Definition, Usage};
use phpthunder::stubs::{Kind, Symbol};
use phpthunder::text::Pos;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const EDITOR_ID: &str = "editor";
pub const EDITOR_SCROLL_ID: &str = "editor-scroll";
const HOVER_DELAY: Duration = Duration::from_millis(450);
const DOUBLE_SHIFT: Duration = Duration::from_millis(400);

#[derive(Debug, Clone)]
pub enum Message {
    // Project and files
    OpenFolderDialog,
    FolderPicked(Option<PathBuf>),
    FilesListed(Vec<PathBuf>),
    Indexed(IndexReport),
    ToggleDir(PathBuf),
    OpenFile(PathBuf),
    FileLoaded(PathBuf, Result<String, String>, Option<Pos>),
    SelectTab(usize),
    CloseTab(usize),
    // Editor
    Edit(Action),
    Command(Cmd),
    TypeChar(char),
    SmartEnter,
    Backspace,
    Indent(bool),
    Scrolled(Viewport),
    CompletionMove(i32),
    CompletionAccept,
    CompletionPick(usize),
    CloseOverlays,
    MouseMoved(Point),
    MouseLeft,
    HoverFire(u64),
    // Usages and bottom panel
    UsagesFound(Option<(Definition, Vec<Usage>)>),
    OpenUsage(usize),
    SelectBottom(Bottom),
    CloseBottom,
    // Palette
    PaletteQuery(String),
    PaletteSubmit,
    PalettePick(usize),
    // Find in file
    FindQuery(String),
    ReplaceText(String),
    FindToggle(Toggle),
    FindStep(bool),
    ToggleReplace,
    ReplaceOne,
    ReplaceAll,
    CloseFind,
    // Find in path
    SearchQuery(String),
    SearchMask(String),
    SearchToggle(Toggle),
    SearchSubmit,
    SearchDone(Result<SearchResults, String>),
    OpenSearchHit(usize, usize),
    CloseModal,
    // Window
    ToggleSidebar,
    DragWindow,
    ToggleMaximize,
    Keyboard(keyboard::Event, bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bottom {
    Usages,
    Find,
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
    Found {
        title: String,
        rows: Vec<UsageRow>,
        selected: Option<usize>,
    },
}

pub enum Search {
    Running(String),
    Done(SearchResults, Option<(usize, usize)>),
}

/// A place in a file, for back/forward navigation.
#[derive(Debug, Clone)]
pub struct Location {
    pub path: PathBuf,
    pub pos: Pos,
}

pub struct App {
    pub lang: Language,
    pub tree: FileTree,
    pub branch: Option<String>,
    pub project_files: Vec<PathBuf>,
    pub vendor_files: Vec<PathBuf>,
    pub docs: Vec<Document>,
    pub active: Option<usize>,
    pub recent: Vec<PathBuf>,
    pub indexing: Indexing,
    pub sidebar: bool,
    pub viewport_height: f32,
    pub completion: Option<Completion>,
    pub hover: Option<Hover>,
    hover_generation: u64,
    hover_at: Option<(usize, usize)>,
    pub usages: Option<Usages>,
    pub search: Option<Search>,
    pub bottom: Option<Bottom>,
    pub palette: Option<Palette>,
    pub find: Option<FindBar>,
    pub search_dialog: Option<SearchDialog>,
    pub nav_back: Vec<Location>,
    pub nav_forward: Vec<Location>,
    pub notice: Option<String>,
    pub modifiers: Modifiers,
    last_shift: Option<Instant>,
    /// Settings of the last find-in-path, restored when the dialog reopens.
    pub last_search: Option<SearchDialog>,
    /// A selection to apply once its file finishes loading.
    pub pending_select: Option<(PathBuf, usize, usize, usize)>,
}

pub fn is_ident(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Editor key bindings: popups first, then IDE shortcuts, then smart
/// editing keys, then iced's defaults.
pub fn bindings(key: KeyPress, completion_open: bool) -> Option<Binding<Message>> {
    let m = key.modifiers;
    let custom = |msg| Some(Binding::Custom(msg));
    if completion_open {
        match key.key.as_ref() {
            Key::Named(Named::ArrowDown) => return custom(Message::CompletionMove(1)),
            Key::Named(Named::ArrowUp) => return custom(Message::CompletionMove(-1)),
            Key::Named(Named::Enter | Named::Tab) => return custom(Message::CompletionAccept),
            Key::Named(Named::Escape) => return custom(Message::CloseOverlays),
            _ => {}
        }
    }
    if let Some(cmd) = keymap::command(&key.key, m) {
        return custom(Message::Command(cmd));
    }
    let plain = !m.command() && !m.control() && !m.alt();
    match key.key.as_ref() {
        Key::Named(Named::Enter) if plain && !m.shift() => return custom(Message::SmartEnter),
        Key::Named(Named::Backspace) if plain => return custom(Message::Backspace),
        Key::Named(Named::Tab) => return custom(Message::Indent(m.shift())),
        Key::Named(Named::Escape) => return custom(Message::CloseOverlays),
        _ => {}
    }
    if plain
        && let Some(c) = key.text.as_deref().and_then(|t| t.chars().next())
        && "([{\"')]}".contains(c)
    {
        return custom(Message::TypeChar(c));
    }
    Binding::from_key_press(key)
}

fn keyboard_events(
    event: Event,
    status: event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        Event::Keyboard(e) => Some(Message::Keyboard(e, status == event::Status::Captured)),
        _ => None,
    }
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let app = Self {
            lang: Language::default(),
            tree: FileTree::default(),
            branch: None,
            project_files: Vec::new(),
            vendor_files: Vec::new(),
            docs: Vec::new(),
            active: None,
            recent: Vec::new(),
            indexing: Indexing::Idle,
            sidebar: true,
            viewport_height: 700.0,
            completion: None,
            hover: None,
            hover_generation: 0,
            hover_at: None,
            usages: None,
            search: None,
            bottom: None,
            palette: None,
            find: None,
            search_dialog: None,
            nav_back: Vec::new(),
            nav_forward: Vec::new(),
            notice: None,
            modifiers: Modifiers::default(),
            last_shift: None,
            last_search: None,
            pending_select: None,
        };
        let folder = std::env::args()
            .nth(1)
            .map(PathBuf::from)
            .filter(|p| p.is_dir());
        let task = match folder {
            Some(dir) => Task::done(Message::FolderPicked(Some(dir))),
            None => Task::none(),
        };
        (app, task)
    }

    pub fn title(&self) -> String {
        match (&self.tree.root, self.doc()) {
            (Some(root), Some(doc)) => {
                format!("{} — {} — phpthunder", doc.title(), file_name(root))
            }
            (Some(root), None) => format!("{} — phpthunder", file_name(root)),
            _ => "phpthunder".into(),
        }
    }

    pub fn doc(&self) -> Option<&Document> {
        self.active.and_then(|i| self.docs.get(i))
    }

    pub fn doc_mut(&mut self) -> Option<&mut Document> {
        self.active.and_then(|i| self.docs.get_mut(i))
    }

    pub fn subscription(&self) -> Subscription<Message> {
        event::listen_with(keyboard_events)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenFolderDialog => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .pick_folder()
                        .await
                        .map(|h| h.path().to_path_buf())
                },
                Message::FolderPicked,
            ),
            Message::FolderPicked(None) => Task::none(),
            Message::FolderPicked(Some(root)) => self.open_folder(root),
            Message::FilesListed(files) => {
                let (vendor, project): (Vec<PathBuf>, Vec<PathBuf>) = files
                    .into_iter()
                    .partition(|p| p.components().any(|c| c.as_os_str() == "vendor"));
                self.project_files = project;
                self.vendor_files = vendor;
                Task::none()
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
            Message::OpenFile(path) => {
                self.record_nav();
                self.open(path, None)
            }
            Message::FileLoaded(path, result, target) => match result {
                Ok(text) => {
                    if !self.docs.iter().any(|d| d.path == path) {
                        self.docs.push(Document::new(path.clone(), text));
                    }
                    let open = self.open(path, target);
                    Task::batch([open, self.apply_pending_select()])
                }
                Err(e) => {
                    self.notice = Some(format!("Could not open {}: {e}", path.display()));
                    Task::none()
                }
            },
            Message::SelectTab(i) => self.select(i),
            Message::CloseTab(i) => self.close_tab(i),
            Message::Edit(action) => self.edit(action),
            Message::Command(cmd) => self.command(cmd),
            Message::TypeChar('}') => self.edit_with(|d| d.type_close_brace(), Some('}')),
            Message::TypeChar(c) => self.edit_with(|d| d.type_pair_char(c), Some(c)),
            Message::SmartEnter => self.edit_with(|d| d.smart_enter(), None),
            Message::Backspace => {
                let task = self.edit_with(|d| d.backspace(), None);
                if self.completion.is_some() {
                    self.refresh_completion(false);
                }
                task
            }
            Message::Indent(outdent) => self.edit_with(|d| d.indent(outdent), None),
            Message::Scrolled(viewport) => {
                self.viewport_height = viewport.bounds().height;
                let y = viewport.absolute_offset().y;
                if let Some(doc) = self.doc_mut() {
                    doc.scroll_y = y;
                }
                self.hover = None;
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
                if self.find.is_some() {
                    self.find = None;
                }
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
            Message::UsagesFound(found) => {
                self.show_usages(found);
                Task::none()
            }
            Message::OpenUsage(i) => {
                let Some(Usages::Found { rows, selected, .. }) = &mut self.usages else {
                    return Task::none();
                };
                let Some(row) = rows.get(i) else {
                    return Task::none();
                };
                *selected = Some(i);
                let (path, pos) = (row.path.clone(), row.pos);
                self.record_nav();
                self.open(path, Some(pos))
            }
            Message::SelectBottom(b) => {
                self.bottom = Some(b);
                Task::none()
            }
            Message::CloseBottom => {
                self.bottom = None;
                Task::none()
            }
            Message::PaletteQuery(q) => self.palette_query(q),
            Message::PaletteSubmit => self.palette_open_selected(),
            Message::PalettePick(i) => {
                if let Some(p) = &mut self.palette {
                    p.selected = i;
                }
                self.palette_open_selected()
            }
            Message::FindQuery(q) => self.find_query(q),
            Message::ReplaceText(r) => {
                if let Some(f) = &mut self.find {
                    f.replacement = r;
                }
                Task::none()
            }
            Message::FindToggle(t) => {
                if let Some(f) = &mut self.find {
                    f.options.toggle(t);
                }
                self.refresh_find(true)
            }
            Message::FindStep(forward) => self.find_step(forward),
            Message::ToggleReplace => {
                if let Some(f) = &mut self.find {
                    f.show_replace = !f.show_replace;
                }
                Task::none()
            }
            Message::ReplaceOne => self.replace_one(),
            Message::ReplaceAll => self.replace_all(),
            Message::CloseFind => {
                self.find = None;
                operation::focus(Id::new(EDITOR_ID))
            }
            Message::SearchQuery(q) => {
                if let Some(d) = &mut self.search_dialog {
                    d.query = q;
                    d.error = None;
                }
                Task::none()
            }
            Message::SearchMask(m) => {
                if let Some(d) = &mut self.search_dialog {
                    d.mask = m;
                }
                Task::none()
            }
            Message::SearchToggle(t) => {
                if let Some(d) = &mut self.search_dialog {
                    if t == Toggle::Vendor {
                        d.include_vendor = !d.include_vendor;
                    } else {
                        d.options.toggle(t);
                    }
                }
                Task::none()
            }
            Message::SearchSubmit => self.run_search(),
            Message::SearchDone(result) => {
                match result {
                    Ok(results) => self.search = Some(Search::Done(results, None)),
                    Err(e) => {
                        self.search = None;
                        self.notice = Some(format!("Invalid pattern: {e}"));
                    }
                }
                Task::none()
            }
            Message::OpenSearchHit(f, h) => self.open_search_hit(f, h),
            Message::CloseModal => {
                self.palette = None;
                self.search_dialog = None;
                operation::focus(Id::new(EDITOR_ID))
            }
            Message::ToggleSidebar => {
                self.sidebar = !self.sidebar;
                Task::none()
            }
            Message::DragWindow => iced::window::latest().and_then(iced::window::drag),
            Message::ToggleMaximize => {
                iced::window::latest().and_then(iced::window::toggle_maximize)
            }
            Message::Keyboard(event, captured) => self.keyboard(event, captured),
        }
    }

    fn keyboard(&mut self, event: keyboard::Event, captured: bool) -> Task<Message> {
        match event {
            keyboard::Event::ModifiersChanged(m) => {
                let shift_down = m.shift()
                    && !self.modifiers.shift()
                    && !m.command()
                    && !m.alt()
                    && !m.control();
                self.modifiers = m;
                if shift_down {
                    let now = Instant::now();
                    if self.last_shift.is_some_and(|t| now - t < DOUBLE_SHIFT) {
                        self.last_shift = None;
                        return self.command(Cmd::SearchEverywhere);
                    }
                    self.last_shift = Some(now);
                }
                Task::none()
            }
            keyboard::Event::KeyPressed { key, modifiers, .. } => {
                self.last_shift = None;
                // Modal popups own arrows and Escape even while their input has focus.
                if self.palette.is_some() {
                    return match key.as_ref() {
                        Key::Named(Named::ArrowDown) => self.palette_move(1),
                        Key::Named(Named::ArrowUp) => self.palette_move(-1),
                        Key::Named(Named::Escape) => self.update(Message::CloseModal),
                        _ => Task::none(),
                    };
                }
                if self.search_dialog.is_some() {
                    return match key.as_ref() {
                        Key::Named(Named::Escape) => self.update(Message::CloseModal),
                        _ => Task::none(),
                    };
                }
                if self.find.is_some() && matches!(key.as_ref(), Key::Named(Named::Escape)) {
                    return self.update(Message::CloseFind);
                }
                if self.find.is_some()
                    && captured
                    && matches!(key.as_ref(), Key::Named(Named::Enter))
                    && modifiers.shift()
                {
                    return self.find_step(false);
                }
                if captured {
                    return Task::none();
                }
                if let Some(cmd) = keymap::command(&key, modifiers) {
                    return self.command(cmd);
                }
                if matches!(key.as_ref(), Key::Named(Named::Escape)) {
                    if self.completion.is_none() && self.hover.is_none() {
                        self.bottom = None;
                    }
                    self.close_overlays();
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    fn open_folder(&mut self, root: PathBuf) -> Task<Message> {
        let root = root.canonicalize().unwrap_or(root);
        self.tree = FileTree::open(root.clone());
        self.branch = git_branch(&root);
        self.docs.clear();
        self.active = None;
        self.recent.clear();
        self.nav_back.clear();
        self.nav_forward.clear();
        self.close_overlays();
        self.usages = None;
        self.search = None;
        self.bottom = None;
        self.indexing = Indexing::Running;
        let list_root = root.clone();
        Task::batch([
            Task::perform(self.lang.clone().index_project(root), Message::Indexed),
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || crate::tree::list_files(&list_root))
                        .await
                        .unwrap_or_default()
                },
                Message::FilesListed,
            ),
        ])
    }

    pub fn close_overlays(&mut self) {
        self.completion = None;
        self.hover = None;
        self.notice = None;
    }

    // ----- files and tabs ---------------------------------------------------

    /// Opens (or focuses) a file, optionally placing the cursor at `target`.
    pub fn open(&mut self, path: PathBuf, target: Option<Pos>) -> Task<Message> {
        let Some(i) = self.docs.iter().position(|d| d.path == path) else {
            let p = path.clone();
            return Task::perform(
                async move {
                    tokio::fs::read_to_string(&p)
                        .await
                        .map_err(|e| e.to_string())
                },
                move |result| Message::FileLoaded(path.clone(), result, target),
            );
        };
        self.tree.reveal(&path);
        let select = self.select(i);
        let Some(target) = target else { return select };
        let doc = &mut self.docs[i];
        let position = doc.position_from_utf16(target);
        Task::batch([select, self.place_cursor(position)])
    }

    /// Moves the cursor of the active document, scrolling it a third of the
    /// way down the viewport.
    pub fn place_cursor(&mut self, position: Position) -> Task<Message> {
        let view = self.viewport_height;
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        doc.move_to(position);
        let y = (EDITOR_PAD_TOP + position.line as f32 * CODE_LINE - view / 3.0).max(0.0);
        doc.scroll_y = y;
        Task::batch([scroll_to(y), operation::focus(Id::new(EDITOR_ID))])
    }

    pub fn select(&mut self, i: usize) -> Task<Message> {
        if i >= self.docs.len() {
            return Task::none();
        }
        if self.active != Some(i) {
            self.close_overlays();
        }
        self.active = Some(i);
        let path = self.docs[i].path.clone();
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(50);
        let task = self.refresh_find(false);
        let y = self.docs[i].scroll_y;
        Task::batch([task, scroll_to(y), operation::focus(Id::new(EDITOR_ID))])
    }

    fn close_tab(&mut self, i: usize) -> Task<Message> {
        if i >= self.docs.len() {
            return Task::none();
        }
        self.docs.remove(i);
        self.close_overlays();
        let next = match self.active {
            _ if self.docs.is_empty() => None,
            Some(a) if a > i => Some(a - 1),
            Some(a) => Some(a.min(self.docs.len() - 1)),
            None => None,
        };
        self.active = None;
        match next {
            Some(n) => self.select(n),
            None => Task::none(),
        }
    }

    pub fn save(&mut self) -> Task<Message> {
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        if let Err(e) = std::fs::write(&doc.path, &doc.text) {
            let msg = format!("Could not save {}: {e}", doc.title());
            self.notice = Some(msg);
        } else {
            doc.mark_saved();
        }
        Task::none()
    }

    // ----- navigation history -------------------------------------------------

    pub fn current_location(&self) -> Option<Location> {
        let doc = self.doc()?;
        Some(Location {
            path: doc.path.clone(),
            pos: doc.utf16_pos(doc.cursor()),
        })
    }

    /// Remembers where we are before a jump.
    pub fn record_nav(&mut self) {
        if let Some(loc) = self.current_location() {
            if self
                .nav_back
                .last()
                .is_none_or(|l| l.path != loc.path || l.pos.line != loc.pos.line)
            {
                self.nav_back.push(loc);
                if self.nav_back.len() > 100 {
                    self.nav_back.remove(0);
                }
            }
            self.nav_forward.clear();
        }
    }

    pub fn navigate(&mut self, back: bool) -> Task<Message> {
        let (from, to) = if back {
            (&mut self.nav_back, &mut self.nav_forward)
        } else {
            (&mut self.nav_forward, &mut self.nav_back)
        };
        let Some(target) = from.pop() else {
            return Task::none();
        };
        let here = {
            let doc = self.active.and_then(|i| self.docs.get(i));
            doc.map(|d| Location {
                path: d.path.clone(),
                pos: d.utf16_pos(d.cursor()),
            })
        };
        if let Some(here) = here {
            to.push(here);
        }
        self.open(target.path, Some(target.pos))
    }

    // ----- editing ----------------------------------------------------------

    pub fn edit(&mut self, action: Action) -> Task<Message> {
        let inserted = match &action {
            Action::Edit(Edit::Insert(c)) => Some(*c),
            _ => None,
        };
        let deleted = matches!(action, Action::Edit(Edit::Backspace | Edit::Delete));
        let clicked = matches!(action, Action::Click(_));
        let dragged = matches!(action, Action::Drag(_));
        let goto = clicked && self.modifiers.command();
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        let changed = doc.perform(action);
        self.after_change(changed);

        if self.completion.is_some() {
            match (inserted, deleted) {
                (Some(c), _) if is_ident(c) || c == '$' => self.refresh_completion(false),
                (None, true) => self.refresh_completion(false),
                _ => self.completion = None,
            }
        } else if let Some(c) = inserted
            && self.should_auto_complete(c)
        {
            self.refresh_completion(false);
        }
        if goto {
            return self.goto_definition();
        }
        if clicked || dragged {
            Task::none()
        } else {
            self.reveal_cursor()
        }
    }

    /// Runs a document operation as an edit (index, find matches, popups).
    pub fn edit_with(
        &mut self,
        op: impl FnOnce(&mut Document),
        typed: Option<char>,
    ) -> Task<Message> {
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        let before = doc.highlight_version;
        op(doc);
        let changed = doc.highlight_version != before;
        self.after_change(changed);
        match typed {
            Some(c) if self.completion.is_some() && !is_ident(c) => self.completion = None,
            _ if typed.is_none() => self.completion = None,
            _ => {}
        }
        self.reveal_cursor()
    }

    /// Bookkeeping after the active document changed.
    pub fn after_change(&mut self, changed: bool) {
        self.hover = None;
        self.notice = None;
        if !changed {
            return;
        }
        if let Some(doc) = self.doc()
            && doc.is_php()
        {
            self.lang.update_file(&doc.path.clone(), &doc.text.clone());
        }
        let _ = self.refresh_find(false);
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

    pub fn refresh_completion(&mut self, explicit: bool) {
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
        let selected = self
            .completion
            .as_ref()
            .map(|c| c.selected)
            .unwrap_or(0)
            .min(items.len() - 1);
        self.completion = Some(Completion {
            items,
            selected,
            start,
            line,
            col,
        });
    }

    fn accept_completion(&mut self) -> Task<Message> {
        let Some(completion) = self.completion.take() else {
            return Task::none();
        };
        let Some(item) = completion.items.get(completion.selected).cloned() else {
            return Task::none();
        };
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let mut start = completion.start.min(offset);
        // A variable candidate already carries `$`; a member never does.
        if !item.insert_text.starts_with('$') && doc.text[start..offset].starts_with('$') {
            start += 1;
        }
        doc.checkpoint();
        doc.replace(start, offset, &item.insert_text);
        let after = doc.cursor();
        if let Some((at, text)) = &item.import
            && *at <= start
        {
            doc.replace(*at, *at, text);
            let shift = text.matches('\n').count();
            doc.move_to(Position {
                line: after.line + shift,
                column: after.column,
            });
        }
        self.after_change(true);
        self.reveal_cursor()
    }

    pub fn reveal_cursor(&self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
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

    // ----- hover --------------------------------------------------------------

    fn mouse_moved(&mut self, p: Point) -> Task<Message> {
        let line = ((p.y - EDITOR_PAD_TOP) / CODE_LINE).floor();
        let col = ((p.x - EDITOR_PAD_LEFT) / CHAR_WIDTH).floor();
        let at = (line >= 0.0 && col >= 0.0).then_some((line as usize, col as usize));
        if at == self.hover_at {
            return Task::none();
        }
        self.hover_at = at;
        self.hover = None;
        self.hover_generation += 1;
        let generation = self.hover_generation;
        Task::perform(tokio::time::sleep(HOVER_DELAY), move |_| {
            Message::HoverFire(generation)
        })
    }

    fn show_hover(&mut self) {
        let (Some((line, col)), Some(doc)) = (self.hover_at, self.doc()) else {
            return;
        };
        if !doc.is_php() || line >= doc.line_count() {
            return;
        }
        let pos = doc.position_at_visual(line, col);
        if doc.visual_column(pos) != col {
            return; // past the end of the line
        }
        let offset = doc.offset(pos);
        let Some(def) = self.lang.definitions(&doc.text, offset).into_iter().next() else {
            return;
        };
        let s = &def.symbol;
        let start = phpthunder::analysis::ident_start(&doc.text, (offset + 1).min(doc.text.len()));
        let start_pos = doc.position_of(start);
        self.hover = Some(Hover {
            line,
            col: doc.visual_column(start_pos),
            signature: s.signature.clone(),
            location: format!(
                "{} · {}:{}",
                qualified(s),
                file_name(&def.path),
                s.name_span.start.line + 1
            ),
            doc: s.doc.clone(),
        });
    }

    // ----- declarations and usages --------------------------------------------

    pub fn goto_definition(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        match self.lang.definitions(&doc.text, offset).into_iter().next() {
            Some(def) => {
                self.close_overlays();
                self.record_nav();
                self.open(def.path, Some(def.symbol.name_span.start))
            }
            None => {
                self.notice = Some("No declaration found".into());
                Task::none()
            }
        }
    }

    pub fn find_usages(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let open = self.open_texts();
        let word_start = phpthunder::analysis::ident_start(&doc.text, offset);
        let word_end = doc.text[offset..]
            .find(|c: char| !is_ident(c))
            .map(|i| offset + i)
            .unwrap_or(doc.text.len());
        let name = doc.text[word_start..word_end].to_string();
        let text = doc.text.clone();
        self.usages = Some(Usages::Searching(name));
        self.bottom = Some(Bottom::Usages);
        self.close_overlays();
        Task::perform(
            self.lang.clone().usages(text, offset, open),
            Message::UsagesFound,
        )
    }

    /// Unsaved editor buffers, which win over disk content.
    pub fn open_texts(&self) -> std::collections::HashMap<PathBuf, String> {
        self.docs
            .iter()
            .filter(|d| d.dirty)
            .map(|d| (d.path.clone(), d.text.clone()))
            .collect()
    }

    pub fn file_text(&self, path: &Path) -> Option<String> {
        self.docs
            .iter()
            .find(|d| d.path == path)
            .map(|d| d.text.clone())
            .or_else(|| std::fs::read_to_string(path).ok())
    }

    fn show_usages(&mut self, found: Option<(Definition, Vec<Usage>)>) {
        let Some((def, usages)) = found else {
            self.usages = None;
            self.bottom = None;
            self.notice = Some("Nothing to search: place the cursor on a symbol".into());
            return;
        };
        let mut cache: std::collections::HashMap<PathBuf, String> =
            std::collections::HashMap::new();
        let mut files = std::collections::HashSet::new();
        let rows: Vec<UsageRow> = usages
            .iter()
            .map(|u| {
                files.insert(u.path.clone());
                let text = cache
                    .entry(u.path.clone())
                    .or_insert_with(|| self.file_text(&u.path).unwrap_or_default());
                let preview = text
                    .lines()
                    .nth(u.span.start.line as usize)
                    .unwrap_or("")
                    .trim()
                    .to_string();
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
        self.usages = Some(Usages::Found {
            title,
            rows,
            selected: None,
        });
    }

    /// Namespace › Class › member for the cursor position.
    pub fn breadcrumb(&self) -> Vec<String> {
        let Some(doc) = self.doc() else {
            return Vec::new();
        };
        let line = doc.cursor().line as u32;
        let symbols = self.lang.file_symbols(&doc.path);
        let contains = |s: &Symbol| s.span.start.line <= line && line <= s.span.end.line;
        let mut out = Vec::new();
        if let Some(class) = symbols
            .iter()
            .find(|s| s.kind.is_class_like() && contains(s))
        {
            if let Some((ns, short)) = class.fqn.rsplit_once('\\') {
                out.push(ns.to_string());
                out.push(short.to_string());
            } else {
                out.push(class.fqn.clone());
            }
            if let Some(m) = symbols
                .iter()
                .find(|s| s.kind == Kind::Method && !s.is_virtual && contains(s))
            {
                out.push(format!("{}()", m.name));
            }
        } else if let Some(f) = symbols
            .iter()
            .find(|s| s.kind == Kind::Function && contains(s))
        {
            out.push(format!("{}()", f.fqn));
        }
        if out.is_empty() {
            // Outside any declaration: show where the file lives instead.
            let rel = self
                .tree
                .root
                .as_ref()
                .and_then(|r| doc.path.strip_prefix(r).ok())
                .unwrap_or(&doc.path);
            out = rel
                .iter()
                .map(|c| c.to_string_lossy().into_owned())
                .collect();
        }
        out
    }
}

pub fn scroll_to(y: f32) -> Task<Message> {
    operation::scroll_to(
        Id::new(EDITOR_SCROLL_ID),
        AbsoluteOffset {
            x: None,
            y: Some(y),
        },
    )
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn qualified(s: &Symbol) -> String {
    if s.kind.is_member() {
        let owner = s.fqn.rsplit('\\').next().unwrap_or(&s.fqn);
        format!("{owner}::{}", s.name)
    } else {
        s.fqn.clone()
    }
}
