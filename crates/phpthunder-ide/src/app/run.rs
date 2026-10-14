//! Run configurations (`⌃⌥R`): the project's own, from
//! `.phpthunder/project.toml`, plus the obvious ones (the current file,
//! `artisan serve`, PHP's built-in server), run in the terminal through
//! the project's interpreter.

use super::*;
use crate::interpreter::{Interpreter, PROJECT_FILE, PROJECT_TEMPLATE, project_config};
use crate::keymap::Cmd;
use crate::palette::{Item, Target};
use crate::tr;
use iced::Task;

impl App {
    /// Opens the list of run configurations; the choice runs in the
    /// terminal.
    pub(crate) fn open_run_configurations(&mut self) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let config = project_config(&root);
        let interpreter = &config.interpreter;
        let item = |badge: &'static str, title: String, command: &str| Item {
            badge,
            title,
            matched: Vec::new(),
            detail: format!("{command}  ·  {}", interpreter.label()),
            target: Target::Command(interpreter.run_line(&root, command)),
        };
        let mut items: Vec<Item> = config
            .run
            .iter()
            .map(|r| item("▶", r.name.clone(), &r.command))
            .collect();
        if let Some(doc) = self
            .doc()
            .filter(|d| d.is_php() && !d.title().ends_with(".blade.php"))
        {
            let rel = doc
                .path
                .strip_prefix(&root)
                .unwrap_or(&doc.path)
                .to_string_lossy()
                .into_owned();
            items.push(item(
                "▷",
                tr!("run-current-file", file = doc.title()),
                &format!("php {rel}"),
            ));
        }
        if root.join("artisan").is_file() {
            items.push(item("▷", "artisan serve".into(), "php artisan serve"));
        }
        if interpreter.is_local() {
            let docroot = if root.join("public").is_dir() {
                "public"
            } else {
                "."
            };
            items.push(item(
                "▷",
                tr!("run-builtin-server"),
                &format!("php -S localhost:8000 -t {docroot}"),
            ));
        }
        items.push(Item {
            badge: "⚙",
            title: tr!("run-edit-configurations"),
            matched: Vec::new(),
            detail: PROJECT_FILE.into(),
            target: Target::Action(Cmd::EditProjectSettings),
        });
        self.open_choice(tr!("run-title"), items)
    }

    /// Opens `.phpthunder/project.toml`, creating it with every setting
    /// commented out when missing.
    pub(crate) fn edit_project_settings(&mut self) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let path = root.join(PROJECT_FILE);
        if !path.exists() {
            let written = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, PROJECT_TEMPLATE));
            if let Err(e) = written {
                self.fail(tr!(
                    "notice-write-failed",
                    path = path.display().to_string(),
                    error = e.to_string()
                ));
                return Task::none();
            }
            if let Some(dir) = path.parent() {
                self.tree.refresh(dir);
            }
        }
        self.open(path, None)
    }

    /// Re-reads the project's interpreter for the status bar.
    pub(crate) fn refresh_interpreter(&mut self) {
        self.interpreter_label = self
            .tree
            .root
            .as_deref()
            .map(Interpreter::of)
            .filter(|i| !i.is_local())
            .map(|i| i.label());
    }
}
