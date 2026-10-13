//! Laravel keys and the project catalog of them.

use crate::syntax::Pos;
use std::path::PathBuf;

/// What a string key names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyKind {
    /// A named route (`->name('users.show')`).
    Route,
    /// A Blade view (`resources/views/users/index.blade.php` -> `users.index`).
    View,
    /// A configuration key (`config/app.php` `'name'` -> `app.name`).
    Config,
    /// A translation key (`lang/en/auth.php` `'failed'` -> `auth.failed`,
    /// or a whole string from `lang/en.json`).
    Translation,
    /// An environment variable from `.env` / `.env.example`.
    Env,
}

/// A key declared in the project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaravelKey {
    /// The key as written in code.
    pub name: String,
    /// File declaring it.
    pub path: PathBuf,
    /// Where in that file (start of the file for views).
    pub pos: Pos,
    /// Short context: the route's URI, the view's file, the config value.
    pub detail: String,
}

/// Every route name, view and config key of a Laravel project, built by
/// [`scan_project`].
#[derive(Clone, Debug, Default)]
pub struct LaravelProject {
    /// Named routes.
    pub routes: Vec<LaravelKey>,
    /// Blade views.
    pub views: Vec<LaravelKey>,
    /// Configuration keys, nested ones dotted.
    pub config: Vec<LaravelKey>,
    /// Translation keys of every locale, merged.
    pub translations: Vec<LaravelKey>,
    /// Environment variable names.
    pub env: Vec<LaravelKey>,
}

impl LaravelProject {
    /// The keys of one kind.
    pub fn keys(&self, kind: KeyKind) -> &[LaravelKey] {
        match kind {
            KeyKind::Route => &self.routes,
            KeyKind::View => &self.views,
            KeyKind::Config => &self.config,
            KeyKind::Translation => &self.translations,
            KeyKind::Env => &self.env,
        }
    }

    /// The declaration of `name`, if the project has it.
    pub fn find(&self, kind: KeyKind, name: &str) -> Option<&LaravelKey> {
        self.keys(kind).iter().find(|k| k.name == name)
    }
}

/// A string argument at the caret that names a Laravel key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyRef {
    /// What the string names.
    pub kind: KeyKind,
    /// Byte offset where the string's content starts.
    pub start: usize,
    /// Byte offset where it ends (before the closing quote).
    pub end: usize,
    /// Content between `start` and the caret, for completion filtering.
    pub prefix: String,
    /// The whole content.
    pub value: String,
}
