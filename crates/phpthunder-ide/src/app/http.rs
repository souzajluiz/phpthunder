//! Running the request at the caret of a `.http` file and showing the
//! response in a tab; picking the environment its variables come from.

use super::*;
use crate::palette::{Item, Target};
use crate::tr;
use iced::Task;
use std::path::Path;
use std::sync::Mutex;

/// The environment of `http-client.env.json` requests use.
static ENVIRONMENT: Mutex<Option<String>> = Mutex::new(None);

/// The chosen environment, `dev` until one is picked.
fn environment() -> String {
    ENVIRONMENT
        .lock()
        .ok()
        .and_then(|e| e.clone())
        .unwrap_or_else(|| "dev".to_string())
}

impl App {
    /// Sends the request at the caret of the active `.http` file; the
    /// response opens in a tab ([`Message::HttpResponse`]).
    pub(crate) fn run_http_request(&mut self) -> Task<Message> {
        let Some(doc) = self
            .doc()
            .filter(|d| d.file_type == crate::filetype::FileType::Http)
        else {
            self.notice = Some(tr!("http-not-a-request-file"));
            return Task::none();
        };
        let offset = doc.offset(doc.cursor());
        let Some(request) = crate::http_client::request_at(&doc.text, offset) else {
            self.notice = Some(tr!("http-no-request"));
            return Task::none();
        };
        let dir = doc.path.parent().map(Path::to_path_buf).unwrap_or_default();
        let root = self.tree.root.clone().unwrap_or_else(|| dir.clone());
        let mut vars = crate::http_client::environment(&dir, &root, &environment());
        vars.extend(crate::http_client::file_variables(&doc.text));
        let name = if request.name.is_empty() {
            format!("{} {}", request.method, request.url)
        } else {
            request.name.clone()
        };
        self.notice = Some(tr!("http-sending", name = name.as_str()));
        Task::perform(crate::http_client::send(request, vars), move |result| {
            Message::HttpResponse(name, result)
        })
    }

    /// Opens a response (or reports the error).
    pub(crate) fn http_response(
        &mut self,
        name: &str,
        result: Result<String, String>,
    ) -> Task<Message> {
        let text = match result {
            Ok(text) => text,
            Err(error) => {
                self.fail(tr!("http-failed", error = error));
                return Task::none();
            }
        };
        let safe: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .take(60)
            .collect();
        let dir = std::env::temp_dir().join("phpthunder-http");
        let path = dir.join(format!("{safe}.response.http"));
        match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, text)) {
            Ok(()) => {
                self.notice = None;
                self.open(path, None)
            }
            Err(error) => {
                self.fail(error.to_string());
                Task::none()
            }
        }
    }

    /// Lists the environments of the `http-client.env.json` files near the
    /// active file to choose from.
    pub(crate) fn choose_http_environment(&mut self) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        let dir = self
            .doc()
            .and_then(|d| d.path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| root.clone());
        let mut names: Vec<String> = [root.as_path(), dir.as_path()]
            .iter()
            .flat_map(|f| {
                ["http-client.env.json", "http-client.private.env.json"].map(|n| f.join(n))
            })
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .filter_map(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .flat_map(|v| v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()))
            .flatten()
            .collect();
        names.sort();
        names.dedup();
        if names.is_empty() {
            self.notice = Some(tr!("http-no-environments"));
            return Task::none();
        }
        let current = environment();
        let items = names
            .into_iter()
            .map(|n| Item {
                badge: if n == current { "●" } else { "○" },
                title: n.clone(),
                matched: Vec::new(),
                detail: String::new(),
                target: Target::HttpEnvironment(n),
            })
            .collect();
        self.open_choice(tr!("http-environments-title"), items)
    }

    /// Makes `name` the environment of later requests.
    pub(crate) fn set_http_environment(&mut self, name: String) -> Task<Message> {
        self.notice = Some(tr!("http-environment-set", name = name.as_str()));
        if let Ok(mut env) = ENVIRONMENT.lock() {
            *env = Some(name);
        }
        Task::none()
    }
}
