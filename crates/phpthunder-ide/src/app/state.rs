//! The application state and its update loop: files, tabs, editing,
//! completion, hover, navigation history, declarations and usages.

use super::*;
use crate::editor::Document;
use crate::keymap::{self, Cmd};
use crate::language::Language;
use crate::palette::Palette;
use crate::project::{FileTree, git_branch};
use crate::search::{FindBar, SearchDialog, Toggle};
use crate::tr;
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
    /// Sidebar width and bottom panel height, resized by dragging.
    pub panes: Panes,
    /// Last known editor viewport height, for scroll math.
    pub viewport_height: f32,
    /// Width of the editor area, for soft wrap.
    pub editor_width: f32,
    /// Whether long lines wrap at the editor width.
    pub soft_wrap: bool,
    /// Open completion popup.
    pub completion: Option<Completion>,
    /// A PHP completion asked for since the last update (`Some(explicit)`).
    pub completion_request: Option<bool>,
    /// Number of the latest background completion request.
    pub completion_generation: u64,
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
    /// Tool panel shown below the editor, `None` when that area is hidden.
    pub bottom: Option<Bottom>,
    /// Tool panel shown right of the editor, `None` when that area is hidden.
    pub right: Option<Bottom>,
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
    /// Whether the window has focus; see [`App::notify_background`].
    pub focused: bool,
    /// Currently held keyboard modifiers.
    pub modifiers: Modifiers,
    /// When a bare Shift was last pressed, for double-Shift detection.
    last_shift: Option<Instant>,
    /// Settings of the last find-in-path, restored when the dialog reopens.
    pub last_search: Option<SearchDialog>,
    /// Whether "Replace all" in the Find panel waits for its confirming
    /// second press.
    pub confirm_replace_in_path: bool,
    /// A selection to apply once its file finishes loading.
    pub pending_select: Option<(PathBuf, usize, usize, usize)>,
    /// Open `⌥↵` menu.
    pub intentions: Option<Intentions>,
    /// Open parameter info popup.
    pub signature: Option<SignatureView>,
    /// Parameter info asked for since the last update (`Some(explicit)`).
    pub signature_request: Option<bool>,
    /// Number of the latest background parameter-info request.
    pub signature_generation: u64,
    /// Open rename dialog.
    pub rename: Option<RenameDialog>,
    /// Open About dialog.
    pub about: Option<About>,
    /// The remembered session (last project, tabs per project).
    pub session: SavedSession,
    /// Number of the latest session-save timer.
    pub session_generation: u64,
    /// Whether saved tabs are being read back (the session is not saved
    /// meanwhile, so the half-restored state never overwrites it).
    pub restoring: bool,
    /// Input prompt of a refactoring, while open.
    pub refactor_prompt: Option<RefactorPrompt>,
    /// Name prompt for a tree file operation, while open.
    pub name_prompt: Option<NamePrompt>,
    /// Content of the type hierarchy panel.
    pub hierarchy: Option<Hierarchy>,
    /// Set by edits and tab switches; [`App::update`] turns it into a
    /// debounced inspection run.
    pub inspect_pending: bool,
    /// Generation of the latest inspection timer; older timers do nothing.
    pub inspect_generation: u64,
    /// Size facts of open image and binary tabs (see [`crate::ui::Preview`]).
    pub previews: std::collections::HashMap<PathBuf, crate::ui::Preview>,
    /// The integrated terminal's shell, once started.
    pub terminal: Option<crate::terminal::Session>,
    /// The Claude CLI session of the Claude panel, once started.
    pub claude: Option<crate::terminal::Session>,
    /// Whether keys go to the terminal (set by clicking it or `⌥F12`).
    pub terminal_focused: bool,
    /// Console panel keys and pastes go to: [`Bottom::Terminal`] or
    /// [`Bottom::Claude`].
    pub console: Bottom,
    /// Last measured terminal grid as (rows, columns).
    pub terminal_grid: Option<(u16, u16)>,
    /// Last measured Claude panel grid as (rows, columns).
    pub claude_grid: Option<(u16, u16)>,
    /// Content of the Tests panel.
    pub tests: Option<Tests>,
    /// Line coverage of the last run with coverage, per project file.
    pub coverage: std::collections::HashMap<PathBuf, crate::testing::FileCoverage>,
    /// Scope of the last test run, for "run again".
    pub last_test_scope: Option<crate::testing::Scope>,
    /// Whether PHPStan is analysing a saved file.
    /// Quality tools still analysing the last saved file.
    pub quality_running: usize,
    /// Label of the project's PHP interpreter when it is not the local
    /// `php` (shown in the status bar); refreshed when the project opens
    /// and when its settings file changes.
    pub interpreter_label: Option<String>,
    /// Route names, views and config keys of a Laravel project.
    pub laravel: LaravelScan,
    /// Bookmarked lines (0-based) by file (`F3`).
    pub bookmarks: std::collections::BTreeMap<PathBuf, std::collections::BTreeSet<usize>>,
    /// Latest edit locations as (file, line), oldest first.
    pub edit_locations: Vec<(PathBuf, usize)>,
    /// How far back the last `⇧⌘⌫` went; reset by the next edit.
    pub edit_back: usize,
    /// Copied and cut texts, newest first (`⇧⌘V`).
    pub clipboard_history: Vec<String>,
    /// What the Compare panel shows, once something was compared.
    pub compare: Option<Compare>,
    /// Git Annotate lines of files where it is turned on.
    pub blame: std::collections::HashMap<PathBuf, Vec<crate::git::LineBlame>>,
    /// While distraction-free mode is on: the sidebar and panels to bring
    /// back when it ends.
    pub zen: Option<(bool, Option<Bottom>, Option<Bottom>)>,
    /// Interface scale (`⌘=` / `⌘−` / `⌘0`), 1.0 by default.
    pub zoom: f32,
    /// Whether spaces and tabs are drawn as dots and arrows.
    pub show_whitespace: bool,
    /// Claude plan limits last reported by the Claude panel's CLI.
    pub claude_usage: crate::claude_usage::Usage,
    /// Installed Composer and npm packages, from the project's lock files;
    /// shown beside dependencies in `composer.json` and `package.json`.
    pub packages: crate::packages::Installs,
    /// Git status of the project; `None` outside a repository.
    pub git: Option<crate::git::RepoStatus>,
    /// `HEAD` version of open files (`None`: not in `HEAD`), for the gutter.
    pub git_base: std::collections::HashMap<PathBuf, Option<String>>,
    /// Gutter change marks of open files.
    pub git_marks: std::collections::HashMap<PathBuf, Vec<crate::git::LineChange>>,
    /// Commit panel state.
    pub commit: CommitPanel,
    /// What the branch picker does with the chosen branch.
    pub branch_mode: BranchMode,
    /// Line breakpoints (0-based lines) by file.
    pub breakpoints: std::collections::HashMap<PathBuf, std::collections::BTreeSet<usize>>,
    /// The debugger panel and its session, once one was started.
    pub debug: Option<DebugView>,
    /// Debug sessions stop at the first line of the script.
    pub debug_break_first: bool,
    /// Last mouse y over the line-number gutter, in content pixels.
    pub gutter_y: f32,
    /// How often each completion proposal was picked, for ranking.
    pub completion_stats: std::collections::HashMap<String, u32>,
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
            Key::Named(Named::Enter) => return custom(Message::CompletionAccept),
            Key::Named(Named::Tab) => return custom(Message::CompletionReplace),
            Key::Named(Named::Escape) => return custom(Message::CloseOverlays),
            _ => {}
        }
    }
    if let Some(cmd) = keymap::command(&key.key, m) {
        return custom(Message::Command(cmd));
    }
    // Copy and cut also feed the clipboard history (`⇧⌘V`).
    if m.command()
        && !m.shift()
        && !m.alt()
        && let Key::Character(c) = key.key.as_ref()
    {
        let action = match c {
            "c" => Some(Binding::Copy),
            "x" => Some(Binding::Cut),
            _ => None,
        };
        if let Some(action) = action {
            return Some(Binding::Sequence(vec![
                Binding::Custom(Message::RememberSelection),
                action,
            ]));
        }
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
/// captured them, and window focus changes as [`Message::WindowFocus`];
/// drops everything else.
fn keyboard_events(
    event: Event,
    status: event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        Event::Keyboard(e) => Some(Message::Keyboard(e, status == event::Status::Captured)),
        Event::Window(iced::window::Event::Focused) => Some(Message::WindowFocus(true)),
        Event::Window(iced::window::Event::Unfocused) => Some(Message::WindowFocus(false)),
        _ => None,
    }
}

/// Maps mouse events to pane resizing while a splitter is dragged
/// (subscribed only then, see [`App::subscription`]).
fn resize_events(
    event: Event,
    _status: event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
            Some(Message::ResizeMove(position))
        }
        Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left))
        | Event::Mouse(iced::mouse::Event::CursorLeft) => Some(Message::ResizeEnd),
        _ => None,
    }
}

impl App {
    /// Builds the initial empty state; iced calls it once at startup. If the
    /// first CLI argument is a directory, returns a task that opens it as the
    /// project via [`Message::FolderPicked`].
    pub fn boot() -> (Self, Task<Message>) {
        let mut app = Self {
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
            panes: Panes::default(),
            viewport_height: 700.0,
            editor_width: 900.0,
            soft_wrap: crate::settings::settings().soft_wrap,
            completion: None,
            completion_request: None,
            completion_generation: 0,
            hover: None,
            hover_generation: 0,
            hover_at: None,
            usages: None,
            search: None,
            bottom: None,
            right: None,
            palette: None,
            find: None,
            search_dialog: None,
            nav_back: Vec::new(),
            nav_forward: Vec::new(),
            notice: None,
            focused: true,
            modifiers: Modifiers::default(),
            last_shift: None,
            last_search: None,
            confirm_replace_in_path: false,
            pending_select: None,
            intentions: None,
            signature: None,
            signature_request: None,
            signature_generation: 0,
            rename: None,
            about: None,
            name_prompt: None,
            refactor_prompt: None,
            session: session::load_session(),
            session_generation: 0,
            restoring: false,
            hierarchy: None,
            inspect_pending: false,
            inspect_generation: 0,
            previews: std::collections::HashMap::new(),
            terminal: None,
            claude: None,
            terminal_focused: false,
            console: Bottom::Terminal,
            terminal_grid: None,
            claude_grid: None,
            tests: None,
            coverage: std::collections::HashMap::new(),
            last_test_scope: None,
            quality_running: 0,
            interpreter_label: None,
            laravel: None,
            packages: crate::packages::Installs::default(),
            claude_usage: crate::claude_usage::Usage::default(),
            show_whitespace: false,
            zoom: 1.0,
            zen: None,
            compare: None,
            blame: std::collections::HashMap::new(),
            bookmarks: std::collections::BTreeMap::new(),
            edit_locations: Vec::new(),
            edit_back: 0,
            clipboard_history: Vec::new(),
            git: None,
            git_base: Default::default(),
            git_marks: Default::default(),
            commit: CommitPanel::default(),
            branch_mode: BranchMode::Checkout,
            breakpoints: Default::default(),
            debug: None,
            debug_break_first: false,
            gutter_y: 0.0,
            completion_stats: completion::load_completion_stats(),
        };
        // Sizes live in the settings file; older versions kept them in the
        // session, still read until the first resize writes the settings.
        if let Some(layout) = SavedLayout::from_settings().or(app.session.layout) {
            app.panes = Panes::new(layout.into());
        }
        // The folder given on the command line, else the last project
        // (unless the user turned `restore_session` off).
        let folder = std::env::args()
            .nth(1)
            .map(PathBuf::from)
            .or_else(|| {
                crate::settings::settings()
                    .restore_session
                    .then(|| app.session.last_project.clone())
                    .flatten()
            })
            .filter(|p| p.is_dir());
        let snapshot = App::snapshot_tasks(folder.as_deref());
        let task = match folder {
            Some(dir) => Task::done(Message::FolderPicked(Some(dir))),
            None => Task::none(),
        };
        (
            app,
            Task::batch([task, snapshot, crate::native_menu::install()]),
        )
    }

    /// Window title: `file — project — PhpThunder`, dropping parts that do
    /// not exist yet. Polled by iced after every update.
    pub fn title(&self) -> String {
        match (&self.tree.root, self.doc()) {
            (Some(root), Some(doc)) => {
                format!(
                    "{} — {} — {}",
                    doc.title(),
                    file_name(root),
                    crate::APP_NAME
                )
            }
            (Some(root), None) => format!("{} — {}", file_name(root), crate::APP_NAME),
            _ => crate::APP_NAME.into(),
        }
    }

    /// Shows `message` as the notice and writes it to the error log with
    /// the caller's location. Used where an operation fails (save, git,
    /// file ops, terminal, tools), not for informational notices.
    #[track_caller]
    pub(crate) fn fail(&mut self, message: String) {
        crate::log::error(&message);
        self.notify_background(crate::tr!("notify-error"), message.clone());
        self.notice = Some(message);
    }

    /// Changes the project tree filter with `change`, rereads the tree and
    /// saves the filters to the settings file. Used by the filter buttons
    /// above the tree.
    fn toggle_tree_filter(
        &mut self,
        change: impl FnOnce(&mut crate::project::TreeFilter),
    ) -> Task<Message> {
        let mut filter = self.tree.filter;
        change(&mut filter);
        self.tree.set_filter(filter);
        crate::settings::save_tree_filters(filter.hide_dot_folders, filter.hide_dependencies);
        Task::none()
    }

    /// Sends a desktop notification, but only while the window is in the
    /// background: in front, the in-app notice already shows the same.
    pub(crate) fn notify_background(&self, summary: String, body: String) {
        if !self.focused {
            crate::notify::send(summary, body);
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
        let mut subscriptions = vec![
            event::listen_with(keyboard_events),
            crate::native_menu::about_requests(),
        ];
        for session in [&self.terminal, &self.claude].into_iter().flatten() {
            subscriptions.push(
                Subscription::run_with(session.id, crate::terminal::output_events)
                    .map(Message::TerminalOutput),
            );
        }
        // The mouse is followed window-wide only while a pane is resized,
        // so the drag keeps going when the cursor outruns the thin handle.
        if self.panes.dragging().is_some() {
            subscriptions.push(event::listen_with(resize_events));
        }
        // Indexing reports through atomics; a steady tick redraws the bar.
        if matches!(self.indexing, Indexing::Running(_)) {
            subscriptions.push(
                iced::time::every(std::time::Duration::from_millis(120))
                    .map(|_| Message::IndexTick),
            );
        }
        // The Claude CLI rewrites its status snapshot as it answers; a slow
        // poll is enough for limits that move by the minute.
        if self.claude.as_ref().is_some_and(|s| !s.has_exited()) {
            subscriptions.push(
                iced::time::every(std::time::Duration::from_secs(5))
                    .map(|_| Message::ClaudeUsageTick),
            );
        }
        // The About spider walks only while the dialog is open.
        if self.about.is_some() {
            subscriptions.push(iced::window::frames().map(Message::AboutTick));
        }
        if let Some(root) = &self.tree.root {
            subscriptions.push(
                Subscription::run_with(root.clone(), crate::watch::file_changes)
                    .map(Message::FilesChanged),
            );
        }
        Subscription::batch(subscriptions)
    }

    /// iced update entry point: applies one [`Message`] to the state and
    /// returns follow-up work (file reads, indexing, scrolling, focus).
    /// Most arms delegate to the methods below or in the `app` command handlers.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        let session = self.save_session_if_changed();
        let inspection = self.schedule_inspection();
        let completion = self.schedule_completion();
        let signature = self.schedule_signature();
        Task::batch([task, inspection, completion, signature, session])
    }

    /// Dispatches one message; [`App::update`] wraps it to schedule
    /// inspections after any change.
    fn handle(&mut self, message: Message) -> Task<Message> {
        if let Some(task) = self.multi_caret_message(&message) {
            return task;
        }
        if let Message::Edit(action) = &message
            && self.alt_click_caret(action)
        {
            return Task::none();
        }
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
            Message::FilesChanged(paths) => self.files_changed(paths),
            Message::ShowFileActions(path) => self.show_file_actions(path),
            Message::SessionRestored(tabs, active) => self.session_restored(tabs, active),
            Message::PromptInput(value) => {
                if let Some(p) = &mut self.name_prompt {
                    p.value = value;
                    p.error = None;
                }
                Task::none()
            }
            Message::PromptSubmit => self.submit_name_prompt(),
            Message::RefactorInput(value) => {
                if let Some(p) = &mut self.refactor_prompt {
                    p.value = value;
                    p.error = None;
                }
                Task::none()
            }
            Message::RefactorSubmit => self.submit_refactor(),
            Message::RefactorPlanned(plan) => self.apply_refactor_plan(plan),
            Message::FilesListed(files) => {
                let (vendor, project): (Vec<PathBuf>, Vec<PathBuf>) = files
                    .into_iter()
                    .partition(|p| p.components().any(|c| c.as_os_str() == "vendor"));
                self.project_files = project;
                self.vendor_files = vendor;
                Task::none()
            }
            Message::GitStatus(result) => self.git_status_arrived(result),
            Message::GitBase(path, content) => {
                self.git_base_arrived(path, content);
                Task::none()
            }
            Message::GitRefresh => self.refresh_git(),
            Message::GitToggleStage(path) => self.git_toggle_stage(path),
            Message::GitStageAll(stage) => self.git_stage_all(stage),
            Message::GitSelect(path) => self.git_select(path),
            Message::GitDiff(path, diff) => {
                self.git_diff_arrived(path, diff);
                Task::none()
            }
            Message::GitOpenLine(line) => self.git_open_line(line),
            Message::GitCommitText(message) => {
                self.commit.message = message;
                Task::none()
            }
            Message::GitCommit(and_push) => self.git_commit(and_push),
            Message::GitDiscard(path) => self.git_discard(path),
            Message::GitPull => self.git_pull(),
            Message::GitPush => self.git_push(),
            Message::GitShowBranches => self.git_pick_branch(BranchMode::Checkout),
            Message::GitStashes(result) => self.git_stashes_arrived(result),
            Message::GitRemote(page, result) => self.git_remote_arrived(page, result),
            Message::GitBranches(result) => self.git_branches_arrived(result),
            Message::GitDone(op, result) => self.git_done(op, result),
            Message::DebugTest => self.debug_test(),
            Message::Debug(event) => self.debug_event(event),
            Message::DebugSend(command) => self.debug_send(command),
            Message::DebugFrame(level) => self.debug_select_frame(level),
            Message::DebugToggleVar(fullname) => self.debug_toggle_variable(fullname),
            Message::DebugEvalText(text) => {
                if let Some(d) = &mut self.debug {
                    d.eval = text;
                }
                Task::none()
            }
            Message::DebugEvalSubmit => self.debug_evaluate(),
            Message::GutterMoved(y) => {
                self.gutter_y = y;
                Task::none()
            }
            Message::GutterPressed => self.toggle_breakpoint_at_gutter(),
            Message::TakeSnapshot => self.take_snapshot(),
            Message::SnapshotType(text) => self.snapshot_type(&text),
            Message::Snapshot(shot) => self.save_snapshot(shot),
            Message::LaravelScanned(project) => {
                self.laravel = project;
                Task::none()
            }
            Message::Indexed(Err(error)) => {
                self.indexing = Indexing::Idle;
                self.fail(error);
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
                    let mut base = Task::none();
                    if !self.docs.iter().any(|d| d.path == path) {
                        let mut doc = Document::new(path.clone(), text);
                        doc.set_editorconfig(self.editorconfig_for(&path));
                        self.docs.push(doc);
                        base = self.load_git_base(path.clone());
                    }
                    let open = self.open(path, target);
                    self.inspect_pending = true;
                    Task::batch([open, self.apply_pending_select(), base])
                }
                Err(e) => {
                    self.fail(tr!(
                        "notice-open-failed",
                        path = path.display().to_string(),
                        error = e
                    ));
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
            Message::Indent(false) if self.expand_template() => self.reveal_cursor(),
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
                self.accept_completion(false)
            }
            Message::CompletionAccept => self.accept_completion(false),
            Message::CompletionReplace => self.accept_completion(true),
            Message::CloseOverlays => {
                self.close_overlays();
                if let Some(doc) = self.doc_mut() {
                    doc.clear_extra_carets();
                }
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
                self.show_panel(b);
                if b == Bottom::Claude {
                    if self
                        .claude
                        .as_ref()
                        .is_none_or(crate::terminal::Session::has_exited)
                    {
                        self.start_claude();
                    }
                    return self.focus_terminal();
                }
                if b == Bottom::Terminal {
                    if self
                        .terminal
                        .as_ref()
                        .is_none_or(crate::terminal::Session::has_exited)
                    {
                        self.start_terminal();
                    }
                    return self.focus_terminal();
                }
                self.terminal_focused = false;
                Task::none()
            }
            Message::ClosePanels(dock) => {
                self.close_panels(dock);
                Task::none()
            }
            Message::MovePanel(panel) => self.move_panel(panel),
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
            Message::SearchReplaceText(text) => {
                if let Some(s) = &mut self.last_search {
                    s.replacement = text;
                }
                self.confirm_replace_in_path = false;
                Task::none()
            }
            Message::ReplaceInPath => self.replace_in_path(),
            Message::EditorResized(size) => {
                self.editor_width = size.width;
                self.viewport_height = size.height;
                Task::none()
            }
            Message::SearchDone(result) => {
                match result {
                    Ok(results) => self.search = Some(Search::Done(results, None)),
                    Err(e) => {
                        self.search = None;
                        self.notice = Some(tr!("notice-invalid-pattern", error = e));
                    }
                }
                Task::none()
            }
            Message::OpenSearchHit(f, h) => self.open_search_hit(f, h),
            Message::CloseModal => {
                self.palette = None;
                self.about = None;
                self.search_dialog = None;
                self.name_prompt = None;
                self.refactor_prompt = None;
                operation::focus(Id::new(EDITOR_ID))
            }
            Message::ToggleSidebar => {
                self.sidebar = !self.sidebar;
                Task::none()
            }
            Message::OpenAt(path, pos) => {
                self.record_nav();
                self.open(path, Some(pos))
            }
            Message::ImplementationsAt(offset) => {
                if let Some(doc) = self.doc_mut() {
                    let pos = doc.position_of(offset);
                    doc.move_to(pos);
                }
                self.goto_implementation()
            }
            Message::ClipboardForCompare(text) => self.clipboard_arrived(text),
            Message::BlameArrived(path, result) => {
                self.blame_arrived(path, result);
                Task::none()
            }
            Message::RestoreCompared => self.restore_compared(),
            Message::RememberSelection => {
                self.remember_selection();
                Task::none()
            }
            Message::JumpToLine(line) => {
                let Some(doc) = self.doc() else {
                    return Task::none();
                };
                let column = doc.line_text(line).len() - doc.line_text(line).trim_start().len();
                self.place_cursor(iced::widget::text_editor::Position { line, column })
            }
            Message::AboutTick(now) => {
                if let Some(about) = &mut self.about {
                    about.now = now;
                }
                Task::none()
            }
            Message::OpenRepository => {
                crate::packages::open_in_browser(env!("CARGO_PKG_REPOSITORY"));
                Task::none()
            }
            Message::ClaudeUsageTick => {
                self.claude_usage = crate::claude_usage::read(&crate::claude_usage::status_file());
                Task::none()
            }
            Message::MarkdownView(view) => {
                if let Some(doc) = self.doc_mut() {
                    doc.markdown_view = view;
                }
                Task::none()
            }
            Message::MarkdownLink(uri) => self.open_markdown_link(&uri),
            Message::ResizeStart(splitter) => {
                let room = match splitter {
                    Splitter::Sidebar | Splitter::Right => self.editor_width,
                    Splitter::Bottom => self.viewport_height,
                };
                self.panes.begin(splitter, room);
                Task::none()
            }
            Message::ResizeMove(cursor) => {
                self.panes.moved(cursor);
                Task::none()
            }
            Message::ResizeEnd => {
                if self.panes.end() {
                    SavedLayout::from(self.panes.sizes).save();
                }
                Task::none()
            }
            Message::ResizeReset(splitter) => {
                self.panes.reset(splitter);
                SavedLayout::from(self.panes.sizes).save();
                Task::none()
            }
            Message::IndexTick => Task::none(),
            Message::ToggleDotFolders => self.toggle_tree_filter(|f| {
                f.hide_dot_folders = !f.hide_dot_folders;
            }),
            Message::ToggleDependencies => self.toggle_tree_filter(|f| {
                f.hide_dependencies = !f.hide_dependencies;
            }),
            Message::WindowFocus(focused) => {
                self.focused = focused;
                Task::none()
            }
            Message::SplitterHover(splitter) => {
                self.panes.hovered = splitter;
                Task::none()
            }
            Message::DragWindow => iced::window::latest().and_then(iced::window::drag),
            Message::ToggleMaximize => {
                iced::window::latest().and_then(iced::window::toggle_maximize)
            }
            Message::Keyboard(event, captured) => self.keyboard(event, captured),
            Message::Inspect(generation) => self.run_inspection(generation),
            Message::Gutter(action) => self.gutter_action(action),
            Message::Inspected(path, version, diagnostics, markers) => {
                self.store_diagnostics(&path, version, diagnostics, markers);
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
            Message::TerminalOutput(id) => {
                // Output of a replaced session is stale: nothing to redraw.
                for session in [&self.terminal, &self.claude].into_iter().flatten() {
                    if session.id == id {
                        session.acknowledge_output();
                    }
                }
                Task::none()
            }
            Message::TerminalResized(panel, size) => {
                self.terminal_resized(panel, size);
                Task::none()
            }
            Message::TerminalFocus(panel) => {
                self.console = panel;
                self.focus_terminal()
            }
            Message::TerminalPaste(text) => {
                self.terminal_paste(text);
                Task::none()
            }
            Message::TerminalRestart(panel) => {
                self.console = panel;
                if panel == Bottom::Claude {
                    self.start_claude();
                } else {
                    self.start_terminal();
                }
                self.focus_terminal()
            }
            Message::RunTests(target) => self.run_tests(target),
            Message::TestsDone(result) => {
                self.tests_done(result);
                Task::none()
            }
            Message::OpenTest(i) => self.open_test(i),
            Message::DebuggerValidated(result) => {
                match result {
                    Ok(report) => self.notice = Some(report),
                    Err(error) => self.fail(error),
                }
                Task::none()
            }
            Message::Profiled(result) => match result {
                Ok(path) => self.open(path, None),
                Err(error) => {
                    self.fail(error);
                    Task::none()
                }
            },
            Message::CompletionReady {
                generation,
                version,
                offset,
                explicit,
                start,
                items,
            } => {
                self.completion_ready(generation, version, offset, explicit, start, items);
                Task::none()
            }
            Message::SignatureReady {
                generation,
                version,
                offset,
                explicit,
                help,
            } => {
                self.signature_ready(generation, version, offset, explicit, help);
                Task::none()
            }
            Message::SaveSession(generation) => self.save_session_now(generation),
            Message::SessionSaved => Task::none(),
            Message::ClearCoverage => {
                self.coverage.clear();
                Task::none()
            }
            Message::Formatted(path, version, result) => {
                self.formatted(&path, version, result);
                Task::none()
            }
            Message::QualityDone(tool, path, result) => {
                self.quality_done(tool, &path, result);
                Task::none()
            }
            Message::TerminalScroll(panel, lines) => {
                if let Some(session) = self.session_of_mut(panel) {
                    session.scroll(lines);
                }
                Task::none()
            }
        }
    }

    /// Handles [`Message::Keyboard`]. Detects double-Shift for Search
    /// Everywhere, lets open modals own arrows/Escape even while their input
    /// has focus, routes Shift+Enter in the find bar, and runs keymap
    /// commands for uncaptured keys. Escape with no popup open also hides the
    /// bottom panel.
    fn keyboard(&mut self, event: keyboard::Event, captured: bool) -> Task<Message> {
        // The palette and dialogs still own their keys; otherwise a focused
        // terminal gets everything but IDE shortcuts.
        if self.palette.is_none()
            && self.search_dialog.is_none()
            && self.rename.is_none()
            && self.about.is_none()
            && let Some(task) = self.terminal_keyboard(&event)
        {
            self.last_shift = None;
            return task;
        }
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
                if self.search_dialog.is_some() || self.rename.is_some() || self.about.is_some() {
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
        self.refresh_interpreter();
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
        self.right = None;
        let progress = std::sync::Arc::new(crate::language::IndexProgress::default());
        self.indexing = Indexing::Running(progress.clone());
        self.laravel = None;
        self.packages = crate::packages::Installs::load(&root);
        self.git = None;
        self.git_base.clear();
        self.git_marks.clear();
        self.commit = CommitPanel::default();
        let list_root = root.clone();
        let restore = self.restore_tabs(&root);
        Task::batch([
            restore,
            self.scan_laravel(),
            self.refresh_git(),
            Task::perform(
                self.lang.clone().index_project(root, progress),
                Message::Indexed,
            ),
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
            // Images and binaries open as preview tabs: nothing to read as text.
            let file_type = crate::filetype::FileType::detect(&path, None);
            if matches!(
                file_type,
                crate::filetype::FileType::Image | crate::filetype::FileType::Binary
            ) {
                self.previews
                    .insert(path.clone(), crate::ui::Preview::read(&path, file_type));
                return Task::done(Message::FileLoaded(path, Ok(String::new()), None));
            }
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
        let Some(layout) = self.doc().map(|d| self.wrap_layout(d)) else {
            return Task::none();
        };
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        doc.move_to(position);
        let col = doc.visual_column(position);
        let y = (crate::app::row_top(&layout, position.line, col) - view / 3.0).max(0.0);
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
        self.refresh_composer_problems();
        let task = self.refresh_find(false);
        let y = self.docs[i].scroll_y;
        Task::batch([task, scroll_to(y), operation::focus(Id::new(EDITOR_ID))])
    }

    /// Closes tab `i` without a save prompt, then activates the neighbour
    /// that keeps the same visual position (or nothing if no tabs remain).
    pub(crate) fn close_tab(&mut self, i: usize) -> Task<Message> {
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

    /// Whether the active tab shows an image or binary file instead of
    /// text; such tabs are read-only and never saved.
    pub fn preview_active(&self) -> bool {
        self.doc().is_some_and(|d| {
            matches!(
                d.file_type,
                crate::filetype::FileType::Image | crate::filetype::FileType::Binary
            )
        })
    }

    /// The EditorConfig style for `path`, reading `.editorconfig` files up
    /// to the project root (or the file's folder outside a project).
    pub(crate) fn editorconfig_for(&self, path: &Path) -> crate::formatter::editorconfig::Style {
        let root = self
            .tree
            .root
            .as_deref()
            .filter(|r| path.starts_with(r))
            .or_else(|| path.parent());
        root.map(|r| crate::formatter::editorconfig::style_for(path, r))
            .unwrap_or_default()
    }

    /// Writes the active buffer to disk synchronously (`Cmd::Save`), first
    /// applying the file's EditorConfig save rules (trailing whitespace,
    /// final newline, line endings). On
    /// failure the error is shown as a notice and the tab stays dirty.
    pub fn save(&mut self) -> Task<Message> {
        // A preview tab holds no text: writing it would empty the file.
        if self.preview_active() {
            return Task::none();
        }
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        let (text, disk) = doc.text_for_save();
        if text != doc.text {
            let cursor = doc.cursor();
            doc.checkpoint();
            doc.set_text(text, cursor);
        }
        if let Err(e) = std::fs::write(&doc.path, &disk) {
            let msg = tr!(
                "notice-save-failed",
                file = doc.title(),
                error = e.to_string()
            );
            self.fail(msg);
            return Task::none();
        }
        doc.mark_saved();
        let path = doc.path.clone();
        self.record_local_history();
        // Saved lines changed: annotations would point at the wrong lines.
        self.blame.remove(&path);
        let rescan = self.rescan_laravel_after_save(&path);
        Task::batch([rescan, self.refresh_git(), self.quality_after_save(path)])
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
        if self.preview_active() {
            return Task::none();
        }
        let inserted = match &action {
            Action::Edit(Edit::Insert(c)) => Some(*c),
            _ => None,
        };
        let deleted = matches!(action, Action::Edit(Edit::Backspace | Edit::Delete));
        let clicked = matches!(action, Action::Click(_));
        let dragged = matches!(action, Action::Drag(_));
        let goto = clicked && self.modifiers.command();
        if clicked {
            self.terminal_focused = false;
        }
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        let changed = doc.perform(action);
        self.after_change(changed);

        if self.completion.is_some() {
            match (inserted, deleted) {
                (Some(c), _)
                    if is_ident(c)
                        || c == '$'
                        || self.in_string_of_laravel_project()
                        || self.should_auto_complete(c) =>
                {
                    self.refresh_completion(false)
                }
                (None, true) => self.refresh_completion(false),
                _ => self.completion = None,
            }
        } else if let Some(c) = inserted
            && (self.should_auto_complete(c) || self.in_string_of_laravel_project())
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
        if self.preview_active() {
            return Task::none();
        }
        let Some(doc) = self.doc_mut() else {
            return Task::none();
        };
        let before = doc.highlight_version;
        op(doc);
        let changed = doc.highlight_version != before;
        self.after_change(changed);
        match typed {
            Some('\'' | '"') if self.in_string_of_laravel_project() => {
                self.refresh_completion(false)
            }
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
            self.lang.update_file_later(&doc.path, &doc.text);
        }
        if let Some(path) = self.doc().map(|d| d.path.clone()) {
            self.refresh_git_marks(&path);
        }
        self.inspect_pending = true;
        self.intentions = None;
        self.record_edit_location();
        let _ = self.refresh_find(false);
    }

    /// Scrolls the editor just enough to keep the cursor line visible, with
    /// a line of margin above and two below. Called after keyboard edits.
    pub fn reveal_cursor(&self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let cursor = doc.cursor();
        let top = crate::app::row_top(
            &self.wrap_layout(doc),
            cursor.line,
            doc.visual_column(cursor),
        );
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
        let row = ((p.y - EDITOR_PAD_TOP) / CODE_LINE).floor();
        let col = ((p.x - EDITOR_PAD_LEFT) / CHAR_WIDTH).floor();
        let at = match self.doc() {
            Some(doc) if row >= 0.0 && col >= 0.0 => {
                // A wrapped row continues its line further right.
                let (line, base) = self.wrap_layout(doc).line_at_row(row as usize);
                Some((line, base + col as usize))
            }
            _ => None,
        };
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

    /// `F1` / `⌃J`: shows the hover popup (signature, docs, problems) for
    /// the symbol at the caret instead of the mouse.
    pub(crate) fn quick_doc(&mut self) -> Task<Message> {
        let Some(doc) = self.doc() else {
            return Task::none();
        };
        let pos = doc.cursor();
        let col = doc.visual_column(pos);
        self.hover_at = Some((pos.line, col));
        self.show_hover();
        if self.hover.is_none() {
            self.notice = Some(tr!("notice-no-documentation"));
        }
        Task::none()
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
            .map(|d| (d.severity, crate::i18n::diagnostic_message(&d)))
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
        if let Some(url) = package_page_at(doc) {
            crate::packages::open_in_browser(&url);
            return Task::none();
        }
        if let Some((path, pos)) = self.laravel_definition(&doc.text, offset) {
            self.close_overlays();
            self.record_nav();
            return self.open(path, Some(pos));
        }
        match self.lang.definitions(&doc.text, offset).into_iter().next() {
            Some(def) => {
                self.close_overlays();
                self.record_nav();
                self.open(def.path, Some(def.symbol.name_span.start))
            }
            None => {
                self.notice = Some(tr!("notice-no-declaration"));
                Task::none()
            }
        }
    }

    /// Follows a link clicked in the Markdown preview: `http(s)`/`mailto`
    /// links open in the browser; anything else is a path relative to the
    /// document (its `#fragment` dropped) and opens in a tab when it exists.
    fn open_markdown_link(&mut self, uri: &str) -> Task<Message> {
        if ["http://", "https://", "mailto:"]
            .iter()
            .any(|scheme| uri.starts_with(scheme))
        {
            crate::packages::open_in_browser(uri);
            return Task::none();
        }
        let relative = uri.split('#').next().unwrap_or_default();
        let target = self
            .doc()
            .and_then(|d| d.path.parent())
            .map(|dir| dir.join(relative));
        match target {
            Some(path) if !relative.is_empty() && path.is_file() => self.open(path, None),
            _ => Task::none(),
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
        self.show_panel(Bottom::Usages);
        self.close_overlays();
        Task::perform(self.lang.clone().usages(text, offset, open), |found| {
            Message::UsagesFound(found.map(Box::new))
        })
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
    fn show_usages(&mut self, found: Option<Box<(Definition, Vec<Usage>)>>) {
        let Some((def, usages)) = found.map(|b| *b) else {
            self.usages = None;
            self.hide_panel(Bottom::Usages);
            self.notice = Some(tr!("notice-nothing-to-search"));
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
        let title = tr!(
            "usages-title",
            name = name,
            count = count,
            files = files.len()
        );
        self.usages = Some(Usages::Found {
            title,
            rows,
            selected: None,
        });
    }

    /// Class › member (or function) at the cursor, for the status bar;
    /// empty outside declarations.
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
            out.push(
                class
                    .fqn
                    .rsplit('\\')
                    .next()
                    .unwrap_or(&class.fqn)
                    .to_string(),
            );
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

/// Registry page of the dependency declared on the caret's line when `doc`
/// is a `composer.json` or `package.json`; `None` elsewhere. Lets
/// [`App::goto_definition`] (Cmd/Ctrl+click, `F12`) open a package's page.
fn package_page_at(doc: &Document) -> Option<String> {
    let manager = crate::packages::Manager::of_manifest(&doc.path)?;
    let line = doc.cursor().line;
    crate::packages::dependencies(&doc.text, manager)
        .into_iter()
        .find(|d| d.line == line)
        .and_then(|d| manager.page_url(&d.name))
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
