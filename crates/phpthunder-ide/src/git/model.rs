//! Repository status, file changes, diff lines and branches.

use std::path::PathBuf;

/// How a file differs from `HEAD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Tracked and changed.
    Modified,
    /// New in the index.
    Added,
    /// Deleted from the working tree or the index.
    Deleted,
    /// Renamed or copied.
    Renamed,
    /// Not tracked by git.
    Untracked,
    /// Has merge conflicts.
    Conflicted,
}

impl Change {
    /// One-letter marker shown in the Commit panel.
    pub fn letter(self) -> &'static str {
        match self {
            Change::Modified => "M",
            Change::Added => "A",
            Change::Deleted => "D",
            Change::Renamed => "R",
            Change::Untracked => "U",
            Change::Conflicted => "!",
        }
    }
}

/// One changed file of the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Absolute path.
    pub path: PathBuf,
    /// Path relative to the repository root, as git prints it.
    pub rel: String,
    /// Kind of change.
    pub change: Change,
    /// Whether the index holds changes for the file (it will be committed).
    pub staged: bool,
    /// Whether the working tree has changes not in the index.
    pub unstaged: bool,
}

/// Working tree status of the repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoStatus {
    /// Checked-out branch; `None` on a detached `HEAD`.
    pub branch: Option<String>,
    /// Upstream branch, when one is set.
    pub upstream: Option<String>,
    /// Commits not yet pushed.
    pub ahead: u32,
    /// Commits not yet pulled.
    pub behind: u32,
    /// Changed files, sorted by path.
    pub files: Vec<FileChange>,
}

impl RepoStatus {
    /// The change of `path`, if it has one.
    pub fn change_of(&self, path: &std::path::Path) -> Option<Change> {
        self.files.iter().find(|f| f.path == path).map(|f| f.change)
    }

    /// Whether any file is staged for commit.
    pub fn has_staged(&self) -> bool {
        self.files.iter().any(|f| f.staged)
    }
}

/// Kind of a changed line range in the editor gutter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineMark {
    /// Lines not in `HEAD`.
    Added,
    /// Lines that replace lines of `HEAD`.
    Modified,
    /// Lines of `HEAD` removed just above this line.
    Deleted,
}

/// A changed range of the current text compared with `HEAD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineChange {
    /// First line (0-based) of the range in the current text.
    pub line: usize,
    /// Number of lines; 0 for deletions.
    pub count: usize,
    /// Kind of change.
    pub mark: LineMark,
}

/// Kind of a line in a diff view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    /// Unchanged context line.
    Context,
    /// Line only in the new text.
    Added,
    /// Line only in the old text.
    Removed,
    /// `@@` header starting a hunk.
    Hunk,
}

/// One line of a diff view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Kind of line.
    pub kind: DiffKind,
    /// Text without its newline.
    pub text: String,
    /// 1-based line number in the new text (added and context lines).
    pub new_line: Option<usize>,
}

/// A local or remote branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// Short name (`main`, `origin/main`).
    pub name: String,
    /// Whether it is checked out.
    pub current: bool,
    /// Whether it is a remote-tracking branch.
    pub remote: bool,
}
