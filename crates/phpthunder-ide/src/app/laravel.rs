//! Laravel keys in the editor: the project's route names, views, config
//! keys, translation keys and environment variables, completed and resolved
//! inside `route('…')`, `view('…')`, `config('…')`, `__('…')`, `env('…')`
//! and Blade directives.

use super::*;
use iced::Task;
use phpthunder::analysis::Candidate;
use phpthunder::laravel;
use phpthunder::symbol::Kind;
use phpthunder::syntax::Pos;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Most key candidates offered at once.
const MAX_KEY_CANDIDATES: usize = 200;

/// Project folders and files that declare keys; saving one rescans.
const KEY_DIRS: &[&str] = &[
    "routes",
    "config",
    "resources/views",
    "lang",
    "resources/lang",
    ".env",
    ".env.example",
];

impl App {
    /// Scans the open project for Laravel keys in the background; answered
    /// by [`Message::LaravelScanned`] (`None` for other projects).
    pub(crate) fn scan_laravel(&self) -> Task<Message> {
        let Some(root) = self.tree.root.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || laravel::scan_project(&root).map(Arc::new))
                    .await
                    .ok()
                    .flatten()
            },
            Message::LaravelScanned,
        )
    }

    /// Rescans after saving a file that may declare keys.
    pub(crate) fn rescan_laravel_after_save(&self, path: &Path) -> Task<Message> {
        let Some(root) = self.tree.root.as_deref() else {
            return Task::none();
        };
        let declares_keys =
            self.laravel.is_some() && KEY_DIRS.iter().any(|dir| path.starts_with(root.join(dir)));
        if declares_keys {
            self.scan_laravel()
        } else {
            Task::none()
        }
    }

    /// Completion inside a key string: where the replaced text starts (the
    /// string's content) and the matching keys. `None` outside key strings.
    pub(crate) fn laravel_candidates(
        &self,
        text: &str,
        offset: usize,
    ) -> Option<(usize, Vec<Candidate>)> {
        key_candidates(self.laravel.as_ref()?, text, offset)
    }

    /// Whether the caret of a Laravel project's PHP or Blade document sits
    /// in a string literal, from the highlight tokens (no parsing): where
    /// typing may complete route, view or config keys or Eloquent columns.
    /// The exact check happens in the background completion.
    pub(crate) fn in_string_of_laravel_project(&self) -> bool {
        self.laravel.is_some()
            && self.doc().is_some_and(|d| {
                let offset = d.offset(d.cursor());
                offset > 0 && d.token_at(offset - 1) == Some(crate::editor::Token::String)
            })
    }
}

/// Completion inside a key string: where the replaced text starts (the
/// string's content) and the matching keys. `None` outside key strings.
/// Runs on any thread (background completion uses it).
pub(crate) fn key_candidates(
    project: &laravel::LaravelProject,
    text: &str,
    offset: usize,
) -> Option<(usize, Vec<Candidate>)> {
    {
        let key = laravel::key_at(text, offset)?;
        let prefix = key.prefix.to_lowercase();
        let mut items: Vec<Candidate> = project
            .keys(key.kind)
            .iter()
            .filter(|k| k.name.to_lowercase().contains(&prefix))
            .map(|k| Candidate {
                label: k.name.clone(),
                kind: Kind::Constant,
                detail: k.detail.clone(),
                insert_text: k.name.clone(),
                import: None,
            })
            .collect();
        // Prefix matches first, then the rest in name order.
        items.sort_by_key(|c| !c.label.to_lowercase().starts_with(&prefix));
        items.truncate(MAX_KEY_CANDIDATES);
        Some((key.start, items))
    }
}

impl App {
    /// Declaration of the key, column or relation named by the string at
    /// `offset`, if any.
    pub(crate) fn laravel_definition(&self, text: &str, offset: usize) -> Option<(PathBuf, Pos)> {
        let project = self.laravel.as_ref()?;
        let found = laravel::key_at(text, offset)
            .and_then(|key| project.find(key.kind, &key.value))
            .map(|found| (found.path.clone(), found.pos));
        found.or_else(|| self.lang.eloquent_definition(text, offset))
    }
}
