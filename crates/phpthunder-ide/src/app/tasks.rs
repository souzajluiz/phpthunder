//! Tasks in the IDE: the task picker (typed-in tasks and GitHub issues),
//! the active task's timer, a branch and commit message per task, and the
//! time report.

use super::*;
use crate::palette::{Item, Target};
use crate::tasks::{self, TaskList};
use crate::tr;
use iced::Task;

/// Palette entry that creates a task titled after the query.
fn new_task_item(query: &str) -> Option<Item> {
    let title = query.trim();
    (!title.is_empty()).then(|| Item {
        badge: "+",
        title: title.to_string(),
        matched: Vec::new(),
        detail: tr!("task-new"),
        target: Target::NewTask(title.to_string()),
    })
}

impl App {
    /// Opens the task picker: open tasks with their time, the active one
    /// first; typing a new title offers to create it. GitHub issues join
    /// the list when `gh` returns them.
    pub(crate) fn open_tasks(&mut self) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let list = TaskList::load(&root);
        let now = tasks::now();
        let active = list.active.as_ref().map(|(id, _)| id.clone());
        let mut open: Vec<&tasks::WorkTask> = list.tasks.iter().filter(|t| !t.done).collect();
        open.sort_by_key(|t| Some(&t.id) != active.as_ref());
        let items = open
            .into_iter()
            .map(|t| Item {
                badge: if Some(&t.id) == active.as_ref() {
                    "▶"
                } else {
                    "○"
                },
                title: t.title.clone(),
                matched: Vec::new(),
                detail: format!("{}  ·  {}", t.id, tasks::duration(list.spent(t, now))),
                target: Target::ActivateTask(t.id.clone()),
            })
            .collect();
        let shown = self.open_choice(tr!("tasks-title"), items);
        if let Some(p) = &mut self.palette {
            p.extra = Some(new_task_item);
        }
        Task::batch([
            shown,
            Task::perform(tasks::github_issues(root), Message::IssuesLoaded),
        ])
    }

    /// Adds GitHub issues not yet tracked to the open task picker.
    pub(crate) fn issues_loaded(&mut self, result: Result<Vec<(u64, String, String)>, String>) {
        let Ok(issues) = result else {
            // No `gh`, no login or no GitHub remote: typed tasks only.
            return;
        };
        let Some(root) = self.tree.root.clone() else {
            return;
        };
        let known = TaskList::load(&root);
        let title = tr!("tasks-title");
        let Some(p) = self
            .palette
            .as_mut()
            .filter(|p| p.fixed.as_ref().is_some_and(|(t, _)| *t == title))
        else {
            return;
        };
        let Some((_, fixed)) = p.fixed.as_mut() else {
            return;
        };
        fixed.extend(
            issues
                .into_iter()
                .filter(|(n, _, _)| !known.tasks.iter().any(|t| t.id == format!("#{n}")))
                .map(|(number, title, url)| Item {
                    badge: "◇",
                    title: title.clone(),
                    matched: Vec::new(),
                    detail: format!("#{number}  ·  GitHub"),
                    target: Target::ActivateIssue(number, title, url),
                }),
        );
        p.items = crate::app::command::filter_fixed(fixed, &p.query);
    }

    /// Makes a task active (creating it first for a new title or issue),
    /// stopping the previous one's timer.
    pub(crate) fn activate_task(&mut self, target: Target) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let mut list = TaskList::load(&root);
        let id = match target {
            Target::ActivateTask(id) => id,
            Target::NewTask(title) => list.add_local(&title),
            Target::ActivateIssue(number, title, url) => list.add_issue(number, &title, &url),
            _ => return Task::none(),
        };
        list.activate(&id, tasks::now());
        self.notice = Some(match (list.save(&root), list.active_task()) {
            (Err(error), _) => tr!("task-save-failed", error = error.to_string()),
            (Ok(()), Some(task)) => tr!("task-active", title = task.title.as_str()),
            (Ok(()), None) => return Task::none(),
        });
        Task::none()
    }

    /// Closes the active task, adding its time.
    pub(crate) fn close_active_task(&mut self) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let mut list = TaskList::load(&root);
        let Some(title) = list.close_active(tasks::now()) else {
            self.notice = Some(tr!("task-none-active"));
            return Task::none();
        };
        self.notice = Some(match list.save(&root) {
            Ok(()) => tr!("task-closed", title = title),
            Err(error) => tr!("task-save-failed", error = error.to_string()),
        });
        Task::none()
    }

    /// Creates and checks out the active task's branch.
    pub(crate) fn branch_for_active_task(&mut self) -> Task<Message> {
        let branch = self
            .tree
            .root
            .as_deref()
            .map(TaskList::load)
            .and_then(|l| l.active_task().map(tasks::WorkTask::branch_name));
        match branch {
            Some(branch) => self.git_create_branch(branch),
            None => {
                self.notice = Some(tr!("task-none-active"));
                Task::none()
            }
        }
    }

    /// The active task's commit message, for an empty Commit panel.
    pub(crate) fn active_task_message(&self) -> Option<String> {
        let list = TaskList::load(self.tree.root.as_deref()?);
        list.active_task().map(tasks::WorkTask::commit_message)
    }

    /// Opens a Markdown report of the time spent per task.
    pub(crate) fn task_report(&mut self) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let list = TaskList::load(&root);
        let text = list.report(
            tasks::now(),
            &tr!("task-report-title"),
            [
                &tr!("task-report-task"),
                &tr!("task-report-time"),
                &tr!("task-report-done"),
            ],
        );
        let path = std::env::temp_dir().join("phpthunder-time-report.md");
        match std::fs::write(&path, text) {
            Ok(()) => self.open(path, None),
            Err(error) => {
                self.fail(error.to_string());
                Task::none()
            }
        }
    }
}
