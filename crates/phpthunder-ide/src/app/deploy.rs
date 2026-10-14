//! Deployment commands: upload the project or the active file to a
//! `[[deploy]]` server, and upload saved files to servers that ask for it.

use super::*;
use crate::interpreter::project_config;
use crate::palette::{Item, Target};
use crate::tr;
use iced::Task;
use std::path::Path;

impl App {
    /// Lists the project's servers; the choice uploads the project
    /// (`whole`) or the active file.
    pub(crate) fn choose_deploy_server(&mut self, whole: bool) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let servers = project_config(&root).deploy;
        if servers.is_empty() {
            self.notice = Some(tr!("deploy-no-servers"));
            return self.edit_project_settings();
        }
        let items = servers
            .into_iter()
            .map(|s| Item {
                badge: "⇪",
                detail: format!("{:?} · {}", s.protocol, s.host),
                title: s.name.clone(),
                matched: Vec::new(),
                target: Target::Deploy(s.name, whole),
            })
            .collect();
        let title = if whole {
            tr!("deploy-project-title")
        } else {
            tr!("deploy-file-title")
        };
        self.open_choice(title, items)
    }

    /// Uploads the project or the active file to the server `name`.
    pub(crate) fn deploy(&mut self, name: String, whole: bool) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let Some(server) = project_config(&root)
            .deploy
            .into_iter()
            .find(|s| s.name == name)
        else {
            return Task::none();
        };
        let command = if whole {
            match crate::deploy::upload_project(&server, &root) {
                Some(command) => command,
                None => {
                    self.notice = Some(tr!("deploy-files-only", name = name.as_str()));
                    return Task::none();
                }
            }
        } else {
            let Some(file) = self.doc().map(|d| d.path.clone()) else {
                return Task::none();
            };
            crate::deploy::upload_file(&server, &root, &file)
        };
        self.notice = Some(tr!("deploy-uploading", name = name.as_str()));
        Task::perform(crate::deploy::run(root, command), move |result| {
            Message::Deployed(name, result)
        })
    }

    /// Uploads the saved `path` to every server with `upload_on_save`.
    pub(crate) fn deploy_after_save(&self, path: &Path) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        Task::batch(
            project_config(&root)
                .deploy
                .into_iter()
                .filter(|s| s.upload_on_save)
                .map(|s| {
                    let command = crate::deploy::upload_file(&s, &root, path);
                    let name = s.name;
                    Task::perform(crate::deploy::run(root.clone(), command), move |result| {
                        Message::Deployed(name, result)
                    })
                }),
        )
    }

    /// Reports an upload: errors always, success as a notice.
    pub(crate) fn deployed(&mut self, name: &str, result: Result<(), String>) {
        match result {
            Ok(()) => self.notice = Some(tr!("deploy-done", name = name)),
            Err(error) => self.fail(tr!("deploy-failed", name = name, error = error)),
        }
    }
}
