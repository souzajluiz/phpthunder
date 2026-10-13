//! The service container's core aliases (`app('cache')`), for completion
//! and for typing `app('…')` / `resolve('…')` calls.

use super::*;
use std::path::PathBuf;

/// Container aliases Laravel registers, with the class each resolves to.
const BINDINGS: &[(&str, &str)] = &[
    ("app", "Illuminate\\Foundation\\Application"),
    ("auth", "Illuminate\\Auth\\AuthManager"),
    ("auth.driver", "Illuminate\\Contracts\\Auth\\Guard"),
    (
        "blade.compiler",
        "Illuminate\\View\\Compilers\\BladeCompiler",
    ),
    ("cache", "Illuminate\\Cache\\CacheManager"),
    ("cache.store", "Illuminate\\Cache\\Repository"),
    ("config", "Illuminate\\Config\\Repository"),
    ("cookie", "Illuminate\\Cookie\\CookieJar"),
    ("db", "Illuminate\\Database\\DatabaseManager"),
    ("db.connection", "Illuminate\\Database\\Connection"),
    ("db.schema", "Illuminate\\Database\\Schema\\Builder"),
    ("encrypter", "Illuminate\\Encryption\\Encrypter"),
    ("events", "Illuminate\\Events\\Dispatcher"),
    ("files", "Illuminate\\Filesystem\\Filesystem"),
    ("filesystem", "Illuminate\\Filesystem\\FilesystemManager"),
    (
        "filesystem.disk",
        "Illuminate\\Contracts\\Filesystem\\Filesystem",
    ),
    ("hash", "Illuminate\\Hashing\\HashManager"),
    ("log", "Illuminate\\Log\\LogManager"),
    ("mail.manager", "Illuminate\\Mail\\MailManager"),
    ("mailer", "Illuminate\\Mail\\Mailer"),
    ("queue", "Illuminate\\Queue\\QueueManager"),
    ("redirect", "Illuminate\\Routing\\Redirector"),
    ("redis", "Illuminate\\Redis\\RedisManager"),
    ("request", "Illuminate\\Http\\Request"),
    ("router", "Illuminate\\Routing\\Router"),
    ("session", "Illuminate\\Session\\SessionManager"),
    ("session.store", "Illuminate\\Session\\Store"),
    ("translator", "Illuminate\\Translation\\Translator"),
    ("url", "Illuminate\\Routing\\UrlGenerator"),
    ("validator", "Illuminate\\Validation\\Factory"),
    ("view", "Illuminate\\View\\Factory"),
];

/// Functions that resolve a container binding from their first argument.
const RESOLVERS: &[&str] = &["app", "resolve"];

/// The class `function('name')` returns when `function` resolves container
/// bindings and `name` is a core alias. Used by type inference.
pub fn container_binding(function: &str, name: &str) -> Option<&'static str> {
    let function = function.trim_start_matches('\\');
    RESOLVERS
        .iter()
        .any(|r| r.eq_ignore_ascii_case(function))
        .then(|| {
            BINDINGS
                .iter()
                .find(|(alias, _)| *alias == name)
                .map(|(_, class)| *class)
        })
        .flatten()
}

/// Every core alias as a key, for completion (detail: the class).
pub(super) fn bindings() -> Vec<LaravelKey> {
    BINDINGS
        .iter()
        .map(|(alias, class)| LaravelKey {
            name: (*alias).to_string(),
            path: PathBuf::new(),
            pos: crate::syntax::Pos::default(),
            detail: (*class).to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_binding_should_type_core_aliases() {
        assert_eq!(
            container_binding("app", "cache"),
            Some("Illuminate\\Cache\\CacheManager")
        );
        assert_eq!(
            container_binding("\\resolve", "log"),
            Some("Illuminate\\Log\\LogManager")
        );
        assert_eq!(container_binding("app", "nope"), None);
        assert_eq!(container_binding("config", "cache"), None);
    }
}
