//! Composer: the project's scripts plus the common commands, to run in the
//! integrated terminal.

use crate::tr;
use std::path::Path;

/// Commands offered even without scripts, with what they do.
fn standard() -> [(&'static str, String); 5] {
    [
        ("install", tr!("composer-install")),
        ("update", tr!("composer-update")),
        ("dump-autoload", tr!("composer-dump-autoload")),
        ("outdated --direct", tr!("composer-outdated")),
        ("validate", tr!("composer-validate")),
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
