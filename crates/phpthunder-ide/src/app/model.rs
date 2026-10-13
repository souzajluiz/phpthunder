//! Application data: messages, panel and popup state, navigation locations.

use crate::keymap::Cmd;
use crate::language::IndexReport;
use crate::search::{SearchResults, Toggle};
use crate::tr;
use iced::Point;
use iced::keyboard::{self};
use iced::widget::scrollable::Viewport;
use iced::widget::text_editor::Action;
use phpthunder::analysis::{Definition, Usage};
use phpthunder::inspection::{Diagnostic, Fix};
use phpthunder::refactor::{RenamePlan, SignatureHelp};
use phpthunder::syntax::Pos;
use std::path::PathBuf;

/// Result of the background Laravel key scan, shared with the UI state:
/// `None` when the project is not a Laravel application.
pub type LaravelScan = Option<std::sync::Arc<phpthunder::laravel::LaravelProject>>;

/// Every event the UI, async tasks, and subscriptions feed into [`App::update`].
#[derive(Debug, Clone)]
pub enum Message {
    // Project and files
    /// Show the native folder picker.
    OpenFolderDialog,
    /// Folder picker (or CLI argument) result; `None` when cancelled.
    FolderPicked(Option<PathBuf>),
    /// Background file listing of the project finished.
    FilesListed(Vec<PathBuf>),
    /// Files changed on disk outside the editor (from the file watcher).
    FilesChanged(Vec<PathBuf>),
    /// Saved tabs were read back: tabs with their text, active index.
    SessionRestored(Vec<(crate::app::SavedTab, String)>, Option<usize>),
    /// Show the action list for a tree entry (right click).
    ShowFileActions(PathBuf),
    /// The editor area was laid out at this size (for soft wrap).
    EditorResized(iced::Size),
    /// Replacement typed in the Find panel for "Replace in path".
    SearchReplaceText(String),
    /// Replace every hit of the last find in path (asks once to confirm).
    ReplaceInPath,
    /// Text typed in the refactoring prompt.
    RefactorInput(String),
    /// Confirm the refactoring prompt.
    RefactorSubmit,
    /// A project-wide refactoring was planned (change signature, move class).
    RefactorPlanned(Result<phpthunder::refactor::RenamePlan, String>),
    /// Name typed in the file name prompt.
    PromptInput(String),
    /// Confirm the file name prompt.
    PromptSubmit,
    /// Background project indexing finished.
    Indexed(Result<IndexReport, String>),
    /// Laravel key scan finished (`None` when not a Laravel project).
    LaravelScanned(LaravelScan),
    /// Expand or collapse a directory in the file tree.
    ToggleDir(PathBuf),
    /// Open a file from the tree or palette, recording navigation history.
    OpenFile(PathBuf),
    /// Async file read finished: path, contents or error, cursor target.
    FileLoaded(PathBuf, Result<String, String>, Option<Pos>),
    /// Activate the editor tab at this index.
    SelectTab(usize),
    /// Close the editor tab at this index.
    CloseTab(usize),
    // Editor
    /// Raw `text_editor` action (typing, clicks, motions).
    Edit(Action),
    /// IDE command resolved from a shortcut or menu.
    Command(Cmd),
    /// Bracket or quote typed; auto-paired or skipped over.
    TypeChar(char),
    /// Enter with auto-indent.
    SmartEnter,
    /// Backspace that also deletes empty bracket pairs.
    Backspace,
    /// Tab (`false`) or Shift+Tab outdent (`true`).
    Indent(bool),
    /// Editor scrollable moved or resized.
    Scrolled(Viewport),
    /// Move the completion selection by this delta (wraps around).
    CompletionMove(i32),
    /// Insert the selected completion item (Enter).
    CompletionAccept,
    /// Insert the selected item over the rest of the word (Tab).
    CompletionReplace,
    /// Insert the completion item at this index (mouse click).
    CompletionPick(usize),
    /// Escape: dismiss completion, hover, notice, and the find bar.
    CloseOverlays,
    /// Mouse moved over the editor, in editor-local coordinates.
    MouseMoved(Point),
    /// Mouse left the editor area.
    MouseLeft,
    /// Hover delay elapsed for this generation; stale ones are ignored.
    HoverFire(u64),
    // Usages and bottom panel
    /// Async find-usages finished; `None` when no symbol was under the cursor.
    UsagesFound(Option<Box<(Definition, Vec<Usage>)>>),
    /// Jump to the usage row at this index.
    OpenUsage(usize),
    /// Show a tool panel, in the area it is docked to.
    SelectBottom(Bottom),
    /// Hide the panel area on this side.
    ClosePanels(Dock),
    /// Move a tool panel to the other side (bottom <-> right).
    MovePanel(Bottom),
    // Palette
    /// Palette input text changed.
    PaletteQuery(String),
    /// Enter in the palette: open the selected entry.
    PaletteSubmit,
    /// Open the palette entry at this index (mouse click).
    PalettePick(usize),
    // Find in file
    /// Find bar search text changed.
    FindQuery(String),
    /// Find bar replacement text changed.
    ReplaceText(String),
    /// Flip a find-bar option (case, word, regex).
    FindToggle(Toggle),
    /// Jump to the next (`true`) or previous (`false`) match.
    FindStep(bool),
    /// Show or hide the replace row of the find bar.
    ToggleReplace,
    /// Replace the current match and advance.
    ReplaceOne,
    /// Replace every match in the active document.
    ReplaceAll,
    /// Close the find bar and re-focus the editor.
    CloseFind,
    // Find in path
    /// Find-in-path query text changed.
    SearchQuery(String),
    /// Find-in-path file mask changed.
    SearchMask(String),
    /// Flip a find-in-path option, including vendor inclusion.
    SearchToggle(Toggle),
    /// Start the find-in-path search.
    SearchSubmit,
    /// Background find-in-path finished, or the pattern was invalid.
    SearchDone(Result<SearchResults, String>),
    /// Open the hit at (file index, hit index) in the results panel.
    OpenSearchHit(usize, usize),
    /// Close the palette or find-in-path dialog.
    CloseModal,
    // Window
    /// Show or hide the project sidebar.
    ToggleSidebar,
    /// Mouse pressed on a pane splitter: start resizing that pane.
    ResizeStart(crate::app::Splitter),
    /// Cursor moved while a splitter is dragged (window coordinates).
    ResizeMove(Point),
    /// Mouse released: stop resizing.
    ResizeEnd,
    /// Double-click on a splitter: give its pane the default size.
    ResizeReset(crate::app::Splitter),
    /// The mouse entered (`Some`) or left (`None`) a splitter handle.
    SplitterHover(Option<crate::app::Splitter>),
    /// Start dragging the borderless window from the title bar.
    DragWindow,
    /// Maximize or restore the window (title bar double-click).
    ToggleMaximize,
    /// Global keyboard event; the flag is `true` when a widget captured it.
    Keyboard(keyboard::Event, bool),
    // Inspections and intentions
    /// Debounce timer for inspections fired; stale generations are ignored.
    Inspect(u64),
    /// Inspection results for a file at a document version.
    Inspected(
        PathBuf,
        u64,
        Vec<Diagnostic>,
        Vec<phpthunder::laravel::GutterMarker>,
    ),
    /// A Laravel gutter marker was clicked.
    Gutter(phpthunder::laravel::GutterAction),
    /// Move the `⌥↵` menu selection by this many rows.
    IntentionMove(i32),
    /// Apply the selected `⌥↵` entry.
    IntentionAccept,
    /// Apply the `⌥↵` entry at this index (mouse).
    IntentionPick(usize),
    /// Jump to the problem at this index of the Problems panel.
    OpenProblem(usize),
    // Refactoring
    /// The rename dialog's text changed.
    RenameInput(String),
    /// Confirm the rename dialog.
    RenameSubmit,
    /// A rename plan computed in the background, ready to apply.
    RenamePlanned(Result<RenamePlan, String>),
    /// Jump to the hierarchy panel row at this index.
    OpenHierarchy(usize),
    // Terminal
    /// The shell of this session wrote output (redraw).
    TerminalOutput(u64),
    /// A console panel (terminal or Claude) was laid out at this size.
    TerminalResized(Bottom, iced::Size),
    /// A console panel was clicked: keys now go to its session.
    TerminalFocus(Bottom),
    /// Clipboard contents to paste into the shell.
    TerminalPaste(Option<String>),
    /// Start a console panel's session again after it exited.
    TerminalRestart(Bottom),
    /// Mouse wheel over a console panel, in lines (positive is up).
    TerminalScroll(Bottom, i32),
    // Tools
    /// Run tests in this scope.
    RunTests(TestTarget),
    /// A test run finished.
    TestsDone(Result<crate::testing::TestRun, String>),
    /// Open the test case at this index of the Tests panel.
    OpenTest(usize),
    /// The formatter finished: file, document version, result (`None` when
    /// the project has no formatter).
    Formatted(
        PathBuf,
        u64,
        Result<Option<(String, crate::formatter::Formatter)>, String>,
    ),
    /// PHPStan finished analysing a saved file.
    PhpstanDone(PathBuf, Result<Vec<crate::phpstan::Finding>, String>),
    // Git
    /// Git status arrived (`Err` outside a repository or on git errors).
    GitStatus(Result<crate::git::RepoStatus, String>),
    /// `HEAD` content of an open file arrived (`None`: not in `HEAD`).
    GitBase(PathBuf, Option<String>),
    /// Re-read the Git status.
    GitRefresh,
    /// Stage the file if it is not staged, else unstage it.
    GitToggleStage(PathBuf),
    /// Stage (`true`) or unstage (`false`) every changed file.
    GitStageAll(bool),
    /// Show the diff of a changed file in the Commit panel.
    GitSelect(PathBuf),
    /// The diff of the selected file is ready.
    GitDiff(PathBuf, Vec<crate::git::DiffLine>),
    /// Open the file of the selected diff at a 1-based line.
    GitOpenLine(usize),
    /// Commit message edited.
    GitCommitText(String),
    /// Commit the staged files, then push when `true`.
    GitCommit(bool),
    /// Discard a file's changes; the first press asks for confirmation.
    GitDiscard(PathBuf),
    /// Pull the current branch.
    GitPull,
    /// Push the current branch.
    GitPush,
    /// Show the branch picker.
    GitShowBranches,
    /// The branch list for the picker arrived.
    GitBranches(Result<Vec<crate::git::Branch>, String>),
    /// A Git operation finished.
    GitDone(GitOp, Result<String, String>),
    // Debugger
    /// Debug the test at the caret (or the tests panel's context).
    DebugTest,
    /// An event from the debug session.
    Debug(crate::debug::DebugEvent),
    /// Send a command to the debug session.
    DebugSend(crate::debug::DebugCommand),
    /// Show the variables of a stack frame.
    DebugFrame(usize),
    /// Expand or collapse a variable, by its expression.
    DebugToggleVar(String),
    /// Expression typed in the evaluate field.
    DebugEvalText(String),
    /// Evaluate the typed expression.
    DebugEvalSubmit,
    /// The mouse moved over the line-number gutter (y in content px).
    GutterMoved(f32),
    /// The line-number gutter was clicked: toggle a breakpoint there.
    GutterPressed,
    /// Development: capture the window (see `PHPTHUNDER_SNAPSHOT`).
    TakeSnapshot,
    /// Development: type text and open completion before the snapshot.
    SnapshotType(String),
    /// Development: the window capture arrived.
    Snapshot(iced::window::Screenshot),
}

/// Where a debug session is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugState {
    /// Waiting for Xdebug to connect.
    Starting,
    /// The script runs.
    Running,
    /// Stopped at a line.
    Paused,
    /// The session ended.
    Finished,
}

/// State of the debugger panel and the session behind it.
pub struct DebugView {
    /// Commands to the session.
    pub commands: tokio::sync::mpsc::UnboundedSender<crate::debug::DebugCommand>,
    /// What is being debugged (the test scope label).
    pub title: String,
    /// Session state.
    pub state: DebugState,
    /// Call stack while paused, innermost first.
    pub frames: Vec<crate::debug::Frame>,
    /// Selected frame.
    pub frame: usize,
    /// Variables of the selected frame.
    pub variables: Vec<crate::debug::Variable>,
    /// Members loaded on demand, by variable expression.
    pub children: std::collections::HashMap<String, Vec<crate::debug::Variable>>,
    /// Expanded variables, by expression.
    pub expanded: std::collections::HashSet<String>,
    /// Lines the test process printed.
    pub output: Vec<String>,
    /// Expression being typed for evaluation.
    pub eval: String,
    /// Past evaluations, most recent last.
    pub evaluations: Vec<(String, Result<crate::debug::Variable, String>)>,
}

impl DebugView {
    /// File and 0-based line where execution is paused.
    pub fn execution_point(&self) -> Option<(&std::path::Path, usize)> {
        (self.state == DebugState::Paused)
            .then(|| self.frames.get(self.frame))
            .flatten()
            .map(|f| (f.file.as_path(), f.line))
    }
}

/// A Git operation started from the IDE, reported by [`Message::GitDone`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitOp {
    /// Staging or unstaging files.
    Stage,
    /// A commit; pushes afterwards when `true`.
    Commit(bool),
    /// A push.
    Push,
    /// A pull.
    Pull,
    /// Discarding a file's changes.
    Discard,
    /// Checking out a branch.
    Checkout,
    /// Creating a branch.
    CreateBranch,
}

impl GitOp {
    /// Whether the operation may change files on disk (open tabs reload).
    pub fn changes_files(&self) -> bool {
        matches!(self, GitOp::Pull | GitOp::Discard | GitOp::Checkout)
    }

    /// Progress label while it runs.
    pub fn progress(&self) -> String {
        match self {
            GitOp::Stage => tr!("git-staging"),
            GitOp::Commit(_) => tr!("git-committing"),
            GitOp::Push => tr!("git-pushing"),
            GitOp::Pull => tr!("git-pulling"),
            GitOp::Discard => tr!("git-discarding"),
            GitOp::Checkout | GitOp::CreateBranch => tr!("git-switching"),
        }
    }
}

/// State of the Commit panel.
#[derive(Default)]
pub struct CommitPanel {
    /// Commit message being written.
    pub message: String,
    /// File whose diff is shown.
    pub selected: Option<PathBuf>,
    /// Diff of `selected` against `HEAD`.
    pub diff: Vec<crate::git::DiffLine>,
    /// Operation in progress, shown instead of the buttons.
    pub busy: Option<GitOp>,
    /// File whose discard waits for a second press.
    pub confirm_discard: Option<PathBuf>,
}

/// Which tests a [`Message::RunTests`] runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestTarget {
    /// The test at the caret, else the current test file, else everything.
    Context,
    /// The current file.
    File,
    /// The whole suite.
    All,
    /// The previous run again.
    Again,
}

/// Content of the Tests panel.
pub enum Tests {
    /// A run is in progress for this scope label.
    Running(String),
    /// The last run.
    Done(crate::testing::TestRun),
}

/// Tool panels (tabs of the bottom or right panel area).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Bottom {
    /// Find-usages results.
    Usages,
    /// Find-in-path results.
    Find,
    /// Inspection results for the active file.
    Problems,
    /// Supertypes and subtypes of a class.
    Hierarchy,
    /// The integrated terminal.
    Terminal,
    /// The Claude CLI, running in its own terminal session.
    Claude,
    /// Test results.
    Tests,
    /// Git changes and commit.
    Commit,
    /// The debugger.
    Debug,
}

/// Where a tool panel is docked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dock {
    /// Below the editor.
    Bottom,
    /// Right of the editor.
    Right,
}

/// State of the background project index, shown in the status bar.
pub enum Indexing {
    /// No project opened yet.
    Idle,
    /// Indexing task in flight.
    Running,
    /// Indexing finished with these stats.
    Done(IndexReport),
}

/// Open completion popup state; rebuilt by [`App::refresh_completion`].
pub struct Completion {
    /// Ranked proposals, best first, with their matched ranges.
    pub items: Vec<phpthunder::analysis::Ranked>,
    /// Index of the highlighted candidate.
    pub selected: usize,
    /// Byte offset where the replaced prefix starts.
    pub start: usize,
    /// Visual position of the prefix start, for placing the popup.
    pub line: usize,
    /// Visual column of the prefix start.
    pub col: usize,
}

/// Quick-documentation popup for the symbol under the mouse.
pub struct Hover {
    /// Editor line the popup anchors to.
    pub line: usize,
    /// Visual column of the hovered identifier's start.
    pub col: usize,
    /// Declaration signature, rendered as code.
    pub signature: String,
    /// `Owner::member · file:line` of the declaration.
    pub location: String,
    /// Doc comment text of the declaration.
    pub doc: String,
    /// Inspection messages for the hovered range, shown first.
    pub problems: Vec<(phpthunder::inspection::Severity, String)>,
}

/// One line of the usages panel.
pub struct UsageRow {
    /// File containing the usage.
    pub path: PathBuf,
    /// Start of the usage (UTF-16 position).
    pub pos: Pos,
    /// `file:line` label.
    pub label: String,
    /// Trimmed source line of the usage.
    pub preview: String,
    /// Whether this row is the declaration rather than a use.
    pub is_declaration: bool,
}

/// Content of the usages panel.
pub enum Usages {
    /// Search for this symbol name is in flight.
    Searching(String),
    /// Search finished.
    Found {
        /// Summary header, e.g. `Foo::bar() — 3 usages in 2 files`.
        title: String,
        /// One row per usage, declaration included.
        rows: Vec<UsageRow>,
        /// Row last opened, highlighted in the panel.
        selected: Option<usize>,
    },
}

/// Content of the find-in-path results panel.
pub enum Search {
    /// Search for this query is in flight.
    Running(String),
    /// Results plus the (file, hit) last opened.
    Done(SearchResults, Option<(usize, usize)>),
}

/// A place in a file, for back/forward navigation.
#[derive(Debug, Clone)]
pub struct Location {
    /// File of the location.
    pub path: PathBuf,
    /// Cursor position (UTF-16).
    pub pos: Pos,
}

/// An entry of the `⌘N` Generate menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Generation {
    /// A constructor assigning the uninitialized properties.
    Constructor,
    /// Getters for properties that lack one.
    Getters,
    /// Setters for properties that lack one.
    Setters,
    /// Both getters and setters.
    GettersAndSetters,
    /// Stubs for abstract and interface methods.
    ImplementMethods,
    /// Opens the list of parent methods to override.
    OverrideMethods,
    /// Overrides the named parent method.
    Override(String),
}

impl Generation {
    /// Menu label.
    pub fn title(&self) -> String {
        match self {
            Generation::Constructor => tr!("generate-constructor"),
            Generation::Getters => tr!("generate-getters"),
            Generation::Setters => tr!("generate-setters"),
            Generation::GettersAndSetters => tr!("generate-getters-setters"),
            Generation::ImplementMethods => tr!("intention-implement-methods"),
            Generation::OverrideMethods => tr!("generate-override"),
            Generation::Override(name) => name.clone(),
        }
    }
}

/// One entry of the `⌥↵` menu.
#[derive(Debug, Clone)]
pub enum Intention {
    /// A quick fix for a diagnostic at the caret.
    Fix(Fix),
    /// Generate stubs for the abstract/interface methods `class` lacks.
    ImplementMethods {
        /// FQN of the class at the caret.
        class: String,
    },
    /// Remove unused imports and sort the rest.
    OptimizeImports,
    /// A refactoring that asks for a name or a list first.
    Refactor(RefactorAction),
    /// Replace the variable at the caret by its value.
    InlineVariable,
}

/// A refactoring that needs input from a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefactorAction {
    /// Extract the expression in this byte range into a variable.
    ExtractVariable(usize, usize),
    /// Extract the statements in this byte range into a method.
    ExtractMethod(usize, usize),
    /// Change the parameters of the function or method at this offset.
    ChangeSignature(usize),
    /// Move the class of the active file to another namespace.
    MoveClass,
}

/// The modal asking for the input of a [`RefactorAction`].
pub struct RefactorPrompt {
    /// What the input is for.
    pub action: RefactorAction,
    /// Dialog title.
    pub title: String,
    /// Help shown under the input.
    pub help: String,
    /// Text being typed.
    pub value: String,
    /// Why the last submit failed.
    pub error: Option<String>,
}

impl Intention {
    /// Menu label.
    pub fn title(&self) -> String {
        match self {
            Intention::Fix(fix) => crate::i18n::fix_title(fix),
            Intention::ImplementMethods { .. } => tr!("intention-implement-methods"),
            Intention::OptimizeImports => tr!("intention-optimize-imports"),
            Intention::Refactor(RefactorAction::ExtractVariable(..)) => {
                tr!("refactor-extract-variable")
            }
            Intention::Refactor(RefactorAction::ExtractMethod(..)) => {
                tr!("refactor-extract-method")
            }
            Intention::Refactor(RefactorAction::ChangeSignature(_)) => {
                tr!("refactor-change-signature")
            }
            Intention::Refactor(RefactorAction::MoveClass) => tr!("refactor-move-class"),
            Intention::InlineVariable => tr!("refactor-inline-variable"),
        }
    }
}

/// Open `⌥↵` menu, anchored at the caret.
pub struct Intentions {
    /// Entries, quick fixes first.
    pub items: Vec<Intention>,
    /// Highlighted entry.
    pub selected: usize,
    /// Caret line the menu anchors to.
    pub line: usize,
    /// Caret visual column.
    pub col: usize,
}

/// Parameter info popup, shown above the call being typed.
pub struct SignatureView {
    /// Callee signature and active parameter.
    pub help: SignatureHelp,
    /// Caret line the popup anchors to.
    pub line: usize,
    /// Caret visual column.
    pub col: usize,
}

/// The rename dialog (`⇧F6`).
pub struct RenameDialog {
    /// New name being typed.
    pub name: String,
    /// Name before the rename, shown in the title.
    pub original: String,
    /// Byte offset of the symbol in the active document.
    pub offset: usize,
    /// Last error from planning the rename.
    pub error: Option<String>,
}

/// A file operation from the project tree's action list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileAction {
    /// Create a file in this folder (asks for the name).
    NewFile(PathBuf),
    /// Create a folder in this folder (asks for the name).
    NewFolder(PathBuf),
    /// Create a PHP class in this folder, namespaced from Composer's PSR-4
    /// autoload (asks for the class name).
    NewClass(PathBuf),
    /// Rename or move this file or folder (asks for the new relative path).
    Rename(PathBuf),
    /// Move this file or folder to the Trash.
    Delete(PathBuf),
    /// Show this file or folder in Finder.
    Reveal(PathBuf),
    /// Copy the path relative to the project to the clipboard.
    CopyPath(PathBuf),
}

/// The modal asking for a name for a [`FileAction`].
pub struct NamePrompt {
    /// What the name is for.
    pub action: FileAction,
    /// Dialog title.
    pub title: String,
    /// Name or relative path being typed.
    pub value: String,
    /// Why the last submit failed.
    pub error: Option<String>,
}

/// One row of the type hierarchy panel.
pub struct HierarchyRow {
    /// Indentation level (supertypes above, subtypes below the class).
    pub depth: usize,
    /// Class name.
    pub label: String,
    /// Namespace and file.
    pub detail: String,
    /// Where the class is declared, when indexed.
    pub target: Option<(PathBuf, Pos)>,
}

/// Content of the type hierarchy panel.
pub struct Hierarchy {
    /// Panel header.
    pub title: String,
    /// Rows: the class, its supertypes, then its subtypes.
    pub rows: Vec<HierarchyRow>,
}
