//! Navigation popup data: modes, targets, items and the open popup.

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
}

impl Mode {
    /// Mode name shown in the palette footer.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Mode::Files => "Go to file",
            Mode::Classes => "Go to class",
            Mode::Symbols => "Go to symbol",
            Mode::Recent => "Recent files",
            Mode::Line => "Go to line",
            Mode::Everywhere => "Search everywhere",
            Mode::Structure => "File structure",
            Mode::Choose => "Choose",
        }
    }

    /// Placeholder text for the query input.
    pub(crate) fn placeholder(self) -> &'static str {
        match self {
            Mode::Line => "line[:column]",
            Mode::Recent => "Filter recent files",
            _ => "Type a name",
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
}
