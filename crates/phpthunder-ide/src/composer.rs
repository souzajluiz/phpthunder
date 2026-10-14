//! Composer: the project's scripts plus the common commands, to run in the
//! integrated terminal, and problems in `composer.json` (packages that are
//! not installed, packages required twice).

use crate::tr;
use std::path::Path;

/// Commands offered even without scripts, with what they do. The ones
/// ending with a space need a package name: they are typed into the
/// terminal for the user to finish.
fn standard() -> [(&'static str, String); 14] {
    [
        ("install", tr!("composer-install")),
        ("update", tr!("composer-update")),
        ("require ", tr!("composer-require")),
        ("require --dev ", tr!("composer-require-dev")),
        ("remove ", tr!("composer-remove")),
        (
            "update --with-dependencies ",
            tr!("composer-update-package"),
        ),
        ("dump-autoload", tr!("composer-dump-autoload")),
        ("outdated --direct", tr!("composer-outdated")),
        ("validate", tr!("composer-validate")),
        ("show --tree", tr!("composer-show-tree")),
        ("licenses", tr!("composer-licenses")),
        ("check-platform-reqs", tr!("composer-platform")),
        ("diagnose", tr!("composer-diagnose")),
        ("clear-cache", tr!("composer-clear-cache")),
    ]
}

/// A command line for `composer` and its description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerCommand {
    /// Arguments after `composer`.
    pub args: String,
    /// Shown next to the command in the picker.
    pub description: String,
}

impl ComposerCommand {
    /// Whether the command needs more input (a package name) before it
    /// runs: it is typed into the terminal, not run.
    pub fn is_draft(&self) -> bool {
        self.args.ends_with(' ')
    }
}

/// A problem found in `composer.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonProblem {
    /// Byte range of the package name (with its quotes).
    pub start: usize,
    /// End byte.
    pub end: usize,
    /// What is wrong.
    pub kind: JsonProblemKind,
    /// The package.
    pub package: String,
}

/// What is wrong with a requirement in `composer.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonProblemKind {
    /// Required but missing from `vendor/composer/installed.json`.
    NotInstalled,
    /// In both `require` and `require-dev`.
    Duplicated,
}

/// Packages installed in the project's `vendor`, lowercase; `None` when
/// nothing is installed yet (then nothing is reported as missing).
fn installed_packages(root: &Path) -> Option<std::collections::HashSet<String>> {
    let text = std::fs::read_to_string(root.join("vendor/composer/installed.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    // Composer 2 wraps the list in `packages`; Composer 1 writes the list.
    let list = json.get("packages").unwrap_or(&json).as_array()?;
    Some(
        list.iter()
            .filter_map(|p| p.get("name")?.as_str())
            .map(str::to_ascii_lowercase)
            .collect(),
    )
}

/// Problems in `text`, the project's `composer.json`: required packages
/// that are not installed and packages in both `require` and
/// `require-dev`. Platform requirements (`php`, `ext-*`) are skipped.
pub fn json_problems(root: &Path, text: &str) -> Vec<JsonProblem> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let installed = installed_packages(root);
    let names = |section: &str| -> Vec<String> {
        json.get(section)
            .and_then(|r| r.as_object())
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    };
    let require = names("require");
    let dev = names("require-dev");
    let platform =
        |n: &str| n == "php" || n.starts_with("ext-") || n.starts_with("lib-") || !n.contains('/');
    // Where a package name is written inside a section, as `"name"`.
    let position = |section: &str, name: &str| -> Option<(usize, usize)> {
        let at = text.find(&format!("\"{section}\""))?;
        let needle = format!("\"{name}\"");
        let found = at + text[at..].find(&needle)?;
        Some((found, found + needle.len()))
    };
    let mut out = Vec::new();
    for (section, list) in [("require", &require), ("require-dev", &dev)] {
        for name in list.iter().filter(|n| !platform(n)) {
            let Some((start, end)) = position(section, name) else {
                continue;
            };
            let missing = installed
                .as_ref()
                .is_some_and(|i| !i.contains(&name.to_ascii_lowercase()));
            if missing {
                out.push(JsonProblem {
                    start,
                    end,
                    kind: JsonProblemKind::NotInstalled,
                    package: name.clone(),
                });
            }
            if section == "require-dev" && require.contains(name) {
                out.push(JsonProblem {
                    start,
                    end,
                    kind: JsonProblemKind::Duplicated,
                    package: name.clone(),
                });
            }
        }
    }
    out
}

/// The project's `composer.json` scripts followed by the standard commands.
/// Empty when the project has no `composer.json`.
pub fn composer_commands(root: &Path) -> Vec<ComposerCommand> {
    let Ok(text) = std::fs::read_to_string(root.join("composer.json")) else {
        return Vec::new();
    };
    let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    let mut commands: Vec<ComposerCommand> = json
        .get("scripts")
        .and_then(|s| s.as_object())
        .map(|scripts| {
            scripts
                .iter()
                .filter(|(name, _)| !name.starts_with("pre-") && !name.starts_with("post-"))
                .map(|(name, body)| ComposerCommand {
                    args: name.clone(),
                    description: match body {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Array(steps) => {
                            tr!("composer-steps", count = steps.len())
                        }
                        _ => tr!("composer-script"),
                    },
                })
                .collect()
        })
        .unwrap_or_default();
    commands.extend(
        standard()
            .into_iter()
            .map(|(args, description)| ComposerCommand {
                args: args.to_string(),
                description,
            }),
    );
    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_problems_should_find_missing_and_duplicated_packages() {
        let dir =
            std::env::temp_dir().join(format!("phpthunder-composer-json-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("vendor/composer")).unwrap();
        std::fs::write(
            dir.join("vendor/composer/installed.json"),
            r#"{"packages":[{"name":"laravel/framework"},{"name":"pestphp/pest"}]}"#,
        )
        .unwrap();
        let text = r#"{"require":{"php":"^8.2","laravel/framework":"^11","spatie/missing":"^1"},"require-dev":{"pestphp/pest":"^3","laravel/framework":"^11"}}"#;
        let problems: Vec<(JsonProblemKind, String)> = json_problems(&dir, text)
            .into_iter()
            .map(|p| (p.kind, p.package))
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            problems,
            [
                (JsonProblemKind::NotInstalled, "spatie/missing".to_string()),
                (JsonProblemKind::Duplicated, "laravel/framework".to_string()),
            ]
        );
    }

    #[test]
    fn composer_commands_should_list_scripts_then_standard_commands() {
        let dir =
            std::env::temp_dir().join(format!("phpthunder-composer-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("composer.json"),
            r#"{"scripts":{"test":"pest","lint":["pint","phpstan"],"post-install-cmd":"x"}}"#,
        )
        .unwrap();
        let names: Vec<String> = composer_commands(&dir)
            .into_iter()
            .map(|c| c.args)
            .collect();
        assert_eq!(&names[..2], ["lint", "test"]);
        assert!(names.contains(&"install".to_string()));
        assert!(!names.iter().any(|n| n.starts_with("post-")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
