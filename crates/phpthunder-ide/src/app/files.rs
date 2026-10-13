//! Files changed outside the editor: reloading unmodified tabs, keeping
//! the index, project tree, file lists, Git status and Laravel keys current.

use super::*;
use crate::tr;
use iced::Task;
use std::path::{Path, PathBuf};

/// Whether `path` is PHP source the index should hold.
fn is_php_source(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "php")
}

impl App {
    /// Applies a batch of changed paths reported by the file watcher.
    /// Open tabs without unsaved edits take the new content; a tab with
    /// unsaved edits keeps them and a notice says the file changed on disk.
    pub(crate) fn files_changed(&mut self, paths: Vec<PathBuf>) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut conflicts = Vec::new();
        let mut git_changed = false;
        for path in paths {
            if path.components().any(|c| c.as_os_str() == ".git") {
                git_changed = true;
                continue;
            }
            git_changed = true;
            if let Some(dir) = path.parent() {
                self.tree.refresh(dir);
            }
            if path.is_dir() {
                self.tree.refresh(&path);
                continue;
            }
            let exists = path.is_file();
            if !exists {
                if is_php_source(&path) {
                    self.lang.remove_file(&path);
                }
                self.project_files.retain(|p| p != &path);
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(doc) = self.docs.iter_mut().find(|d| d.path == path)
                && doc.text != text
            {
                if doc.dirty {
                    conflicts.push(doc.title());
                    continue;
                }
                let cursor = doc.cursor();
                doc.set_text(text.clone(), cursor);
                doc.mark_saved();
                self.inspect_pending = true;
            }
            if is_php_source(&path) {
                self.lang.update_file(&path, &text);
            }
            let in_vendor = path.components().any(|c| c.as_os_str() == "vendor");
            let list = if in_vendor {
                &mut self.vendor_files
            } else {
                &mut self.project_files
            };
            if let Err(at) = list.binary_search(&path) {
                list.insert(at, path.clone());
            }
            tasks.push(self.rescan_laravel_after_save(&path));
            let paths: Vec<PathBuf> = self.docs.iter().map(|d| d.path.clone()).collect();
            for p in paths.iter().filter(|p| **p == path) {
                self.refresh_git_marks(p);
            }
        }
        if !conflicts.is_empty() {
            self.notice = Some(tr!("notice-changed-on-disk", files = conflicts.join(", ")));
        }
        if git_changed {
            self.branch = self
                .tree
                .root
                .as_deref()
                .and_then(crate::project::git_branch);
            tasks.push(self.refresh_git());
        }
        Task::batch(tasks)
    }
}

/// Widget id of the file name prompt's input.
pub const PROMPT_INPUT: &str = "name-prompt-input";

/// PHP namespace for classes in `dir`, from the `autoload` and
/// `autoload-dev` PSR-4 maps of the project's `composer.json` (the longest
/// matching folder wins). `None` outside every mapped folder.
pub(crate) fn namespace_for(root: &Path, dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join("composer.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let rel = dir.strip_prefix(root).ok()?;
    let mut best: Option<(usize, String)> = None;
    for section in ["autoload", "autoload-dev"] {
        let Some(map) = json
            .get(section)
            .and_then(|a| a.get("psr-4"))
            .and_then(|m| m.as_object())
        else {
            continue;
        };
        for (prefix, folders) in map {
            let folders: Vec<&str> = match folders {
                serde_json::Value::String(s) => vec![s.as_str()],
                serde_json::Value::Array(a) => a.iter().filter_map(|v| v.as_str()).collect(),
                _ => Vec::new(),
            };
            for folder in folders {
                let base = Path::new(folder.trim_end_matches('/'));
                let Ok(rest) = rel.strip_prefix(base) else {
                    continue;
                };
                let depth = base.components().count();
                if best.as_ref().is_some_and(|(d, _)| *d > depth) {
                    continue;
                }
                let mut ns = prefix.trim_end_matches('\\').to_string();
                for part in rest.components() {
                    ns.push('\\');
                    ns.push_str(&part.as_os_str().to_string_lossy());
                }
                best = Some((depth, ns));
            }
        }
    }
    best.map(|(_, ns)| ns)
}

/// Source of a new PHP class file.
fn class_source(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(ns) if !ns.is_empty() => format!("<?php\n\nnamespace {ns};\n\nclass {name}\n{{\n}}\n"),
        _ => format!("<?php\n\nclass {name}\n{{\n}}\n"),
    }
}

/// Moves `path` to the user's Trash (macOS `~/.Trash`, else the XDG trash
/// folder), picking a free name there.
///
/// # Errors
/// Returns the I/O error.
fn move_to_trash(path: &Path) -> std::io::Result<()> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("no HOME"))?;
    let trash = if cfg!(target_os = "macos") {
        home.join(".Trash")
    } else {
        home.join(".local/share/Trash/files")
    };
    std::fs::create_dir_all(&trash)?;
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("no file name"))?;
    let mut target = trash.join(name);
    let mut n = 1;
    while target.exists() {
        target = trash.join(format!("{} {n}", name.to_string_lossy()));
        n += 1;
    }
    std::fs::rename(path, target)
}

impl App {
    /// Opens the action list for a tree entry (right click on its row).
    pub(crate) fn show_file_actions(&mut self, path: PathBuf) -> Task<Message> {
        let dir = if path.is_dir() {
            path.clone()
        } else {
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| path.clone())
        };
        let item = |badge: &'static str, title: String, action: FileAction| crate::palette::Item {
            badge,
            title,
            matched: Vec::new(),
            detail: String::new(),
            target: crate::palette::Target::FileAction(action),
        };
        let items = vec![
            item("+", tr!("file-new-file"), FileAction::NewFile(dir.clone())),
            item(
                "+",
                tr!("file-new-folder"),
                FileAction::NewFolder(dir.clone()),
            ),
            item("C", tr!("file-new-class"), FileAction::NewClass(dir)),
            item("✎", tr!("file-rename"), FileAction::Rename(path.clone())),
            item("⌫", tr!("file-delete"), FileAction::Delete(path.clone())),
            item("◎", tr!("file-reveal"), FileAction::Reveal(path.clone())),
            item(
                "⧉",
                tr!("file-copy-path"),
                FileAction::CopyPath(path.clone()),
            ),
        ];
        let title = self.relative(&path);
        self.open_choice(title, items)
    }

    /// `path` relative to the project root, for titles and prompts.
    fn relative(&self, path: &Path) -> String {
        self.tree
            .root
            .as_deref()
            .and_then(|r| path.strip_prefix(r).ok())
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    /// Runs a picked file action: actions needing a name open the prompt.
    pub(crate) fn run_file_action(&mut self, action: FileAction) -> Task<Message> {
        let (title, value) = match &action {
            FileAction::NewFile(dir) => (
                tr!("file-new-file-in", dir = self.relative(dir)),
                String::new(),
            ),
            FileAction::NewFolder(dir) => (
                tr!("file-new-folder-in", dir = self.relative(dir)),
                String::new(),
            ),
            FileAction::NewClass(dir) => (
                tr!("file-new-class-in", dir = self.relative(dir)),
                String::new(),
            ),
            FileAction::Rename(path) => (
                tr!("file-rename-title", path = self.relative(path)),
                self.relative(path),
            ),
            FileAction::Delete(path) => {
                let path = path.clone();
                return match move_to_trash(&path) {
                    Ok(()) => {
                        self.after_file_moved(&path, None);
                        self.notice = Some(tr!("file-deleted", path = self.relative(&path)));
                        Task::none()
                    }
                    Err(e) => {
                        self.notice = Some(tr!("file-op-failed", error = e.to_string()));
                        Task::none()
                    }
                };
            }
            FileAction::Reveal(path) => {
                let _ = std::process::Command::new("open")
                    .arg("-R")
                    .arg(path)
                    .spawn();
                return Task::none();
            }
            FileAction::CopyPath(path) => {
                return iced::clipboard::write(self.relative(path));
            }
        };
        self.name_prompt = Some(NamePrompt {
            action,
            title,
            value,
            error: None,
        });
        Task::batch([
            iced::widget::operation::focus(iced::widget::Id::new(PROMPT_INPUT)),
            iced::widget::operation::select_all(iced::widget::Id::new(PROMPT_INPUT)),
        ])
    }

    /// Carries out the prompt's action with the typed name.
    pub(crate) fn submit_name_prompt(&mut self) -> Task<Message> {
        let Some(prompt) = self.name_prompt.as_ref() else {
            return Task::none();
        };
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let value = prompt.value.trim().to_string();
        if value.is_empty() || value.contains("..") {
            if let Some(p) = &mut self.name_prompt {
                p.error = Some(tr!("file-bad-name"));
            }
            return Task::none();
        }
        let result: Result<Option<PathBuf>, String> = match prompt.action.clone() {
            FileAction::NewFile(dir) => create_file(&dir.join(&value), ""),
            FileAction::NewFolder(dir) => {
                let path = dir.join(&value);
                std::fs::create_dir_all(&path)
                    .map(|()| None)
                    .map_err(|e| e.to_string())
            }
            FileAction::NewClass(dir) => {
                let name = value.trim_end_matches(".php");
                let namespace = namespace_for(&root, &dir);
                create_file(
                    &dir.join(format!("{name}.php")),
                    &class_source(namespace.as_deref(), name),
                )
            }
            FileAction::Rename(from) => {
                let to = root.join(&value);
                if to.exists() {
                    Err(tr!("file-exists", path = value))
                } else {
                    to.parent()
                        .map(std::fs::create_dir_all)
                        .transpose()
                        .and_then(|_| std::fs::rename(&from, &to))
                        .map_err(|e| e.to_string())
                        .map(|()| {
                            self.after_file_moved(&from, Some(&to));
                            None
                        })
                }
            }
            _ => Ok(None),
        };
        match result {
            Ok(created) => {
                self.name_prompt = None;
                if let Some(dir) = created.as_deref().and_then(Path::parent) {
                    self.tree.reveal(dir);
                    self.tree.refresh(dir);
                }
                match created {
                    Some(path) => self.open(path, None),
                    None => Task::none(),
                }
            }
            Err(error) => {
                if let Some(p) = &mut self.name_prompt {
                    p.error = Some(error);
                }
                Task::none()
            }
        }
    }

    /// Updates tabs, the index and the tree after `from` was moved to `to`
    /// (or deleted when `to` is `None`). Folders move their open files too.
    fn after_file_moved(&mut self, from: &Path, to: Option<&Path>) {
        for doc in &mut self.docs {
            if let (Ok(rest), Some(to)) = (doc.path.strip_prefix(from), to) {
                let new = if rest.as_os_str().is_empty() {
                    to.to_path_buf()
                } else {
                    to.join(rest)
                };
                self.lang.remove_file(&doc.path);
                doc.path = new;
                if doc.is_php() {
                    self.lang.update_file(&doc.path.clone(), &doc.text.clone());
                }
            }
        }
        if to.is_none() {
            let gone: Vec<usize> = self
                .docs
                .iter()
                .enumerate()
                .filter(|(_, d)| d.path.starts_with(from))
                .map(|(i, _)| i)
                .collect();
            for i in gone.into_iter().rev() {
                let _ = self.close_tab(i);
            }
        }
        self.lang.remove_file(from);
        for path in [Some(from), to].into_iter().flatten() {
            if let Some(dir) = path.parent() {
                self.tree.refresh(dir);
            }
        }
        self.project_files.retain(|p| !p.starts_with(from));
        if let Some(to) = to.filter(|t| t.is_file())
            && let Err(at) = self.project_files.binary_search(&to.to_path_buf())
        {
            self.project_files.insert(at, to.to_path_buf());
        }
    }
}

/// Creates `path` (and missing folders) with `content`, refusing to
/// overwrite; returns it so the caller opens it.
///
/// # Errors
/// Returns why it could not be created.
fn create_file(path: &Path, content: &str) -> Result<Option<PathBuf>, String> {
    if path.exists() {
        return Err(tr!("file-exists", path = path.display().to_string()));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, content).map_err(|e| e.to_string())?;
    Ok(Some(path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_for_should_follow_psr4_maps() {
        let root = std::env::temp_dir().join(format!("phpthunder-ns-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("composer.json"),
            r#"{"autoload":{"psr-4":{"App\\":"app/","Database\\Factories\\":"database/factories/"}},"autoload-dev":{"psr-4":{"Tests\\":"tests/"}}}"#,
        )
        .unwrap();
        assert_eq!(
            namespace_for(&root, &root.join("app/Models")).as_deref(),
            Some("App\\Models")
        );
        assert_eq!(
            namespace_for(&root, &root.join("app")).as_deref(),
            Some("App")
        );
        assert_eq!(
            namespace_for(&root, &root.join("tests/Unit")).as_deref(),
            Some("Tests\\Unit")
        );
        assert_eq!(
            namespace_for(&root, &root.join("database/factories")).as_deref(),
            Some("Database\\Factories")
        );
        assert_eq!(namespace_for(&root, &root.join("routes")), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn class_source_should_declare_the_namespace() {
        assert_eq!(
            class_source(Some("App\\Models"), "User"),
            "<?php\n\nnamespace App\\Models;\n\nclass User\n{\n}\n"
        );
    }
}
