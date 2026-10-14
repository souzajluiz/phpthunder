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
    /// Open a UML class diagram of the class at the caret or of the
    /// active file's folder (`⌥⇧⌘U`).
    ShowDiagram,
    /// Open the same class diagram as PlantUML text.
    ShowDiagramPlantUml,
    /// Write or refresh the copyright header of the active file.
    UpdateCopyright,
    /// Write or refresh the copyright header of every project file.
    UpdateCopyrightEverywhere,
    /// Pick one of the project's log files to open.
    OpenLog,
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
    /// Stash local changes (untracked included).
    GitStash,
    /// Restore or delete a stash.
    GitStashes,
    /// Rebase the current branch onto another.
    GitRebase,
    /// Merge another branch into the current one.
    GitMerge,
    /// Continue the rebase or merge in progress.
    GitContinue,
    /// Abort the rebase or merge in progress.
    GitAbort,
    /// Open the file at the caret line on GitHub, GitLab or Bitbucket.
    OpenOnRemote,
    /// Open a new pull request from the current branch.
    CreatePullRequest,
    /// Pick a run configuration to start in the terminal (`⌃⌥R`).
    RunConfiguration,
    /// Debug the active PHP file as a script.
    DebugCurrentFile,
    /// Start or stop listening for Xdebug connections from web requests.
    DebugListen,
    /// Toggle stopping at the first line of debugged scripts.
    DebugBreakAtFirstLine,
    /// Report how Xdebug is set up for the project's PHP.
    ValidateDebugger,
    /// Profile the active PHP file with Xdebug and open the report.
    ProfileCurrentFile,
    /// Open the project settings file (interpreter, run configurations).
    EditProjectSettings,
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
    /// Diff the selection or file against the clipboard.
    CompareWithClipboard,
    /// Earlier saved versions of the file.
    LocalHistory,
    /// Show who last changed each line (git blame).
    Annotate,
    /// Turn the selected expression into a class constant (`⌥⌘C`).
    IntroduceConstant,
    /// Turn the selected expression into a property (`⌥⌘F`).
    IntroduceField,
    /// Turn the selected expression into a parameter (`⌥⌘P`).
    IntroduceParameter,
    /// Show the declaration's source in a popup (`⌥Space`).
    QuickDefinition,
    /// Hide everything but the editor.
    ToggleDistractionFree,
    /// Show the About dialog (version, commit, author, repository).
    About,
    /// Implement the parent's abstract methods (`⌃I`).
    ImplementMethods,
    /// Override parent methods (`⌃O`).
    OverrideMethods,
    /// Delete the member at the caret if nothing uses it (`⌘⌦`).
    SafeDelete,
    /// Go to the next function or method (`⌃⇧↓`).
    NextMethod,
    /// Go to the previous function or method (`⌃⇧↑`).
    PreviousMethod,
    /// Go to the next change since the last commit (`⌃⌥⇧↓`).
    NextChange,
    /// Go to the previous change since the last commit (`⌃⌥⇧↑`).
    PreviousChange,
    /// Show the problem at the caret (`⌘F1`).
    ErrorDescription,
    /// Open the PHP manual for the word at the caret (`⇧F1`).
    ExternalDoc,
    /// Copy the file's absolute path (`⇧⌘C`).
    CopyPath,
    /// Show the file in the system file manager (`⌥⌘F12`).
    RevealInFileManager,
    /// Select the file in the project tree (`⌥F1`).
    SelectInTree,
    /// Show or hide the Find panel (`⌘3`).
    ToggleFindPanel,
    /// Show or hide the Debug panel (`⌘5`).
    ToggleDebugPanel,
    /// Show or hide the Tests panel (`⌥⌘8`).
    ToggleTestsPanel,
    /// Hide the open tool panel (`⇧⎋`).
    HideToolWindow,
    /// Hide every tool panel and the tree, or bring them back (`⇧⌘F12`).
    HideAllToolWindows,
    /// Scroll the caret line to the middle (`⌃L`).
    ScrollToCenter,
    /// Drop the occurrence added last (`⌃⇧G`).
    UnselectOccurrence,
    /// Evaluate the selection in the paused script (`⌥F8`).
    EvaluateExpression,
    /// List the line breakpoints (`⇧⌘F8`).
    ViewBreakpoints,
    /// Open the settings file (`⌘,`).
    Settings,
    /// Reload open files from disk (`⌥⌘Y`).
    ReloadFromDisk,
    /// Enter or leave full screen (`⌃⌘F`).
    ToggleFullScreen,
    /// Minimize the window (`⌘M`).
    Minimize,
    /// Pick a branch to check out or create (⇧⌘ plus the backtick key).
    GitBranches,
    /// Revert the changed lines at the caret to the last commit (`⌥⌘Z`).
    RollbackChangedLines,
    /// Open a new scratch file outside the project (`⇧⌘N`).
    NewScratchFile,
    /// Save the file under another name (`⇧⌘S`).
    SaveAs,
    /// Run the paused script up to the caret line (`⌥F9`).
    RunToCursor,
    /// Go to where the paused script is (`⌥F10`).
    ShowExecutionPoint,
    /// Grow the panel next to the editor leftward (`⌃⌥←`).
    StretchLeft,
    /// Grow the panel next to the editor rightward (`⌃⌥→`).
    StretchRight,
    /// Grow the bottom panel upward (`⌃⌥↑`).
    StretchUp,
    /// Shrink the bottom panel downward (`⌃⌥↓`).
    StretchDown,
    /// Maximize the bottom panel, or restore it (`⇧⌘'`).
    MaximizeToolWindow,
    /// Put a digit or letter bookmark on the caret line (`⌥F3`).
    ToggleBookmarkWithMnemonic,
    /// Show the inferred type at the caret (`⌃⇧P`).
    ExpressionTypeInfo,
    /// Keep the usages of the word at the caret highlighted (`⇧⌘F7`).
    HighlightUsagesInFile,
    /// List the callers of the function at the caret (`⌃⌥H`).
    CallHierarchy,
    /// Show which classes define the method at the caret (`⇧⌘H`).
    MethodHierarchy,
    /// Go to a related file: the test, its subject, the parent class (`⌃⌘↑`).
    GotoRelated,
    /// Switch column (block) selection mode (`⇧⌘8`).
    ColumnSelection,
}

impl Cmd {
    /// Every command, in the order the Find Action palette lists them.
    pub const ALL: [Cmd; 178] = {
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
            ShowDiagram,
            ShowDiagramPlantUml,
            UpdateCopyright,
            UpdateCopyrightEverywhere,
            OpenLog,
            FileStructure,
            Problems,
            ToggleTerminal,
            ToggleClaude,
            RunTests,
            Reformat,
            Composer,
            GitStash,
            GitStashes,
            GitRebase,
            GitMerge,
            GitContinue,
            GitAbort,
            OpenOnRemote,
            CreatePullRequest,
            RunConfiguration,
            EditProjectSettings,
            DebugCurrentFile,
            DebugListen,
            DebugBreakAtFirstLine,
            ValidateDebugger,
            ProfileCurrentFile,
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
            CompareWithClipboard,
            LocalHistory,
            Annotate,
            IntroduceConstant,
            IntroduceField,
            IntroduceParameter,
            QuickDefinition,
            ToggleDistractionFree,
            About,
            ImplementMethods,
            OverrideMethods,
            SafeDelete,
            NextMethod,
            PreviousMethod,
            NextChange,
            PreviousChange,
            ErrorDescription,
            ExternalDoc,
            CopyPath,
            RevealInFileManager,
            SelectInTree,
            ToggleFindPanel,
            ToggleDebugPanel,
            ToggleTestsPanel,
            HideToolWindow,
            HideAllToolWindows,
            ScrollToCenter,
            UnselectOccurrence,
            EvaluateExpression,
            ViewBreakpoints,
            Settings,
            ReloadFromDisk,
            ToggleFullScreen,
            Minimize,
            GitBranches,
            RollbackChangedLines,
            NewScratchFile,
            SaveAs,
            RunToCursor,
            ShowExecutionPoint,
            StretchLeft,
            StretchRight,
            StretchUp,
            StretchDown,
            MaximizeToolWindow,
            ToggleBookmarkWithMnemonic,
            ExpressionTypeInfo,
            HighlightUsagesInFile,
            CallHierarchy,
            MethodHierarchy,
            GotoRelated,
            ColumnSelection,
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
            ("f", false, false) if m.control() => ToggleFullScreen,
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
            ("u", true, true) => ShowDiagram,
            ("t", false, true) => SurroundWith,
            ("a", true, false) => FindAction,
            ("c", false, true) => IntroduceConstant,
            ("f", false, true) => IntroduceField,
            ("p", false, true) => IntroduceParameter,
            ("=", false, false) | ("+", _, false) => ZoomIn,
            ("-", false, false) => ZoomOut,
            ("0", false, false) => Commit,
            ("2", false, false) => ShowBookmarks,
            ("3", false, false) => ToggleFindPanel,
            ("5", false, false) => ToggleDebugPanel,
            ("7", false, false) => FileStructure,
            ("8", false, true) => ToggleTestsPanel,
            (",", false, false) => Settings,
            (";", false, false) => EditProjectSettings,
            ("m", false, false) => Minimize,
            ("c", true, false) => CopyPath,
            ("r", true, false) => FindInPath,
            ("y", false, true) => ReloadFromDisk,
            ("l", true, true) => Reformat,
            ("j", false, true) => SurroundWith,
            ("i", true, true) => Problems,
            ("e", true, false) => RecentLocations,
            ("v", true, false) => PasteFromHistory,
            ("c", true, true) => CopyReference,
            ("u", false, false) => GotoSuperMethod,
            ("b", true, false) => GotoTypeDeclaration,
            ("t", true, false) => GotoTest,
            ("`", true, false) | ("~", true, false) => GitBranches,
            ("z", false, true) => RollbackChangedLines,
            ("n", true, false) => NewScratchFile,
            ("s", true, false) => SaveAs,
            ("'", true, false) | ("\"", true, false) => MaximizeToolWindow,
            ("h", true, false) => MethodHierarchy,
            ("8", true, false) | ("*", true, false) => ColumnSelection,
            ("}", true, false) | ("]", true, false) => NextTab,
            ("{", true, false) | ("[", true, false) => PrevTab,
            _ => return None,
        },
        Key::Named(Named::Escape) if shift && !cmd && !alt => HideToolWindow,
        Key::Named(Named::F12) if cmd && shift => HideAllToolWindows,
        Key::Named(Named::F12) if cmd && alt => RevealInFileManager,
        Key::Named(Named::F1) if cmd => ErrorDescription,
        Key::Named(Named::F1) if shift => ExternalDoc,
        Key::Named(Named::F1) if alt => SelectInTree,
        Key::Named(Named::F8) if alt && !cmd && !shift => EvaluateExpression,
        Key::Named(Named::F8) if cmd && shift => ViewBreakpoints,
        Key::Named(Named::F7) if cmd && shift && !alt => HighlightUsagesInFile,
        Key::Named(Named::F7) if shift && !cmd && !alt => StepInto,
        Key::Named(Named::F9) if alt && !cmd && !shift => RunToCursor,
        Key::Named(Named::F10) if alt && !cmd && !shift => ShowExecutionPoint,
        Key::Named(Named::F3) if alt && !cmd && !shift => ToggleBookmarkWithMnemonic,
        Key::Named(Named::ArrowUp) if cmd && m.control() && !shift && !alt => GotoRelated,
        Key::Named(Named::ArrowLeft) if m.control() && alt && !cmd && !shift => StretchLeft,
        Key::Named(Named::ArrowRight) if m.control() && alt && !cmd && !shift => StretchRight,
        Key::Named(Named::ArrowUp) if m.control() && alt && !cmd && !shift => StretchUp,
        Key::Named(Named::ArrowDown) if m.control() && alt && !cmd && !shift => StretchDown,
        Key::Named(Named::F4) if m.control() && shift => CloseTab,
        Key::Named(Named::F4) if !cmd && !alt && !shift && !m.control() => GotoDeclaration,
        Key::Named(Named::ArrowDown) if m.control() && alt && shift => NextChange,
        Key::Named(Named::ArrowUp) if m.control() && alt && shift => PreviousChange,
        Key::Named(Named::ArrowDown) if m.control() && shift && !alt && !cmd => NextMethod,
        Key::Named(Named::ArrowUp) if m.control() && shift && !alt && !cmd => PreviousMethod,
        Key::Named(Named::ArrowDown) if cmd && !shift && !alt => GotoDeclaration,
        Key::Named(Named::ArrowRight) if m.control() && !shift && !alt && !cmd => NextTab,
        Key::Named(Named::ArrowLeft) if m.control() && !shift && !alt && !cmd => PrevTab,
        Key::Named(Named::Delete) if cmd && !shift && !alt => SafeDelete,
        Key::Named(Named::Tab) if m.control() && !cmd && !alt => RecentFiles,
        Key::Character(c) if m.control() && alt && !cmd && !shift => {
            match c.to_ascii_lowercase().as_str() {
                "i" => Reformat,
                "h" => CallHierarchy,
                "=" | "+" => ZoomIn,
                "-" => ZoomOut,
                "0" => ZoomReset,
                "o" => OptimizeImports,
                "c" => Composer,
                "r" => RunConfiguration,
                " " => Complete,
                _ => return None,
            }
        }
        Key::Character(c) if m.control() && !alt && !cmd && !shift => {
            match c.to_ascii_lowercase().as_str() {
                "i" => ImplementMethods,
                "o" => OverrideMethods,
                "l" => ScrollToCenter,
                "r" => RunConfiguration,
                "d" => DebugCurrentFile,
                _ => return control_letter(c),
            }
        }
        Key::Character(c)
            if m.control() && shift && !alt && !cmd && c.eq_ignore_ascii_case("g") =>
        {
            UnselectOccurrence
        }
        Key::Character(c)
            if m.control() && shift && !alt && !cmd && c.eq_ignore_ascii_case("p") =>
        {
            ExpressionTypeInfo
        }
        Key::Character(c) if m.control() && alt && c.eq_ignore_ascii_case("o") => OptimizeImports,
        Key::Character(c) if m.control() && alt && c.eq_ignore_ascii_case("c") => Composer,
        Key::Character(c) if m.control() && alt && !cmd && c.eq_ignore_ascii_case("r") => {
            RunConfiguration
        }
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
        Key::Named(Named::Space) if alt && !cmd && !m.control() => QuickDefinition,
        Key::Character(c) if alt && !cmd && !m.control() && (c == " " || c == "\u{a0}") => {
            QuickDefinition
        }
        Key::Named(Named::Space) if m.control() => Complete,
        Key::Character(" ") if m.control() => Complete,
        _ => return None,
    })
}

/// The existing `⌃`-letter bindings (`⌃H`, `⌃M`, `⌃T`, `⌃G`, `⌃J`, `⌃Space`) for
/// `command`'s plain-control arm, which handles the new letters first.
fn control_letter(c: &str) -> Option<Cmd> {
    Some(match c.to_ascii_lowercase().as_str() {
        "h" => Cmd::TypeHierarchy,
        "m" => Cmd::MatchBrace,
        "t" => Cmd::Transpose,
        "g" => Cmd::SelectNextOccurrence,
        "j" => Cmd::QuickDoc,
        " " => Cmd::Complete,
        _ => return None,
    })
}

/// Numbered bookmark shortcut: `⌃⇧0`–`⌃⇧9` sets (`true`), `⌃0`–`⌃9`
/// jumps (`false`). Checked by the editor and window key handling before
/// `command`. Shift may turn the digit into its US symbol, so both match.
pub fn numbered_bookmark(key: &Key, m: Modifiers) -> Option<(u8, bool)> {
    if !m.control() || m.alt() || m.command() {
        return None;
    }
    let Key::Character(c) = key.as_ref() else {
        return None;
    };
    let digit = "0123456789".find(c).or_else(|| ")!@#$%^&*(".find(c))?;
    Some((digit as u8, m.shift()))
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
        ShowDiagram => "⌥⇧⌘U",
        ShowDiagramPlantUml | UpdateCopyright | UpdateCopyrightEverywhere | OpenLog => "",
        FileStructure => "⌘F12",
        Problems => "⌘6",
        ToggleTerminal => "⌥F12",
        ToggleClaude => "⌥⌘A",
        RunTests => "⌃⇧R",
        Reformat => "⌥⌘L",
        Composer => "⌃⌥C",
        RunConfiguration => "⌃⌥R",
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
        ToggleWhitespace
        | GitStash
        | GitStashes
        | GitRebase
        | GitMerge
        | GitContinue
        | GitAbort
        | OpenOnRemote
        | CreatePullRequest
        | EditProjectSettings
        | DebugCurrentFile
        | DebugListen
        | DebugBreakAtFirstLine
        | ValidateDebugger
        | ProfileCurrentFile => "",
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
        ZoomReset => "⌃⌥0",
        CompareWithClipboard | LocalHistory | Annotate => "",
        IntroduceConstant => "⌥⌘C",
        IntroduceField => "⌥⌘F",
        IntroduceParameter => "⌥⌘P",
        QuickDefinition => "⌥Space",
        ToggleDistractionFree | About => "",
        ImplementMethods => "⌃I",
        OverrideMethods => "⌃O",
        SafeDelete => "⌘⌦",
        NextMethod => "⌃⇧↓",
        PreviousMethod => "⌃⇧↑",
        NextChange => "⌃⌥⇧↓",
        PreviousChange => "⌃⌥⇧↑",
        ErrorDescription => "⌘F1",
        ExternalDoc => "⇧F1",
        CopyPath => "⇧⌘C",
        RevealInFileManager => "⌥⌘F12",
        SelectInTree => "⌥F1",
        ToggleFindPanel => "⌘3",
        ToggleDebugPanel => "⌘5",
        ToggleTestsPanel => "⌥⌘8",
        HideToolWindow => "⇧⎋",
        HideAllToolWindows => "⇧⌘F12",
        ScrollToCenter => "⌃L",
        UnselectOccurrence => "⌃⇧G",
        EvaluateExpression => "⌥F8",
        ViewBreakpoints => "⇧⌘F8",
        Settings => "⌘,",
        ReloadFromDisk => "⌥⌘Y",
        ToggleFullScreen => "⌃⌘F",
        Minimize => "⌘M",
        GitBranches => "⇧⌘`",
        RollbackChangedLines => "⌥⌘Z",
        NewScratchFile => "⇧⌘N",
        SaveAs => "⇧⌘S",
        RunToCursor => "⌥F9",
        ShowExecutionPoint => "⌥F10",
        StretchLeft => "⌃⌥←",
        StretchRight => "⌃⌥→",
        StretchUp => "⌃⌥↑",
        StretchDown => "⌃⌥↓",
        MaximizeToolWindow => "⇧⌘'",
        ToggleBookmarkWithMnemonic => "⌥F3",
        ExpressionTypeInfo => "⌃⇧P",
        HighlightUsagesInFile => "⇧⌘F7",
        CallHierarchy => "⌃⌥H",
        MethodHierarchy => "⇧⌘H",
        GotoRelated => "⌃⌘↑",
        ColumnSelection => "⇧⌘8",
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

    /// `command` for a character key with `mods`.
    fn on(c: &str, mods: Modifiers) -> Option<Cmd> {
        command(&Key::Character(c.into()), mods)
    }

    #[test]
    fn control_space_should_still_complete() {
        assert_eq!(on(" ", Modifiers::CTRL), Some(Cmd::Complete));
    }

    #[test]
    fn control_alt_space_should_complete() {
        assert_eq!(
            on(" ", Modifiers::CTRL | Modifiers::ALT),
            Some(Cmd::Complete)
        );
    }

    #[test]
    fn control_h_should_still_open_the_type_hierarchy() {
        assert_eq!(on("h", Modifiers::CTRL), Some(Cmd::TypeHierarchy));
    }

    #[test]
    fn control_i_should_implement_methods() {
        assert_eq!(on("i", Modifiers::CTRL), Some(Cmd::ImplementMethods));
    }

    #[test]
    fn command_zero_should_open_the_commit_panel() {
        assert_eq!(on("0", Modifiers::COMMAND), Some(Cmd::Commit));
    }

    #[test]
    fn control_alt_zero_should_reset_zoom() {
        assert_eq!(
            on("0", Modifiers::CTRL | Modifiers::ALT),
            Some(Cmd::ZoomReset)
        );
    }

    #[test]
    fn shifted_digit_symbol_should_set_a_numbered_bookmark() {
        let key = Key::Character("!".into());
        assert_eq!(
            numbered_bookmark(&key, Modifiers::CTRL | Modifiers::SHIFT),
            Some((1, true))
        );
    }

    #[test]
    fn control_digit_should_jump_to_a_numbered_bookmark() {
        let key = Key::Character("7".into());
        assert_eq!(numbered_bookmark(&key, Modifiers::CTRL), Some((7, false)));
    }
}
