//! Files the IDE keeps in `~/.config/phpthunder`, read and written with
//! `universal-config`: the user's settings (`settings.toml`, or
//! `settings.json`), loaded once at start, and the work session
//! (`session.json`: open projects and their tabs). Development snapshots
//! never read or write either, so they cannot change the user's files.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;
use universal_config::{ConfigLoader, Format};

/// Program name the loader is built for.
const APP: &str = "phpthunder";

/// Settings file name, without extension.
const SETTINGS_FILE: &str = "settings";

/// Session file name, without extension.
const SESSION_FILE: &str = "session";

/// The settings file written on first start: every key with its default,
/// the optional ones commented out, so the user sees all of them.
const SETTINGS_TEMPLATE: &str = r#"# PhpThunder settings. Remove a line to go back to its default.

# UI language ("pt-BR", "en-US"); the system's when unset.
# language = "pt-BR"

# Reopen the last project and its tabs on start.
restore_session = true

# Projects listed as recent on the welcome screen.
recent_projects = 12

# Soft wrap long lines when the IDE starts.
soft_wrap = false

# Shell for the terminal and the Claude panel; $SHELL when unset.
# shell = "/bin/zsh"

# Command the Claude panel runs.
claude_command = "claude"
"#;

/// What the user can set in `settings.toml`. Missing keys keep their
/// defaults, so the file only needs the lines the user changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// UI language (`pt-BR`, `en-US`); the system's when unset.
    pub language: Option<String>,
    /// Reopen the last project and its tabs on start.
    pub restore_session: bool,
    /// Projects listed as recent on the welcome screen.
    pub recent_projects: usize,
    /// Soft wrap long lines when the IDE starts.
    pub soft_wrap: bool,
    /// Shell for the terminal and the Claude panel; `$SHELL` when unset.
    pub shell: Option<String>,
    /// Command the Claude panel runs.
    pub claude_command: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: None,
            restore_session: true,
            recent_projects: 12,
            soft_wrap: false,
            shell: None,
            claude_command: "claude".to_string(),
        }
    }
}

/// Whether the user's files must be left alone: in development snapshots
/// (see `app::snapshot`) and in tests.
fn is_snapshot() -> bool {
    cfg!(test) || std::env::var_os("PHPTHUNDER_SNAPSHOT").is_some()
}

/// `~/.config/phpthunder` on every system, so settings sit with the
/// session the IDE always kept there; `None` without a home folder.
fn config_dir() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let dir = PathBuf::from(home).join(".config").join(APP);
    Some(dir.to_string_lossy().into_owned())
}

/// The user's settings, loaded on first use: `settings.toml` (or `.json`)
/// when present and valid, else the defaults. A missing file is created
/// from [`SETTINGS_TEMPLATE`] so the user finds every key; an invalid one
/// is reported on stderr and left alone. Used wherever a setting applies.
pub fn settings() -> &'static Settings {
    static SETTINGS: OnceLock<Settings> = OnceLock::new();
    SETTINGS.get_or_init(load_settings)
}

/// Reads the settings file, writing the defaults when there is none.
fn load_settings() -> Settings {
    let Some(dir) = config_dir().filter(|_| !is_snapshot()) else {
        return Settings::default();
    };
    let formats = [Format::Toml, Format::Json];
    let loader = ConfigLoader::new(APP)
        .with_config_dir(&dir)
        .with_file_name(SETTINGS_FILE)
        .with_formats(&formats);
    let exists = ["toml", "json"].iter().any(|ext| {
        PathBuf::from(&dir)
            .join(format!("{SETTINGS_FILE}.{ext}"))
            .is_file()
    });
    if !exists {
        let path = PathBuf::from(&dir).join(format!("{SETTINGS_FILE}.toml"));
        let written =
            std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, SETTINGS_TEMPLATE));
        if let Err(error) = written {
            eprintln!("could not write settings: {error}");
        }
        return Settings::default();
    }
    loader.find_and_load().unwrap_or_else(|error| {
        eprintln!("ignoring invalid settings file: {error}");
        Settings::default()
    })
}

/// Reads the saved session as `T`; `None` when missing, unreadable, in a
/// snapshot, or without a home folder.
pub fn load_session<T: serde::de::DeserializeOwned>() -> Option<T> {
    let dir = config_dir().filter(|_| !is_snapshot())?;
    let formats = [Format::Json];
    ConfigLoader::new(APP)
        .with_config_dir(&dir)
        .with_file_name(SESSION_FILE)
        .with_formats(&formats)
        .find_and_load()
        .ok()
}

/// Writes the session; failures only cost what the next start restores.
pub fn save_session<T: Serialize>(session: &T) {
    let Some(dir) = config_dir().filter(|_| !is_snapshot()) else {
        return;
    };
    let formats = [Format::Json];
    let loader = ConfigLoader::new(APP)
        .with_config_dir(&dir)
        .with_file_name(SESSION_FILE)
        .with_formats(&formats);
    if let Err(error) = loader.save(session, &Format::Json) {
        eprintln!("could not save session: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_template_should_parse_to_the_defaults() {
        assert_eq!(toml_from(SETTINGS_TEMPLATE), Settings::default());
    }

    #[test]
    fn settings_should_fill_missing_keys_with_defaults() {
        let parsed: Settings = toml_from("soft_wrap = true\n");
        assert_eq!(
            parsed,
            Settings {
                soft_wrap: true,
                ..Settings::default()
            }
        );
    }

    /// Parses TOML through the same loader the IDE uses.
    fn toml_from(text: &str) -> Settings {
        let unique = format!("{}-{}", std::process::id(), text.len());
        let dir = std::env::temp_dir().join(format!("phpthunder-settings-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.toml");
        std::fs::write(&path, text).unwrap();
        let parsed = ConfigLoader::load(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        parsed
    }
}
