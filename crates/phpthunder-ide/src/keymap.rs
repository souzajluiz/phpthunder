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
    /// Show or hide the terminal (`⌥F12`).
    ToggleTerminal,
    /// Show or hide the Claude panel, starting the `claude` CLI (`⌥⌘A`).
    ToggleClaude,
    /// Run the tests for the caret's context (`⌃⇧R`).
    RunTests,
    /// Reformat the file with the project's formatter (`⌥⌘L`).
    Reformat,
    /// Pick a Composer script or command to run (`⌃⌥C`).
    Composer,
    /// Generate code in the class at the caret (`⌘N`).
    Generate,
    /// Pick a live template to insert at the caret (`⌘J`).
    InsertTemplate,
    /// Open the Git Commit panel (`⌘K`).
    Commit,
    /// Push the current branch (`⇧⌘K`).
    Push,
    /// Pull the current branch (`⌘T`).
    Pull,
    /// Add a caret on the line above (`⌥⌘↑`).
    AddCaretAbove,
    /// Add a caret on the line below (`⌥⌘↓`).
    AddCaretBelow,
    /// Select the next occurrence of the selection as another caret (`⌃G`).
    SelectNextOccurrence,
    /// Wrap long lines at the editor width, or stop wrapping (`⌥⌘W`).
    ToggleSoftWrap,
    /// Extract the selected expression into a variable (`⌥⌘V`).
    ExtractVariable,
    /// Extract the selected statements into a method (`⌥⌘M`).
    ExtractMethod,
    /// Inline the variable at the caret (`⌥⌘N`).
    InlineVariable,
    /// Change the parameters of the function at the caret (`⌘F6`).
    ChangeSignature,
    /// Move the class of the active file to another namespace (`F6`).
    MoveClass,
    /// Debug the test at the caret under Xdebug (`⌃⇧D`).
    DebugTest,
    /// Add or remove a breakpoint on the caret line (`⌘F8`).
    ToggleBreakpoint,
    /// Debugger: continue to the next breakpoint (`F9`).
    Resume,
    /// Debugger: next line in this function (`F8`).
    StepOver,
    /// Debugger: into the call on this line (`F7`).
    StepInto,
    /// Debugger: until the function returns (`⇧F8`).
    StepOut,
    /// Debugger: stop the session (`⌘F2`).
    StopDebug,
    // Editing actions
    /// Grow the selection to the next enclosing structure (`⌥↑`).
    ExtendSelection,
    /// Shrink the selection back one step (`⌥↓`).
    ShrinkSelection,
    /// Join the next line onto this one (`⌃⇧J`).
    JoinLines,
    /// Break the line at the caret, caret staying put (`⌘↵`).
    SplitLine,
    /// Open a new line below, whatever the caret column (`⇧↵`).
    StartNewLine,
    /// Open a new line above (`⌥⌘↵`).
    StartNewLineBefore,
    /// Finish the statement and move to the next line (`⇧⌘↵`).
    CompleteStatement,
    /// Upper- or lower-case the selection or word (`⇧⌘U`).
    ToggleCase,
    /// Swap the characters around the caret (`⌃T`).
    Transpose,
    /// Comment or uncomment with `/* */` (`⌥⌘/`).
    BlockComment,
    /// Jump to the matching bracket (`⌃M`).
    MatchBrace,
    /// Jump to the start of the enclosing block (`⌥⌘[`).
    BlockStart,
    /// Jump to the end of the enclosing block (`⌥⌘]`).
    BlockEnd,
    /// Complete the word from words in the file, cycling (`⌥/`).
    WordCompletion,
    /// Remove the block around the caret, keeping its body (`⇧⌘⌦`).
    Unwrap,
    /// Surround the selection with a block construct (`⌥⌘T`).
    SurroundWith,
    /// Sort the selected lines (or the file).
    SortLines,
    /// Reverse the selected lines (or the file).
    ReverseLines,
    /// Remove repeated lines from the selection (or the file).
    UniqueLines,
    /// Add a caret at every occurrence of the selection (`⌃⌘G`).
    SelectAllOccurrences,
    /// Search every command by name and run it (`⇧⌘A`).
    FindAction,
    /// Go to the next problem in the file (`F2`).
    NextError,
    /// Go to the previous problem in the file (`⇧F2`).
    PreviousError,
    /// Documentation and problems of the symbol at the caret (`F1`, `⌃J`).
    QuickDoc,
    /// Show spaces and tabs as dots and arrows.
    ToggleWhitespace,
    /// Move the statement up (`⇧⌘↑`).
    MoveStatementUp,
    /// Move the statement down (`⇧⌘↓`).
    MoveStatementDown,
    /// Move the argument left (`⌥⇧⌘←`).
    MoveElementLeft,
    /// Move the argument right (`⌥⇧⌘→`).
    MoveElementRight,
    /// Delete to the end of the word (`⌥⌦`).
    DeleteWordEnd,
    /// Delete to the start of the word (`⌥⌫`).
    DeleteWordStart,
    /// One caret at the end of each selected line (`⌥⇧G`).
    AddCaretPerLine,
    /// Add or remove a bookmark on the line (`F3`).
    ToggleBookmark,
    /// List the bookmarks (`⌘F3`).
    ShowBookmarks,
    /// Go to the last edit location (`⇧⌘⌫`).
    JumpToLastEdit,
    /// Recent edit and navigation locations (`⇧⌘E`).
    RecentLocations,
    /// Paste from the clipboard history (`⇧⌘V`).
    PasteFromHistory,
    /// Copy a reference to the symbol at the caret (`⌥⇧⌘C`).
    CopyReference,
    /// Go to the method this one overrides (`⌘U`).
    GotoSuperMethod,
    /// Go to the class of the value at the caret (`⇧⌘B`).
    GotoTypeDeclaration,
    /// Switch between a class and its test (`⇧⌘T`).
    GotoTest,
    /// Make everything larger (`⌘=`).
    ZoomIn,
    /// Make everything smaller (`⌘−`).
    ZoomOut,
    /// Back to the normal size (`⌘0`).
    ZoomReset,
}

impl Cmd {
    /// Every command, in the order the Find Action palette lists them.
    pub const ALL: [Cmd; 106] = {
        use Cmd::*;
        [
            Save,
            CloseTab,
            ToggleSidebar,
            NextTab,
            PrevTab,
            Undo,
            Redo,
            ToggleComment,
            Duplicate,
            DeleteLine,
            MoveLineUp,
            MoveLineDown,
            Complete,
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
            Find,
            Replace,
            FindNext,
            FindPrevious,
            FindInPath,
            QuickFix,
            ParameterInfo,
            Rename,
            OptimizeImports,
            GotoImplementation,
            TypeHierarchy,
            FileStructure,
            Problems,
            ToggleTerminal,
            ToggleClaude,
            RunTests,
            Reformat,
            Composer,
            Generate,
            InsertTemplate,
            Commit,
            Push,
            Pull,
            AddCaretAbove,
            AddCaretBelow,
            SelectNextOccurrence,
            ToggleSoftWrap,
            ExtractVariable,
            ExtractMethod,
            InlineVariable,
            ChangeSignature,
            MoveClass,
            DebugTest,
            ToggleBreakpoint,
            Resume,
            StepOver,
            StepInto,
            StepOut,
            StopDebug,
            ExtendSelection,
            ShrinkSelection,
            JoinLines,
            SplitLine,
            StartNewLine,
            StartNewLineBefore,
            CompleteStatement,
            ToggleCase,
            Transpose,
            BlockComment,
            MatchBrace,
            BlockStart,
            BlockEnd,
            WordCompletion,
            Unwrap,
            SurroundWith,
            SortLines,
            ReverseLines,
            UniqueLines,
            SelectAllOccurrences,
            FindAction,
            NextError,
            PreviousError,
            QuickDoc,
            ToggleWhitespace,
            MoveStatementUp,
            MoveStatementDown,
            MoveElementLeft,
            MoveElementRight,
            DeleteWordEnd,
            DeleteWordStart,
            AddCaretPerLine,
            ToggleBookmark,
            ShowBookmarks,
            JumpToLastEdit,
            RecentLocations,
            PasteFromHistory,
            CopyReference,
            GotoSuperMethod,
            GotoTypeDeclaration,
            GotoTest,
            ZoomIn,
            ZoomOut,
            ZoomReset,
        ]
    };

    /// The command's name in the user's language (Find Action, menus).
    pub fn label(self) -> String {
        let name = format!("{self:?}");
        let mut id = String::from("cmd");
        for c in name.chars() {
            if c.is_ascii_uppercase() {
                id.push('-');
            }
            id.push(c.to_ascii_lowercase());
        }
        crate::i18n::translate(&id, None)
    }
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
            ("a", false, true) => ToggleClaude,
            ("l", false, true) => Reformat,
            ("p", false, false) => ParameterInfo,
            ("6", false, false) => Problems,
            ("l", false, false) => GotoLine,
            ("o", true, false) => GotoFile,
            ("o", false, false) => GotoClass,
            ("o", false, true) => GotoSymbol,
            ("e", false, false) => RecentFiles,
            ("n", false, false) => Generate,
            ("j", false, false) => InsertTemplate,
            ("k", false, false) => Commit,
            ("v", false, true) => ExtractVariable,
            ("w", false, true) => ToggleSoftWrap,
            ("m", false, true) => ExtractMethod,
            ("n", false, true) => InlineVariable,
            ("k", true, false) => Push,
            ("t", false, false) => Pull,
            ("f", false, false) => Find,
            ("r", false, false) => Replace,
            ("g", false, false) if m.control() => SelectAllOccurrences,
            ("g", false, false) => FindNext,
            ("g", true, false) => FindPrevious,
            ("f", true, false) => FindInPath,
            ("[", false, false) => Back,
            ("]", false, false) => Forward,
            ("[", false, true) => BlockStart,
            ("]", false, true) => BlockEnd,
            ("/", false, true) | ("÷", false, true) => BlockComment,
            ("u", true, false) => ToggleCase,
            ("t", false, true) => SurroundWith,
            ("a", true, false) => FindAction,
            ("=", false, false) | ("+", _, false) => ZoomIn,
            ("-", false, false) => ZoomOut,
            ("0", false, false) => ZoomReset,
            ("e", true, false) => RecentLocations,
            ("v", true, false) => PasteFromHistory,
            ("c", true, true) => CopyReference,
            ("u", false, false) => GotoSuperMethod,
            ("b", true, false) => GotoTypeDeclaration,
            ("t", true, false) => GotoTest,
            ("}", true, false) | ("]", true, false) => NextTab,
            ("{", true, false) | ("[", true, false) => PrevTab,
            _ => return None,
        },
        Key::Character(c) if m.control() && alt && c.eq_ignore_ascii_case("o") => OptimizeImports,
        Key::Character(c) if m.control() && alt && c.eq_ignore_ascii_case("c") => Composer,
        Key::Character(c) if m.control() && shift && !cmd && c.eq_ignore_ascii_case("r") => {
            RunTests
        }
        Key::Character(c) if m.control() && shift && !cmd && c.eq_ignore_ascii_case("d") => {
            DebugTest
        }
        Key::Character(c) if m.control() && !cmd && c.eq_ignore_ascii_case("h") => TypeHierarchy,
        Key::Character(c)
            if alt && shift && !cmd && !m.control() && c.eq_ignore_ascii_case("g") =>
        {
            AddCaretPerLine
        }
        Key::Character(c) if m.control() && shift && !cmd && c.eq_ignore_ascii_case("j") => {
            JoinLines
        }
        Key::Character(c)
            if m.control() && !shift && !cmd && !alt && c.eq_ignore_ascii_case("m") =>
        {
            MatchBrace
        }
        Key::Character(c) if m.control() && !shift && !cmd && c.eq_ignore_ascii_case("t") => {
            Transpose
        }
        Key::Character(c) if alt && !cmd && !m.control() && (c == "/" || c == "÷") => {
            WordCompletion
        }
        Key::Character(c) if m.control() && !cmd && !shift && c.eq_ignore_ascii_case("g") => {
            SelectNextOccurrence
        }
        Key::Named(Named::Enter) if alt && !cmd => QuickFix,
        Key::Named(Named::Enter) if cmd && shift => CompleteStatement,
        Key::Named(Named::Enter) if cmd && alt => StartNewLineBefore,
        Key::Named(Named::Enter) if cmd => SplitLine,
        Key::Named(Named::Enter) if shift && !m.control() => StartNewLine,
        Key::Named(Named::F1) if !alt && !shift => QuickDoc,
        Key::Character(c)
            if m.control() && !shift && !cmd && !alt && c.eq_ignore_ascii_case("j") =>
        {
            QuickDoc
        }
        Key::Named(Named::F2) if shift && !cmd => PreviousError,
        Key::Named(Named::F2) if !cmd && !alt => NextError,
        Key::Named(Named::Delete) if cmd && shift => Unwrap,
        Key::Named(Named::F6) if shift => Rename,
        Key::Named(Named::F6) if cmd => ChangeSignature,
        Key::Named(Named::F6) => MoveClass,
        Key::Named(Named::F12) if alt => ToggleTerminal,
        Key::Named(Named::F12) if cmd => FileStructure,
        Key::Named(Named::F12) if shift => FindUsages,
        Key::Named(Named::F12) => GotoDeclaration,
        Key::Named(Named::F8) if cmd => ToggleBreakpoint,
        Key::Named(Named::F8) if shift => StepOut,
        Key::Named(Named::F8) => StepOver,
        Key::Named(Named::F7) if !alt && !cmd && !shift => StepInto,
        Key::Named(Named::F9) => Resume,
        Key::Named(Named::F2) if cmd => StopDebug,
        Key::Named(Named::F7) if alt => FindUsages,
        Key::Named(Named::F3) if cmd => ShowBookmarks,
        Key::Named(Named::F3) if !shift && !alt => ToggleBookmark,
        Key::Named(Named::Backspace) if cmd => DeleteLine,
        Key::Named(Named::ArrowUp) if alt && shift => MoveLineUp,
        Key::Named(Named::ArrowDown) if alt && shift => MoveLineDown,
        Key::Named(Named::ArrowUp) if cmd && shift && !alt => MoveStatementUp,
        Key::Named(Named::ArrowDown) if cmd && shift && !alt => MoveStatementDown,
        Key::Named(Named::ArrowLeft) if cmd && shift && alt => MoveElementLeft,
        Key::Named(Named::ArrowRight) if cmd && shift && alt => MoveElementRight,
        Key::Named(Named::Delete) if alt && !cmd && !shift => DeleteWordEnd,
        Key::Named(Named::Backspace) if alt && !cmd && !shift => DeleteWordStart,
        Key::Named(Named::Backspace) if cmd && shift => JumpToLastEdit,
        Key::Named(Named::ArrowUp) if alt && !cmd && !shift => ExtendSelection,
        Key::Named(Named::ArrowDown) if alt && !cmd && !shift => ShrinkSelection,
        Key::Named(Named::ArrowUp) if cmd && alt => AddCaretAbove,
        Key::Named(Named::ArrowDown) if cmd && alt => AddCaretBelow,
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
        ToggleTerminal => "⌥F12",
        ToggleClaude => "⌥⌘A",
        RunTests => "⌃⇧R",
        Reformat => "⌥⌘L",
        Composer => "⌃⌥C",
        Generate => "⌘N",
        InsertTemplate => "⌘J",
        Commit => "⌘K",
        Push => "⇧⌘K",
        Pull => "⌘T",
        DebugTest => "⌃⇧D",
        ExtractVariable => "⌥⌘V",
        ToggleSoftWrap => "⌥⌘W",
        AddCaretAbove => "⌥⌘↑",
        AddCaretBelow => "⌥⌘↓",
        SelectNextOccurrence => "⌃G",
        ExtractMethod => "⌥⌘M",
        InlineVariable => "⌥⌘N",
        ChangeSignature => "⌘F6",
        MoveClass => "F6",
        ToggleBreakpoint => "⌘F8",
        Resume => "F9",
        StepOver => "F8",
        StepInto => "F7",
        StepOut => "⇧F8",
        StopDebug => "⌘F2",
        ExtendSelection => "⌥↑",
        ShrinkSelection => "⌥↓",
        JoinLines => "⌃⇧J",
        SplitLine => "⌘↵",
        StartNewLine => "⇧↵",
        StartNewLineBefore => "⌥⌘↵",
        CompleteStatement => "⇧⌘↵",
        ToggleCase => "⇧⌘U",
        Transpose => "⌃T",
        BlockComment => "⌥⌘/",
        MatchBrace => "⌃M",
        BlockStart => "⌥⌘[",
        BlockEnd => "⌥⌘]",
        WordCompletion => "⌥/",
        Unwrap => "⇧⌘⌦",
        SurroundWith => "⌥⌘T",
        SortLines | ReverseLines | UniqueLines => "",
        SelectAllOccurrences => "⌃⌘G",
        FindAction => "⇧⌘A",
        NextError => "F2",
        PreviousError => "⇧F2",
        QuickDoc => "F1",
        ToggleWhitespace => "",
        MoveStatementUp => "⇧⌘↑",
        MoveStatementDown => "⇧⌘↓",
        MoveElementLeft => "⌥⇧⌘←",
        MoveElementRight => "⌥⇧⌘→",
        DeleteWordEnd => "⌥⌦",
        DeleteWordStart => "⌥⌫",
        AddCaretPerLine => "⌥⇧G",
        ToggleBookmark => "F3",
        ShowBookmarks => "⌘F3",
        JumpToLastEdit => "⇧⌘⌫",
        RecentLocations => "⇧⌘E",
        PasteFromHistory => "⇧⌘V",
        CopyReference => "⌥⇧⌘C",
        GotoSuperMethod => "⌘U",
        GotoTypeDeclaration => "⇧⌘B",
        GotoTest => "⇧⌘T",
        ZoomIn => "⌘=",
        ZoomOut => "⌘−",
        ZoomReset => "⌘0",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_should_have_a_translated_name() {
        let untranslated: Vec<Cmd> = Cmd::ALL
            .into_iter()
            .filter(|c| c.label().starts_with("cmd-"))
            .collect();
        assert_eq!(untranslated, []);
    }
}
