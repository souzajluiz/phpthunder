//! Keyboard shortcuts (macOS layout). One table serves both the focused
//! editor and the window, so a shortcut works wherever focus is.

use iced::keyboard::{Key, Modifiers, key::Named};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    // Files and window
    Save,
    CloseTab,
    ToggleSidebar,
    NextTab,
    PrevTab,
    // Editing
    Undo,
    Redo,
    ToggleComment,
    Duplicate,
    DeleteLine,
    MoveLineUp,
    MoveLineDown,
    Complete,
    // Navigation
    GotoDeclaration,
    FindUsages,
    GotoLine,
    GotoFile,
    GotoClass,
    GotoSymbol,
    RecentFiles,
    SearchEverywhere,
    Back,
    Forward,
    // Search
    Find,
    Replace,
    FindNext,
    FindPrevious,
    FindInPath,
}

/// The command bound to a key press, if any.
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

/// The labels shown in menus and hints.
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
    }
}
