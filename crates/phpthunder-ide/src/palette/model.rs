//! Navigation popup data: modes, targets, items and the open popup.

use crate::tr;
use phpthunder::syntax::Pos;
use std::path::PathBuf;

/// Widget id of the query input, focused by `open_palette` in the `app` command handlers.
pub const INPUT_ID: &str = "palette-input";

/// Cap on ranked results kept per query.
pub(crate) const MAX_ITEMS: usize = 60;

/// Rows shown at once; the list window follows the selection.
pub(crate) const VISIBLE_ROWS: usize = 12;

/// Height in px of one result row.
pub(crate) const ROW: f32 = 30.0;

/// What the palette searches, chosen by the shortcut that opened it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Project files, matched by name then by relative path.
    Files,
    /// Class-like symbols: classes, interfaces, traits and enums.
    Classes,
    /// Every non-virtual indexed symbol, members included.
    Symbols,
    /// Recently opened files, most recent first.
    Recent,
    /// A `line[:column]` in the active document.
    Line,
    /// Files and symbols together, files slightly demoted.
    Everywhere,
    /// Symbols declared in the active file (`⌘F12`).
    Structure,
    /// A fixed list of targets to choose from (e.g. implementations);
    /// the list and its title live in [`Palette::fixed`].
    Choose,
    /// A JSONPath (JSON, YAML) or XPath (XML, HTML) evaluated on the
    /// active document; the matches are listed.
    Query,
}

impl Mode {
    /// Mode name shown in the palette footer.
    pub(crate) fn title(self) -> String {
        match self {
            Mode::Files => tr!("action-go-to-file"),
            Mode::Classes => tr!("action-go-to-class"),
            Mode::Symbols => tr!("action-go-to-symbol"),
            Mode::Recent => tr!("action-recent-files"),
            Mode::Line => tr!("action-go-to-line"),
            Mode::Everywhere => tr!("action-search-everywhere"),
            Mode::Structure => tr!("action-file-structure"),
            Mode::Choose => tr!("action-choose"),
            Mode::Query => tr!("action-query"),
        }
    }

    /// Placeholder text for the query input.
    pub(crate) fn placeholder(self) -> String {
        match self {
            Mode::Line => tr!("palette-placeholder-line"),
            Mode::Recent => tr!("palette-placeholder-recent"),
            Mode::Query => tr!("palette-placeholder-query"),
            _ => tr!("palette-placeholder"),
        }
    }
}

/// Where picking an item navigates; handled by `palette_open_selected` in the `app` command handlers.
#[derive(Debug, Clone)]
pub enum Target {
    /// Open a file.
    File(PathBuf),
    /// Open a file with the caret at a symbol's name.
    Symbol(PathBuf, Pos),
    /// Move the caret in the active document to `(line, visual column)`, both 0-based.
    Line(usize, usize),
    /// Run this command line in the integrated terminal.
    Command(String),
    /// Type this command line in the integrated terminal without running
    /// it, for the user to finish (`composer require `).
    TerminalDraft(String),
    /// Insert generated code into the class at the caret.
    Generate(crate::app::Generation),
    /// Insert the live template with this abbreviation at the caret.
    Template(&'static str),
    /// Check out this Git branch.
    Checkout(crate::git::Branch),
    /// Create and check out a Git branch with this name.
    CreateBranch(String),
    /// Rebase the current branch onto this one.
    Rebase(String),
    /// Merge this branch into the current one.
    Merge(String),
    /// Apply and drop this stash (`pop`).
    Unstash(usize),
    /// Delete this stash.
    DropStash(usize),
    /// Run a file operation from the project tree.
    FileAction(crate::app::FileAction),
    /// Open a file with the caret at the start of a 0-based line.
    Location(std::path::PathBuf, usize),
    /// Paste this text at the caret (clipboard history).
    Paste(String),
    /// Compare a local history version with the active file.
    LocalVersion(std::path::PathBuf),
    /// Run a command (Find Action).
    Action(crate::keymap::Cmd),
    /// Surround the selection with a block construct.
    Surround(crate::editor::Surround),
    /// Put this mnemonic bookmark (digit or letter) on the caret line.
    Mnemonic(char),
    /// Make this task (by id) the active one.
    ActivateTask(String),
    /// Create a task with this title and make it active.
    NewTask(String),
    /// Track this GitHub issue (number, title, url) and make it active.
    ActivateIssue(u64, String, String),
    /// Use this environment for `.http` requests.
    HttpEnvironment(String),
}

/// One ranked palette result.
#[derive(Debug, Clone)]
pub struct Item {
    /// Kind marker in the left column (`▤` file, `#` line, or a symbol letter).
    pub badge: &'static str,
    /// Main label; matched characters are drawn in the accent color.
    pub title: String,
    /// Character indices of `title` that matched the query.
    pub matched: Vec<usize>,
    /// Right-aligned context: relative path, or namespace and `file:line`.
    pub detail: String,
    /// Destination when the item is picked.
    pub target: Target,
}

/// State of the open navigation palette, held in `App::palette` while visible.
pub struct Palette {
    /// What is being searched.
    pub mode: Mode,
    /// Current input text.
    pub query: String,
    /// Ranked results for `query`, recomputed on every keystroke.
    pub items: Vec<Item>,
    /// Index into `items` of the highlighted row.
    pub selected: usize,
    /// For [`Mode::Choose`]: the footer title and the full list the query
    /// filters.
    pub fixed: Option<(String, Vec<Item>)>,
    /// For [`Mode::Choose`]: builds an extra entry from the query when no
    /// title equals it (e.g. "create branch <query>").
    pub extra: Option<fn(&str) -> Option<Item>>,
}
