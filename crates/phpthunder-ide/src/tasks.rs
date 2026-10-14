//! Tasks and time tracking: the work items of a project (typed in, or
//! GitHub issues through the `gh` CLI), the active one, and the time spent
//! on each. Kept per project in `~/.config/phpthunder/tasks/`, outside the
//! repository.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkTask {
    /// Stable id: `#<number>` for issues, `local-<n>` otherwise.
    pub id: String,
    /// What the task is.
    pub title: String,
    /// Issue page, for issues.
    #[serde(default)]
    pub url: Option<String>,
    /// Seconds spent while it was active, not counting the running stretch.
    #[serde(default)]
    pub seconds: u64,
    /// Closed.
    #[serde(default)]
    pub done: bool,
}

impl WorkTask {
    /// The start of a commit message for this task: `#12 Fix login` for
    /// issues, the title otherwise.
    pub fn commit_message(&self) -> String {
        if self.id.starts_with('#') {
            format!("{} {}", self.id, self.title)
        } else {
            self.title.clone()
        }
    }

    /// A branch name for the task: `task/12-fix-login`.
    pub fn branch_name(&self) -> String {
        let slug: String = self
            .title
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|p| !p.is_empty())
            .take(6)
            .collect::<Vec<_>>()
            .join("-");
        let number = self.id.trim_start_matches('#').trim_start_matches("local-");
        format!("task/{number}-{slug}")
    }
}

/// The tasks of one project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskList {
    /// Every task, oldest first.
    pub tasks: Vec<WorkTask>,
    /// The active task's id and when it became active (Unix seconds).
    pub active: Option<(String, u64)>,
}

/// Now, in Unix seconds.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Where the task list of the project at `root` is kept.
fn store_path(root: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let key = phpthunder::index::content_hash(&root.to_string_lossy());
    Some(PathBuf::from(home).join(format!(".config/phpthunder/tasks/{key:016x}.json")))
}

impl TaskList {
    /// The project's saved list; empty when missing or unreadable.
    pub fn load(root: &Path) -> Self {
        store_path(root)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Saves the list for the project at `root`.
    ///
    /// # Errors
    /// Returns the I/O error.
    pub fn save(&self, root: &Path) -> std::io::Result<()> {
        let path = store_path(root).ok_or_else(|| std::io::Error::other("no HOME"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// The active task.
    pub fn active_task(&self) -> Option<&WorkTask> {
        let (id, _) = self.active.as_ref()?;
        self.tasks.iter().find(|t| t.id == *id)
    }

    /// Seconds spent on `task` at `now`, the running stretch included.
    pub fn spent(&self, task: &WorkTask, now: u64) -> u64 {
        let running = match &self.active {
            Some((id, since)) if *id == task.id => now.saturating_sub(*since),
            _ => 0,
        };
        task.seconds + running
    }

    /// Stops the timer of the active task, adding its time, and clears it.
    pub fn stop(&mut self, now: u64) {
        if let Some((id, since)) = self.active.take()
            && let Some(task) = self.tasks.iter_mut().find(|t| t.id == id)
        {
            task.seconds += now.saturating_sub(since);
        }
    }

    /// Makes `id` the active task from `now`, stopping the previous one.
    pub fn activate(&mut self, id: &str, now: u64) {
        self.stop(now);
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.done = false;
            self.active = Some((id.to_string(), now));
        }
    }

    /// Adds a typed-in task and returns its id.
    pub fn add_local(&mut self, title: &str) -> String {
        let n = self
            .tasks
            .iter()
            .filter_map(|t| t.id.strip_prefix("local-")?.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        let id = format!("local-{n}");
        self.tasks.push(WorkTask {
            id: id.clone(),
            title: title.trim().to_string(),
            url: None,
            seconds: 0,
            done: false,
        });
        id
    }

    /// Adds issue `number` unless already listed; returns its id.
    pub fn add_issue(&mut self, number: u64, title: &str, url: &str) -> String {
        let id = format!("#{number}");
        if !self.tasks.iter().any(|t| t.id == id) {
            self.tasks.push(WorkTask {
                id: id.clone(),
                title: title.to_string(),
                url: Some(url.to_string()),
                seconds: 0,
                done: false,
            });
        }
        id
    }

    /// Closes the active task: its time is added and it is marked done.
    pub fn close_active(&mut self, now: u64) -> Option<String> {
        let (id, _) = self.active.clone()?;
        self.stop(now);
        let task = self.tasks.iter_mut().find(|t| t.id == id)?;
        task.done = true;
        Some(task.title.clone())
    }

    /// A Markdown table of the time spent per task, most first.
    pub fn report(&self, now: u64, title: &str, headers: [&str; 3]) -> String {
        let mut rows: Vec<(&WorkTask, u64)> =
            self.tasks.iter().map(|t| (t, self.spent(t, now))).collect();
        rows.sort_by_key(|(_, s)| std::cmp::Reverse(*s));
        let total: u64 = rows.iter().map(|(_, s)| s).sum();
        let mut out = format!(
            "# {title}\n\n| {} | {} | {} |\n|---|---:|---|\n",
            headers[0], headers[1], headers[2]
        );
        for (task, seconds) in rows {
            let state = if task.done { "✓" } else { "" };
            out.push_str(&format!(
                "| {} {} | {} | {state} |\n",
                task.id,
                task.title.replace('|', "\\|"),
                duration(seconds)
            ));
        }
        out.push_str(&format!("\n**{}**\n", duration(total)));
        out
    }
}

/// `3h 05m`, `12m`, `40s`.
pub fn duration(seconds: u64) -> String {
    let (h, m) = (seconds / 3600, seconds % 3600 / 60);
    match (h, m) {
        (0, 0) => format!("{seconds}s"),
        (0, m) => format!("{m}m"),
        (h, m) => format!("{h}h {m:02}m"),
    }
}

/// Open GitHub issues of the repository at `root`, as (number, title,
/// url), through the `gh` CLI.
///
/// # Errors
/// Returns why `gh` could not list them (missing, not logged in, no
/// GitHub remote).
pub async fn github_issues(root: PathBuf) -> Result<Vec<(u64, String, String)>, String> {
    let out = tokio::process::Command::new("gh")
        .args([
            "issue",
            "list",
            "--json",
            "number,title,url",
            "--limit",
            "50",
        ])
        .current_dir(&root)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    Ok(json
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| {
            Some((
                i["number"].as_u64()?,
                i["title"].as_str()?.to_string(),
                i["url"].as_str().unwrap_or_default().to_string(),
            ))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_tasks_should_add_the_time_spent() {
        let mut list = TaskList::default();
        let a = list.add_local("Write docs");
        let b = list.add_issue(12, "Fix login", "https://example.test/12");
        list.activate(&a, 100);
        list.activate(&b, 160);
        assert_eq!(
            (
                list.spent(&list.tasks[0], 400),
                list.spent(&list.tasks[1], 400)
            ),
            (60, 240)
        );
    }

    #[test]
    fn closing_should_mark_done_and_stop_the_timer() {
        let mut list = TaskList::default();
        let a = list.add_local("Docs");
        list.activate(&a, 0);
        list.close_active(30);
        assert_eq!(
            (
                list.tasks[0].seconds,
                list.tasks[0].done,
                list.active.is_none()
            ),
            (30, true, true)
        );
    }

    #[test]
    fn names_should_follow_the_task() {
        let mut list = TaskList::default();
        list.add_issue(12, "Fix the Login page!", "");
        let task = &list.tasks[0];
        assert_eq!(
            (task.branch_name(), task.commit_message()),
            (
                "task/12-fix-the-login-page".to_string(),
                "#12 Fix the Login page!".to_string()
            )
        );
    }

    #[test]
    fn duration_should_pick_readable_units() {
        assert_eq!(
            [duration(40), duration(720), duration(11_100)],
            ["40s", "12m", "3h 05m"]
        );
    }
}
