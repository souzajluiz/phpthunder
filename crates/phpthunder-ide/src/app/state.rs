//! The application state and its update loop: files, tabs, editing,
//! completion, hover, navigation history, declarations and usages.

use super::*;
use crate::editor::Document;
use crate::keymap::{self, Cmd};
use crate::language::Language;
use crate::palette::Palette;
use crate::project::{FileTree, git_branch};
use crate::search::{FindBar, SearchDialog, Toggle};
use crate::ui::{CHAR_WIDTH, CODE_LINE, EDITOR_PAD_LEFT, EDITOR_PAD_TOP};
use iced::event::{self, Event};
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::widget::Id;
use iced::widget::operation::{self, AbsoluteOffset};
use iced::widget::text_editor::{Action, Binding, Edit, KeyPress, Position};
use iced::{Point, Subscription, Task};
use phpthunder::analysis::{Definition, Usage};
use phpthunder::symbol::{Kind, Symbol};
use phpthunder::syntax::Pos;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Widget id of the code `text_editor`; used to re-focus it after popups,
/// dialogs, and jumps close.
pub const EDITOR_ID: &str = "editor";

/// Widget id of the scrollable wrapping the editor; target of [`scroll_to`].
pub const EDITOR_SCROLL_ID: &str = "editor-scroll";

/// How long the mouse must rest on a symbol before its hover popup shows.
const HOVER_DELAY: Duration = Duration::from_millis(450);

/// Maximum gap between two bare Shift presses to trigger Search Everywhere.
const DOUBLE_SHIFT: Duration = Duration::from_millis(400);

/// Whole IDE state; iced's `boot`/`update`/`view` entry points live on it.
pub struct App {
    /// Language service: index, completion, definitions, usages.
    pub lang: Language,
    /// Project file tree in the sidebar.
    pub tree: FileTree,
    /// Current git branch of the project, if it is a repository.
    pub branch: Option<String>,
    /// Project files outside `vendor/`, used by the palette and find-in-path.
    pub project_files: Vec<PathBuf>,
    /// Files under `vendor/`, searched only when requested.
    pub vendor_files: Vec<PathBuf>,
    /// Open editor tabs.
    pub docs: Vec<Document>,
    /// Index of the active tab in `docs`.
    pub active: Option<usize>,
    /// Recently activated files, most recent first (max 50).
    pub recent: Vec<PathBuf>,
    /// Background indexing state.
    pub indexing: Indexing,
    /// Whether the project sidebar is visible.
    pub sidebar: bool,
    /// Last known editor viewport height, for scroll math.
    pub viewport_height: f32,
    /// Open completion popup.
    pub completion: Option<Completion>,
    /// Visible hover popup.
    pub hover: Option<Hover>,
    /// Bumped on every mouse move so stale hover timers are ignored.
    hover_generation: u64,
    /// Visual (line, column) under the mouse, if over text.
    hover_at: Option<(usize, usize)>,
    /// Usages panel content.
    pub usages: Option<Usages>,
    /// Find-in-path panel content.
    pub search: Option<Search>,
    /// Visible bottom panel tab, `None` when hidden.
    pub bottom: Option<Bottom>,
    /// Open command/file palette.
    pub palette: Option<Palette>,
    /// Open find/replace bar for the active document.
    pub find: Option<FindBar>,
    /// Open find-in-path dialog.
    pub search_dialog: Option<SearchDialog>,
    /// Back navigation stack (max 100).
    pub nav_back: Vec<Location>,
    /// Forward navigation stack; cleared on every new jump.
    pub nav_forward: Vec<Location>,
    /// Transient toast message; cleared on the next edit or Escape.
    pub notice: Option<String>,
    /// Currently held keyboard modifiers.
    pub modifiers: Modifiers,
    /// When a bare Shift was last pressed, for double-Shift detection.
    last_shift: Option<Instant>,
    /// Settings of the last find-in-path, restored when the dialog reopens.
    pub last_search: Option<SearchDialog>,
    /// A selection to apply once its file finishes loading.
    pub pending_select: Option<(PathBuf, usize, usize, usize)>,
    /// Open `⌥↵` menu.
    pub intentions: Option<Intentions>,
    /// Open parameter info popup.
    pub signature: Option<SignatureView>,
    /// Open rename dialog.
    pub rename: Option<RenameDialog>,
    /// Content of the type hierarchy panel.
    pub hierarchy: Option<Hierarchy>,
    /// Set by edits and tab switches; [`App::update`] turns it into a
    /// debounced inspection run.
    pub inspect_pending: bool,
    /// Generation of the latest inspection timer; older timers do nothing.
    pub inspect_generation: u64,
}

/// Whether `c` can be part of a PHP identifier; used to decide when
/// completion keeps filtering, auto-opens, or closes.
pub fn is_ident(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Editor key bindings: popups first, then IDE shortcuts, then smart
/// editing keys, then iced's defaults.
pub fn bindings(
    key: KeyPress,
    completion_open: bool,
    intentions_open: bool,
) -> Option<Binding<Message>> {
    let m = key.modifiers;
    let custom = |msg| Some(Binding::Custom(msg));
    if intentions_open {
        match key.key.as_ref() {
            Key::Named(Named::ArrowDown) => return custom(Message::IntentionMove(1)),
            Key::Named(Named::ArrowUp) => return custom(Message::IntentionMove(-1)),
            Key::Named(Named::Enter | Named::Tab) => return custom(Message::IntentionAccept),
            Key::Named(Named::Escape) => return custom(Message::CloseOverlays),
            _ => {}
        }
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

/// Global event filter for [`App::subscription`]: forwards keyboard events
/// as [`Message::Keyboard`], tagging whether a focused widget already
/// captured them, and drops everything else.
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
    /// Builds the initial empty state; iced calls it once at startup. If the
    /// first CLI argument is a directory, returns a task that opens it as the
    /// project via [`Message::FolderPicked`].
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
            intentions: None,
            signature: None,
            rename: None,
            hierarchy: None,
            inspect_pending: false,
            inspect_generation: 0,
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

    /// Window title: `file — project — phpthunder`, dropping parts that do
    /// not exist yet. Polled by iced after every update.
    pub fn title(&self) -> String {
        match (&self.tree.root, self.doc()) {
            (Some(root), Some(doc)) => {
                format!("{} — {} — phpthunder", doc.title(), file_name(root))
            }
            (Some(root), None) => format!("{} — phpthunder", file_name(root)),
            _ => "phpthunder".into(),
        }
    }

    /// Active editor tab, if any; the target of most editor commands.
    pub fn doc(&self) -> Option<&Document> {
        self.active.and_then(|i| self.docs.get(i))
    }

    /// Mutable access to the active editor tab, if any.
    pub fn doc_mut(&mut self) -> Option<&mut Document> {
        self.active.and_then(|i| self.docs.get_mut(i))
    }

    /// Listens to every keyboard event so shortcuts, double-Shift, and modal
    /// keys work even when the editor does not have focus.
    pub fn subscription(&self) -> Subscription<Message> {
        event::listen_with(keyboard_events)
    }

    /// iced update entry point: applies one [`Message`] to the state and
    /// returns follow-up work (file reads, indexing, scrolling, focus).
    /// Most arms delegate to the methods below or in the `app` command handlers.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        let inspection = self.schedule_inspection();
        Task::batch([task, inspection])
    }

    /// Dispatches one message; [`App::update`] wraps it to schedule
    /// inspections after any change.
    fn handle(&mut self, message: Message) -> Task<Message> {
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
            Message::Indexed(Err(error)) => {
                self.indexing = Indexing::Idle;
                self.notice = Some(error);
                Task::none()
            }
            Message::Indexed(Ok(report)) => {
                self.indexing = Indexing::Done(report);
                // Edits made while the scan ran win over what it read from disk.
                for doc in self.docs.iter().filter(|d| d.dirty && d.is_php()) {
                    self.lang.update_file(&doc.path, &doc.text);
                }
                // Inspections depend on the whole index: rerun them now.
                self.inspect_pending = true;
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
                    self.inspect_pending = true;
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
            Message::Inspect(generation) => self.run_inspection(generation),
            Message::Inspected(path, version, diagnostics) => {
                self.store_diagnostics(&path, version, diagnostics);
                Task::none()
            }
            Message::IntentionMove(delta) => {
                if let Some(menu) = &mut self.intentions {
                    let n = menu.items.len() as i32;
                    menu.selected = (menu.selected as i32 + delta).rem_euclid(n.max(1)) as usize;
                }
                Task::none()
            }
            Message::IntentionAccept => self.apply_selected_intention(),
            Message::IntentionPick(i) => {
                if let Some(menu) = &mut self.intentions {
                    menu.selected = i;
                }
                self.apply_selected_intention()
            }
            Message::OpenProblem(i) => self.open_problem(i),
            Message::RenameInput(name) => {
                if let Some(dialog) = &mut self.rename {
                    dialog.name = name;
                    dialog.error = None;
                }
                Task::none()
            }
            Message::RenameSubmit => self.submit_rename(),
            Message::RenamePlanned(plan) => self.apply_rename(plan),
            Message::OpenHierarchy(i) => self.open_hierarchy_row(i),
        }
    }

    /// Handles [`Message::Keyboard`]. Detects double-Shift for Search
    /// Everywhere, lets open modals own arrows/Escape even while their input
    /// has focus, routes Shift+Enter in the find bar, and runs keymap
    /// commands for uncaptured keys. Escape with no popup open also hides the
    /// bottom panel.
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
                if self.search_dialog.is_some() || self.rename.is_some() {
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

    /// Switches to a new project root on [`Message::FolderPicked`]: resets
    /// tabs, history, and panels, reads the git branch, then starts project
    /// indexing and a blocking file listing in the background (answered by
    /// [`Message::Indexed`] and [`Message::FilesListed`]).
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
                    tokio::task::spawn_blocking(move || crate::project::list_files(&list_root))
                        .await
                        .unwrap_or_default()
                },
                Message::FilesListed,
            ),
        ])
    }

    /// Dismisses the transient popups (completion, hover, notice toast).
    /// Called on Escape, tab switches, jumps, and project changes; leaves the
    /// find bar, modals, and bottom panel alone.
    pub fn close_overlays(&mut self) {
        self.completion = None;
        self.hover = None;
        self.notice = None;
        self.intentions = None;
        self.signature = None;
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

    /// Activates tab `i`: closes popups if the tab changes, moves the file to
    /// the front of `recent`, re-runs find-bar matching for the new buffer,
    /// and restores that tab's scroll offset and editor focus. Out-of-range
    /// indices are ignored.
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
        if self.docs[i].diagnostics_version != self.docs[i].version() {
            self.inspect_pending = true;
        }
        let task = self.refresh_find(false);
        let y = self.docs[i].scroll_y;
        Task::batch([task, scroll_to(y), operation::focus(Id::new(EDITOR_ID))])
    }

    /// Closes tab `i` without a save prompt, then activates the neighbour
    /// that keeps the same visual position (or nothing if no tabs remain).
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

    /// Writes the active buffer to disk synchronously (`Cmd::Save`). On
    /// failure the error is shown as a notice and the tab stays dirty.
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

    /// Cursor location in the active tab, used to push navigation history.
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

    /// Back (`true`) or forward (`false`) through navigation history
    /// (`Cmd::Back`/`Cmd::Forward`). Pushes the current location onto the
    /// opposite stack before jumping, so the move is reversible.
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

    /// Applies a raw `text_editor` action ([`Message::Edit`]). After the
    /// change it reindexes and refreshes find matches, keeps the completion
    /// popup in sync with typing (or auto-opens it after `->`, `::`, `$`, or
    /// a new word), turns Cmd+click into go-to-declaration, and scrolls to
    /// keep the cursor visible for keyboard actions.
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
        if matches!(inserted, Some('(' | ',')) || (changed && self.signature.is_some()) {
            self.refresh_signature(false);
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
        if matches!(typed, Some('(' | ')')) || (changed && self.signature.is_some()) {
            self.refresh_signature(false);
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
        self.inspect_pending = true;
        self.intentions = None;
        let _ = self.refresh_find(false);
    }

    /// Whether typing `c` should open completion unprompted: after `->`,
    /// `::`, `$`, or the first character of a new identifier in PHP files.
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

    /// Recomputes completion candidates at the cursor and opens or updates
    /// the popup, keeping the current selection index where possible. The
    /// replaced prefix includes a leading `$`. `explicit` is set for
    /// `Cmd::Complete`, which shows a "No suggestions" notice when empty;
    /// implicit refreshes from typing just close the popup.
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

    /// Replaces the typed prefix with the selected candidate as one undo step.
    /// Avoids doubling `$` for members, and inserts the candidate's `use`
    /// import above the cursor when it has one, shifting the cursor down by
    /// the inserted lines.
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

    /// Scrolls the editor just enough to keep the cursor line visible, with
    /// a line of margin above and two below. Called after keyboard edits.
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

    /// Tracks the text cell under the mouse. When the cell changes it hides
    /// the hover popup and starts a [`HOVER_DELAY`] timer tagged with a new
    /// generation; only the latest [`Message::HoverFire`] shows a popup.
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

    /// Resolves the declaration of the PHP symbol under the mouse and fills
    /// [`Hover`] with its signature, location, and docs. Does nothing past
    /// the end of a line or when no declaration is found.
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
        let problems: Vec<_> = self
            .diagnostics_at(offset)
            .into_iter()
            .map(|d| (d.severity, d.message))
            .collect();
        let def = self.lang.definitions(&doc.text, offset).into_iter().next();
        if def.is_none() && problems.is_empty() {
            return;
        }
        let start = phpthunder::analysis::ident_start(&doc.text, (offset + 1).min(doc.text.len()));
        let col = doc.visual_column(doc.position_of(start));
        let (signature, location, doc_text) = match &def {
            Some(def) => {
                let s = &def.symbol;
                (
                    s.signature.clone(),
                    format!(
                        "{} · {}:{}",
                        qualified(s),
                        file_name(&def.path),
                        s.name_span.start.line + 1
                    ),
                    s.doc.clone(),
                )
            }
            None => (String::new(), String::new(), String::new()),
        };
        self.hover = Some(Hover {
            line,
            col,
            signature,
            location,
            doc: doc_text,
            problems,
        });
    }

    // ----- declarations and usages --------------------------------------------

    /// Jumps to the declaration of the symbol at the cursor
    /// (`Cmd::GotoDeclaration` or Cmd+click), recording history first.
    /// Shows a notice when nothing is found.
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

    /// Starts a background usages search for the symbol at the cursor
    /// (`Cmd::FindUsages`), passing unsaved buffers so edits count. Opens the
    /// usages panel in a "searching" state; results arrive as
    /// [`Message::UsagesFound`].
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

    /// Text of `path`, preferring an open tab's buffer over a blocking disk
    /// read. Used to build usage previews.
    pub fn file_text(&self, path: &Path) -> Option<String> {
        self.docs
            .iter()
            .find(|d| d.path == path)
            .map(|d| d.text.clone())
            .or_else(|| std::fs::read_to_string(path).ok())
    }

    /// Turns a usages search result into panel rows with source previews
    /// (each file read once) and a `N usages in M files` title. `None` means
    /// no symbol was under the cursor: the panel closes with a notice.
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

/// Task that scrolls the editor scrollable to absolute offset `y`. The
/// caller is responsible for storing the same value in `Document::scroll_y`
/// when it should persist across tab switches.
pub fn scroll_to(y: f32) -> Task<Message> {
    operation::scroll_to(
        Id::new(EDITOR_SCROLL_ID),
        AbsoluteOffset {
            x: None,
            y: Some(y),
        },
    )
}

/// Last path component as a display string (empty for roots); used in
/// titles, usage labels, and hover locations.
pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Display name of a symbol: `Class::member` for members (short class
/// name), the fully qualified name otherwise.
pub fn qualified(s: &Symbol) -> String {
    if s.kind.is_member() {
        let owner = s.fqn.rsplit('\\').next().unwrap_or(&s.fqn);
        format!("{owner}::{}", s.name)
    } else {
        s.fqn.clone()
    }
}
