//! Git in the IDE: the Commit panel (status, staging, diff, commit, push,
//! pull, discard), the branch picker, and change marks in the gutter.

use super::*;
use crate::git::{self, Branch, Change, LineChange, LineMark};
use crate::palette::{Item, Target};
use crate::tr;
use iced::Task;
use phpthunder::syntax::Pos;
use std::path::{Path, PathBuf};

/// Longest error text shown in a notice; git can be verbose.
const MAX_ERROR_LEN: usize = 300;

/// Palette entry that creates a branch named after the query.
fn create_branch_item(query: &str) -> Option<Item> {
    let name = query.trim();
    (!name.is_empty() && !name.contains(char::is_whitespace)).then(|| Item {
        badge: "+",
        title: name.to_string(),
        matched: Vec::new(),
        detail: tr!("git-create-branch"),
        target: Target::CreateBranch(name.to_string()),
    })
}

/// Gutter marks for a whole file that `HEAD` does not have.
fn all_added(text: &str) -> Vec<LineChange> {
    let count = text.lines().count().max(1);
    vec![LineChange {
        line: 0,
        count,
        mark: LineMark::Added,
    }]
}

impl App {
    /// Project root when it is a Git repository.
    fn git_root(&self) -> Option<PathBuf> {
        self.tree.root.clone().filter(|r| git::is_repository(r))
    }

    /// Re-reads the Git status in the background ([`Message::GitStatus`]).
    pub(crate) fn refresh_git(&self) -> Task<Message> {
        match self.git_root() {
            Some(root) => Task::perform(git::status(root), Message::GitStatus),
            None => Task::none(),
        }
    }

    /// Loads the `HEAD` version of `path` for its gutter marks.
    pub(crate) fn load_git_base(&self, path: PathBuf) -> Task<Message> {
        let Some(root) = self.git_root() else {
            return Task::none();
        };
        Task::perform(git::head_content(root, path.clone()), move |content| {
            Message::GitBase(path, content)
        })
    }

    /// Reloads the `HEAD` versions of every open file (after `HEAD` moved).
    fn reload_git_bases(&self) -> Task<Message> {
        Task::batch(self.docs.iter().map(|d| self.load_git_base(d.path.clone())))
    }

    /// Stores a new status; refreshes the selected diff and the gutter.
    pub(crate) fn git_status_arrived(
        &mut self,
        result: Result<git::RepoStatus, String>,
    ) -> Task<Message> {
        match result {
            Ok(status) => {
                self.branch = status.branch.clone().or_else(|| self.branch.clone());
                self.git = Some(status);
                let paths: Vec<PathBuf> = self.docs.iter().map(|d| d.path.clone()).collect();
                for path in paths {
                    self.refresh_git_marks(&path);
                }
                match self.commit.selected.clone() {
                    Some(path)
                        if self
                            .git
                            .as_ref()
                            .is_some_and(|g| g.change_of(&path).is_some()) =>
                    {
                        self.git_select(path)
                    }
                    Some(_) => {
                        self.commit.selected = None;
                        self.commit.diff.clear();
                        Task::none()
                    }
                    None => Task::none(),
                }
            }
            Err(error) => {
                self.notice = Some(tr!("git-failed", error = shorten(&error)));
                Task::none()
            }
        }
    }

    /// Stores the `HEAD` version of an open file and recomputes its marks.
    pub(crate) fn git_base_arrived(&mut self, path: PathBuf, content: Option<String>) {
        self.git_base.insert(path.clone(), content);
        self.refresh_git_marks(&path);
    }

    /// Recomputes the gutter marks of the open file at `path`.
    pub(crate) fn refresh_git_marks(&mut self, path: &Path) {
        let Some(doc) = self.docs.iter().find(|d| d.path == path) else {
            self.git_marks.remove(path);
            return;
        };
        let marks = match self.git_base.get(path) {
            Some(Some(base)) => git::line_changes(base, &doc.text),
            // Not in HEAD: all new when git lists it as a change.
            Some(None)
                if self
                    .git
                    .as_ref()
                    .is_some_and(|g| g.change_of(path).is_some()) =>
            {
                all_added(&doc.text)
            }
            _ => Vec::new(),
        };
        self.git_marks.insert(path.to_path_buf(), marks);
    }

    /// Opens the Commit panel and refreshes the status (`⌘K`).
    pub(crate) fn open_commit_panel(&mut self) -> Task<Message> {
        if self.git_root().is_none() {
            self.notice = Some(tr!("git-not-repository"));
            return Task::none();
        }
        self.show_panel(Bottom::Commit);
        self.refresh_git()
    }

    /// Starts `op` in the background; its result arrives as [`Message::GitDone`].
    fn run_git_op<F>(&mut self, op: GitOp, work: F) -> Task<Message>
    where
        F: std::future::Future<Output = Result<String, String>> + Send + 'static,
    {
        if self.commit.busy.is_some() {
            return Task::none();
        }
        self.commit.busy = Some(op.clone());
        Task::perform(work, move |result| Message::GitDone(op, result))
    }

    /// Stages `path` when it is not staged, else unstages it.
    pub(crate) fn git_toggle_stage(&mut self, path: PathBuf) -> Task<Message> {
        let Some(root) = self.git_root() else {
            return Task::none();
        };
        let staged = self
            .git
            .as_ref()
            .and_then(|g| g.files.iter().find(|f| f.path == path))
            .is_some_and(|f| f.staged && !f.unstaged);
        self.run_git_op(GitOp::Stage, async move {
            let files = vec![path];
            if staged {
                git::unstage(root, files).await
            } else {
                git::stage(root, files).await
            }
            .map(|()| String::new())
        })
    }

    /// Stages (or unstages) every changed file.
    pub(crate) fn git_stage_all(&mut self, stage: bool) -> Task<Message> {
        let (Some(root), Some(status)) = (self.git_root(), self.git.as_ref()) else {
            return Task::none();
        };
        let files: Vec<PathBuf> = status
            .files
            .iter()
            .filter(|f| if stage { f.unstaged } else { f.staged })
            .map(|f| f.path.clone())
            .collect();
        if files.is_empty() {
            return Task::none();
        }
        self.run_git_op(GitOp::Stage, async move {
            if stage {
                git::stage(root, files).await
            } else {
                git::unstage(root, files).await
            }
            .map(|()| String::new())
        })
    }

    /// Shows the diff of `path` against `HEAD` (the open tab's text when it
    /// is open, else the file on disk).
    pub(crate) fn git_select(&mut self, path: PathBuf) -> Task<Message> {
        let Some(root) = self.git_root() else {
            return Task::none();
        };
        self.commit.selected = Some(path.clone());
        self.commit.confirm_discard = None;
        let open = self
            .docs
            .iter()
            .find(|d| d.path == path)
            .map(|d| d.text.clone());
        Task::perform(
            async move {
                let base = git::head_content(root, path.clone())
                    .await
                    .unwrap_or_default();
                let current = match open {
                    Some(text) => text,
                    None => tokio::fs::read_to_string(&path).await.unwrap_or_default(),
                };
                (path, git::diff_lines(&base, &current))
            },
            |(path, diff)| Message::GitDiff(path, diff),
        )
    }

    /// Stores a diff if it is still for the selected file.
    pub(crate) fn git_diff_arrived(&mut self, path: PathBuf, diff: Vec<git::DiffLine>) {
        if self.commit.selected.as_ref() == Some(&path) {
            self.commit.diff = diff;
        }
    }

    /// Opens the selected file at a 1-based line of its diff.
    pub(crate) fn git_open_line(&mut self, line: usize) -> Task<Message> {
        let Some(path) = self.commit.selected.clone() else {
            return Task::none();
        };
        self.record_nav();
        let line = u32::try_from(line.saturating_sub(1)).unwrap_or(0);
        self.open(path, Some(Pos { line, col: 0 }))
    }

    /// Commits the staged files with the panel's message, then pushes when
    /// `and_push`.
    pub(crate) fn git_commit(&mut self, and_push: bool) -> Task<Message> {
        let Some(root) = self.git_root() else {
            return Task::none();
        };
        let message = self.commit.message.trim().to_string();
        if message.is_empty() {
            self.notice = Some(tr!("git-empty-message"));
            return Task::none();
        }
        if !self.git.as_ref().is_some_and(|g| g.has_staged()) {
            self.notice = Some(tr!("git-nothing-staged"));
            return Task::none();
        }
        self.save_all_php();
        self.run_git_op(GitOp::Commit(and_push), git::commit(root, message))
    }

    /// Discards a tracked file's changes on the second press; the first
    /// press only asks for confirmation. Untracked files are left alone.
    pub(crate) fn git_discard(&mut self, path: PathBuf) -> Task<Message> {
        let Some(root) = self.git_root() else {
            return Task::none();
        };
        if self.commit.confirm_discard.as_ref() != Some(&path) {
            self.commit.confirm_discard = Some(path);
            return Task::none();
        }
        self.commit.confirm_discard = None;
        let tracked = self
            .git
            .as_ref()
            .and_then(|g| g.change_of(&path))
            .is_some_and(|c| c != Change::Untracked);
        if !tracked {
            return Task::none();
        }
        self.run_git_op(GitOp::Discard, async move {
            git::discard(root, vec![path]).await.map(|()| String::new())
        })
    }

    /// Pulls the current branch (fast-forward only).
    pub(crate) fn git_pull(&mut self) -> Task<Message> {
        let Some(root) = self.git_root() else {
            return Task::none();
        };
        self.run_git_op(GitOp::Pull, async move {
            git::pull(root).await.map(|()| String::new())
        })
    }

    /// Pushes the current branch.
    pub(crate) fn git_push(&mut self) -> Task<Message> {
        let Some(root) = self.git_root() else {
            return Task::none();
        };
        self.run_git_op(GitOp::Push, async move {
            git::push(root).await.map(|()| String::new())
        })
    }

    /// Loads the branches for the picker ([`Message::GitBranches`]).
    pub(crate) fn git_show_branches(&mut self) -> Task<Message> {
        match self.git_root() {
            Some(root) => Task::perform(git::branches(root), Message::GitBranches),
            None => {
                self.notice = Some(tr!("git-not-repository"));
                Task::none()
            }
        }
    }

    /// Opens the branch picker; typing a new name offers to create it.
    pub(crate) fn git_branches_arrived(
        &mut self,
        result: Result<Vec<Branch>, String>,
    ) -> Task<Message> {
        let branches = match result {
            Ok(b) => b,
            Err(error) => {
                self.notice = Some(tr!("git-failed", error = shorten(&error)));
                return Task::none();
            }
        };
        let items = branches
            .into_iter()
            .map(|b| Item {
                badge: if b.current { "●" } else { "⎇" },
                title: b.name.clone(),
                matched: Vec::new(),
                detail: if b.current {
                    tr!("git-branch-current")
                } else if b.remote {
                    tr!("git-branch-remote")
                } else {
                    String::new()
                },
                target: Target::Checkout(b),
            })
            .collect();
        let task = self.open_choice(tr!("git-branches-title"), items);
        if let Some(p) = &mut self.palette {
            p.extra = Some(create_branch_item);
        }
        task
    }

    /// Checks out `branch` (no-op for the current one).
    pub(crate) fn git_checkout(&mut self, branch: Branch) -> Task<Message> {
        let Some(root) = self.git_root().filter(|_| !branch.current) else {
            return Task::none();
        };
        self.run_git_op(GitOp::Checkout, async move {
            git::checkout(root, branch).await.map(|()| String::new())
        })
    }

    /// Creates and checks out branch `name`.
    pub(crate) fn git_create_branch(&mut self, name: String) -> Task<Message> {
        let Some(root) = self.git_root() else {
            return Task::none();
        };
        self.run_git_op(GitOp::CreateBranch, async move {
            git::create_branch(root, name).await.map(|()| String::new())
        })
    }

    /// Handles a finished operation: reports it, refreshes the status, and
    /// reloads unmodified tabs and gutter bases when files or `HEAD` moved.
    pub(crate) fn git_done(&mut self, op: GitOp, result: Result<String, String>) -> Task<Message> {
        self.commit.busy = None;
        self.commit.confirm_discard = None;
        if let Err(error) = result {
            self.notice = Some(tr!("git-failed", error = shorten(&error)));
            return self.refresh_git();
        }
        let summary = result.unwrap_or_default();
        let mut tasks = vec![self.refresh_git()];
        match &op {
            GitOp::Stage => {}
            GitOp::Commit(and_push) => {
                self.commit.message.clear();
                self.notice = Some(tr!("git-committed", summary = summary));
                if *and_push {
                    tasks.push(self.git_push());
                }
            }
            GitOp::Push => self.notice = Some(tr!("git-pushed")),
            GitOp::Pull => self.notice = Some(tr!("git-pulled")),
            GitOp::Discard => self.notice = Some(tr!("git-discarded")),
            GitOp::Checkout | GitOp::CreateBranch => self.notice = Some(tr!("git-switched")),
        }
        if op.changes_files() {
            self.reload_unmodified_docs();
        }
        if op != GitOp::Stage {
            tasks.push(self.reload_git_bases());
        }
        Task::batch(tasks)
    }

    /// Re-reads open tabs without unsaved edits from disk, after git
    /// changed the files (pull, checkout, discard).
    fn reload_unmodified_docs(&mut self) {
        let mut changed = Vec::new();
        for doc in self.docs.iter_mut().filter(|d| !d.dirty) {
            let Ok(text) = std::fs::read_to_string(&doc.path) else {
                continue;
            };
            if text != doc.text {
                let cursor = doc.cursor();
                doc.set_text(text, cursor);
                doc.mark_saved();
                changed.push((doc.path.clone(), doc.text.clone(), doc.is_php()));
            }
        }
        for (path, text, is_php) in changed {
            if is_php {
                self.lang.update_file(&path, &text);
            }
        }
        self.inspect_pending = true;
    }
}

/// `error` cut to [`MAX_ERROR_LEN`] characters for a notice.
fn shorten(error: &str) -> String {
    if error.chars().count() > MAX_ERROR_LEN {
        format!("{}…", error.chars().take(MAX_ERROR_LEN).collect::<String>())
    } else {
        error.to_string()
    }
}
