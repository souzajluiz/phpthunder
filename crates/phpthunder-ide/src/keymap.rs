//! Keyboard shortcuts (macOS layout). One table serves both the focused
//! editor and the window, so a shortcut works wherever focus is.

use iced::keyboard::{Key, Modifiers, key::Named};

/// A shortcut-triggered command. Produced by `command` and dispatched by the
/// key handling in the `app` module; `hint` gives its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    // Files and window
    /// Write the active document to disk.
    Save,
    /// Close the active tab.
    CloseTab,
    /// Show or hide the project tree.
    ToggleSidebar,
    /// Activate the tab to the right.
    NextTab,
    /// Activate the tab to the left.
    PrevTab,
    // Editing
    /// Undo the last edit.
    Undo,
    /// Redo the last undone edit.
    Redo,
    /// Comment or uncomment the selected lines.
    ToggleComment,
    /// Duplicate the current line or selection.
    Duplicate,
    /// Delete the current line.
    DeleteLine,
    /// Move the current line up.
    MoveLineUp,
    /// Move the current line down.
    MoveLineDown,
    /// Open code completion at the caret.
    Complete,
    // Navigation
    /// Jump to the declaration of the symbol at the caret.
    GotoDeclaration,
    /// List usages of the symbol at the caret in the bottom panel.
    FindUsages,
    /// Open the palette in go-to-line mode.
    GotoLine,
    /// Open the palette in go-to-file mode.
    GotoFile,
    /// Open the palette in go-to-class mode.
    GotoClass,
    /// Open the palette in go-to-symbol mode.
    GotoSymbol,
    /// Open the palette on recently opened files.
    RecentFiles,
    /// Open the palette searching files and symbols together.
    SearchEverywhere,
    /// Jump back through the navigation history.
    Back,
    /// Jump forward through the navigation history.
    Forward,
    // Search
    /// Open the find bar.
    Find,
    /// Open the find bar with the replace row.
    Replace,
    /// Go to the next find match.
    FindNext,
    /// Go to the previous find match.
    FindPrevious,
    /// Open the find-in-path dialog.
    FindInPath,
    /// Quick fixes and intentions at the caret (`⌥↵`).
    QuickFix,
    /// Parameter info for the call at the caret (`⌘P`).
    ParameterInfo,
    /// Rename the symbol at the caret (`⇧F6`).
    Rename,
    /// Remove unused imports and sort the rest (`⌃⌥O`).
    OptimizeImports,
    /// Go to subclasses or overriding methods (`⌥⌘B`).
    GotoImplementation,
    /// Show the type hierarchy panel (`⌃H`).
    TypeHierarchy,
    /// Popup with the symbols of the current file (`⌘F12`).
    FileStructure,
    /// Show the Problems panel (`⌘6`).
    Problems,
}

/// Maps a key press and its modifiers to the bound `Cmd`, or `None` if
/// unbound. Called from both the editor's key binding and the window-level
/// keyboard subscription in the `app` module, so a shortcut works wherever focus is.
/// `⌘` means `Modifiers::command`, which is Ctrl outside macOS.
pub fn command(key: &Key, m: Modifiers) -> Option<Cmd> {
    use Cmd::*;
    let cmd = m.command();
    let (shift, alt) = (m.shift(), m.alt());
    Some(match key.as_ref() {
        Key::Character(c) if cmd => match (c.to_ascii_lowercase().as_str(), shift, alt) {
            ("s", false, false) => Save,
            ("w", false, false) => CloseTab,
            ("1", false, false) => ToggleSidebar,
            ("z", false, false) => Undo,
            ("z", true, false) => Redo,
            ("/", false, false) => ToggleComment,
            ("d", false, false) => Duplicate,
            ("b", false, false) => GotoDeclaration,
            ("b", false, true) => GotoImplementation,
            ("p", false, false) => ParameterInfo,
            ("6", false, false) => Problems,
            ("l", false, false) => GotoLine,
            ("o", true, false) => GotoFile,
            ("o", false, false) => GotoClass,
            ("o", false, true) => GotoSymbol,
            ("e", false, false) => RecentFiles,
            ("f", false, false) => Find,
            ("r", false, false) => Replace,
            ("g", false, false) => FindNext,
            ("g", true, false) => FindPrevious,
            ("f", true, false) => FindInPath,
            ("[", false, false) => Back,
            ("]", false, false) => Forward,
            ("}", true, false) | ("]", true, false) => NextTab,
            ("{", true, false) | ("[", true, false) => PrevTab,
            _ => return None,
        },
        Key::Character(c) if m.control() && alt && c.eq_ignore_ascii_case("o") => OptimizeImports,
        Key::Character(c) if m.control() && !cmd && c.eq_ignore_ascii_case("h") => TypeHierarchy,
        Key::Named(Named::Enter) if alt && !cmd => QuickFix,
        Key::Named(Named::F6) if shift => Rename,
        Key::Named(Named::F12) if cmd => FileStructure,
        Key::Named(Named::F12) if shift => FindUsages,
        Key::Named(Named::F12) => GotoDeclaration,
        Key::Named(Named::F7) if alt => FindUsages,
        Key::Named(Named::F3) if shift => FindPrevious,
        Key::Named(Named::F3) => FindNext,
        Key::Named(Named::Backspace) if cmd => DeleteLine,
        Key::Named(Named::ArrowUp) if alt && shift => MoveLineUp,
        Key::Named(Named::ArrowDown) if alt && shift => MoveLineDown,
        Key::Named(Named::ArrowLeft) if cmd && alt => Back,
        Key::Named(Named::ArrowRight) if cmd && alt => Forward,
        Key::Named(Named::Space) if m.control() => Complete,
        Key::Character(" ") if m.control() => Complete,
        _ => return None,
    })
}

/// The glyph label for `cmd`'s shortcut (e.g. `⌘S`), shown in the empty-editor
/// shortcut list in the workbench view. Kept in sync with `command` by hand.
pub fn hint(cmd: Cmd) -> &'static str {
    use Cmd::*;
    match cmd {
        Save => "⌘S",
        CloseTab => "⌘W",
        ToggleSidebar => "⌘1",
        NextTab => "⇧⌘]",
        PrevTab => "⇧⌘[",
        Undo => "⌘Z",
        Redo => "⇧⌘Z",
        ToggleComment => "⌘/",
        Duplicate => "⌘D",
        DeleteLine => "⌘⌫",
        MoveLineUp => "⌥⇧↑",
        MoveLineDown => "⌥⇧↓",
        Complete => "⌃Space",
        GotoDeclaration => "⌘B",
        FindUsages => "⌥F7",
        GotoLine => "⌘L",
        GotoFile => "⇧⌘O",
        GotoClass => "⌘O",
        GotoSymbol => "⌥⌘O",
        RecentFiles => "⌘E",
        SearchEverywhere => "⇧⇧",
        Back => "⌘[",
        Forward => "⌘]",
        Find => "⌘F",
        Replace => "⌘R",
        FindNext => "⌘G",
        FindPrevious => "⇧⌘G",
        FindInPath => "⇧⌘F",
        QuickFix => "⌥↵",
        ParameterInfo => "⌘P",
        Rename => "⇧F6",
        OptimizeImports => "⌃⌥O",
        GotoImplementation => "⌥⌘B",
        TypeHierarchy => "⌃H",
        FileStructure => "⌘F12",
        Problems => "⌘6",
    }
}
