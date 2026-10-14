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

/// The `scripts` of the project's `package.json` (Vite, webpack, Next and
/// friends), as (title, command line) with the package manager its lock
/// file names: pnpm, Yarn, Bun or npm.
fn package_scripts(root: &std::path::Path) -> Vec<(String, String)> {
    let Some(json) = std::fs::read_to_string(root.join("package.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    else {
        return Vec::new();
    };
    let manager = [
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("bun.lockb", "bun run"),
        ("bun.lock", "bun run"),
    ]
    .into_iter()
    .find(|(lock, _)| root.join(lock).is_file())
    .map_or("npm run", |(_, m)| m);
    json["scripts"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, _)| (name.clone(), format!("{manager} {name}")))
        .collect()
}

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
        if root.join("bin/console").is_file() {
            items.push(item(
                "▷",
                "console cache:clear".into(),
                "php bin/console cache:clear",
            ));
            items.push(item(
                "▷",
                "console debug:router".into(),
                "php bin/console debug:router",
            ));
        }
        for (name, line) in package_scripts(&root) {
            items.push(Item {
                badge: "▷",
                title: name,
                matched: Vec::new(),
                detail: line.clone(),
                // Node runs on the host, not through the PHP interpreter.
                target: Target::Command(line),
            });
        }
        if root.join("vendor/bin/drush").is_file() {
            items.push(item(
                "▷",
                "drush cache:rebuild".into(),
                "vendor/bin/drush cr",
            ));
        }
        if root.join("wp-config.php").is_file() || root.join("wp-config-sample.php").is_file() {
            items.push(item("▷", "wp server".into(), "wp server"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_scripts_should_use_the_locked_package_manager() {
        let dir = std::env::temp_dir().join(format!("phpthunder-scripts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
        std::fs::write(dir.join("pnpm-lock.yaml"), "").unwrap();
        let scripts = package_scripts(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(scripts, [("dev".to_string(), "pnpm dev".to_string())]);
    }
}
