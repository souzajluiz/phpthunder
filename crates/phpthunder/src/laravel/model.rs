//! Laravel keys and the project catalog of them.

use crate::syntax::Pos;
use std::path::PathBuf;

/// What a string key names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    /// A route middleware alias or group (`auth`, `web`, `throttle:api`).
    Middleware,
    /// A gate or policy ability (`Gate::define('edit-post')`, `update`).
    Ability,
    /// An authentication guard (`config/auth.php` `guards`).
    Guard,
    /// A filesystem disk (`config/filesystems.php` `disks`).
    Disk,
    /// A cache store (`config/cache.php` `stores`).
    CacheStore,
    /// A queue connection (`config/queue.php` `connections`).
    QueueConnection,
    /// A log channel (`config/logging.php` `channels`).
    LogChannel,
    /// A database connection (`config/database.php` `connections`).
    DbConnection,
    /// A mailer (`config/mail.php` `mailers`).
    Mailer,
    /// A Pennant feature (`Feature::define('new-api')`).
    Feature,
    /// An Inertia page component (`resources/js/Pages/Users/Index.vue` ->
    /// `Users/Index`).
    InertiaPage,
    /// A Livewire component (`app/Livewire/Users/Table.php` -> `users.table`).
    Livewire,
    /// A Blade stack (`@stack('scripts')`), filled with `@push`.
    Stack,
    /// A Blade section (`@yield('content')` / `@section('content')`).
    Section,
    /// A file Vite builds (`resources/js/app.js`), for `@vite`.
    ViteAsset,
    /// A file under `public` (`css/app.css`), for `asset()`.
    Asset,
    /// An Artisan command (`migrate`, `app:send-emails`).
    Command,
    /// A validation rule name (`required`, `email`), inside a rule string.
    ValidationRule,
    /// A service container alias (`app('cache')`).
    Binding,
}

impl KeyKind {
    /// Every kind, in declaration order.
    pub const ALL: &[KeyKind] = &[
        KeyKind::Route,
        KeyKind::View,
        KeyKind::Config,
        KeyKind::Translation,
        KeyKind::Env,
        KeyKind::Middleware,
        KeyKind::Ability,
        KeyKind::Guard,
        KeyKind::Disk,
        KeyKind::CacheStore,
        KeyKind::QueueConnection,
        KeyKind::LogChannel,
        KeyKind::DbConnection,
        KeyKind::Mailer,
        KeyKind::Feature,
        KeyKind::InertiaPage,
        KeyKind::Livewire,
        KeyKind::Stack,
        KeyKind::Section,
        KeyKind::ViteAsset,
        KeyKind::Asset,
        KeyKind::Command,
        KeyKind::ValidationRule,
        KeyKind::Binding,
    ];

    /// Short English label of the kind, used in messages from the engine
    /// (front ends may translate it).
    pub fn label(self) -> &'static str {
        match self {
            KeyKind::Route => "route",
            KeyKind::View => "view",
            KeyKind::Config => "config key",
            KeyKind::Translation => "translation key",
            KeyKind::Env => "environment variable",
            KeyKind::Middleware => "middleware",
            KeyKind::Ability => "ability",
            KeyKind::Guard => "guard",
            KeyKind::Disk => "disk",
            KeyKind::CacheStore => "cache store",
            KeyKind::QueueConnection => "queue connection",
            KeyKind::LogChannel => "log channel",
            KeyKind::DbConnection => "database connection",
            KeyKind::Mailer => "mailer",
            KeyKind::Feature => "feature",
            KeyKind::InertiaPage => "Inertia page",
            KeyKind::Livewire => "Livewire component",
            KeyKind::Stack => "stack",
            KeyKind::Section => "section",
            KeyKind::ViteAsset => "Vite asset",
            KeyKind::Asset => "asset",
            KeyKind::Command => "Artisan command",
            KeyKind::ValidationRule => "validation rule",
            KeyKind::Binding => "container binding",
        }
    }
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

/// Every key a Laravel project declares (routes, views, config keys,
/// middleware, …), built by [`scan_project`].
#[derive(Clone, Debug, Default)]
pub struct LaravelProject {
    /// Keys by kind; sorted by name, without duplicates.
    pub keys: std::collections::HashMap<KeyKind, Vec<LaravelKey>>,
    /// Blade components (`<x-alert>`), anonymous and class-based.
    pub components: Vec<BladeComponent>,
    /// Variables passed to each view by `view('name', [...])` calls, by
    /// view name.
    pub view_variables: std::collections::HashMap<String, Vec<String>>,
}

/// A Blade component usable as `<x-name>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BladeComponent {
    /// Tag name after `x-` (`forms.input`).
    pub name: String,
    /// File declaring it (the view, or the component class).
    pub path: PathBuf,
    /// Attributes it accepts: `@props` keys or constructor parameters,
    /// kebab-cased.
    pub props: Vec<String>,
}

impl LaravelProject {
    /// The keys of one kind (empty when the project declares none).
    pub fn keys(&self, kind: KeyKind) -> &[LaravelKey] {
        self.keys.get(&kind).map_or(&[], Vec::as_slice)
    }

    /// The declaration of `name`, if the project has it.
    pub fn find(&self, kind: KeyKind, name: &str) -> Option<&LaravelKey> {
        self.keys(kind).iter().find(|k| k.name == name)
    }

    /// Adds keys of `kind`; [`LaravelProject::finish`] sorts them later.
    pub fn add(&mut self, kind: KeyKind, keys: impl IntoIterator<Item = LaravelKey>) {
        self.keys.entry(kind).or_default().extend(keys);
    }

    /// Sorts every kind's keys by name and drops repeated names, keeping
    /// the first declaration of each.
    pub fn finish(&mut self) {
        for keys in self.keys.values_mut() {
            keys.sort_by(|a, b| a.name.cmp(&b.name));
            keys.dedup_by(|a, b| a.name == b.name);
        }
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
