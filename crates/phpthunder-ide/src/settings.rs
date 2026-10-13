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

# Panel sizes in pixels, saved whenever a panel is resized.
# sidebar_width = 280
# bottom_height = 260
# right_width = 460
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
    /// Width of the project sidebar; written by the IDE on resize.
    pub sidebar_width: Option<u32>,
    /// Height of the bottom tool panel; written by the IDE on resize.
    pub bottom_height: Option<u32>,
    /// Width of the right tool panel; written by the IDE on resize.
    pub right_width: Option<u32>,
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
            sidebar_width: None,
            bottom_height: None,
            right_width: None,
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

/// `~/.config/phpthunder` as a path, or `None` in snapshots and tests
/// (which must not touch the user's files) or without a home folder. Used
/// by `log` for the error log.
pub fn user_config_dir() -> Option<PathBuf> {
    config_dir().filter(|_| !is_snapshot()).map(PathBuf::from)
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
            crate::log::error(format_args!("could not write settings: {error}"));
        }
        return Settings::default();
    }
    loader.find_and_load().unwrap_or_else(|error| {
        crate::log::warn(format_args!("ignoring invalid settings file: {error}"));
        Settings::default()
    })
}

/// Writes the panel sizes into the settings file, keeping every other line
/// and comment: `settings.toml` is edited line by line, `settings.json` (if
/// that is the file in use) rewritten with the keys set. Called when a
/// panel drag ends or a handle is reset; failures are reported on stderr.
pub fn save_panel_sizes(sidebar: u32, bottom: u32, right: u32) {
    let Some(dir) = config_dir().filter(|_| !is_snapshot()) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let keys = [
        ("sidebar_width", sidebar),
        ("bottom_height", bottom),
        ("right_width", right),
    ];
    let json = dir.join(format!("{SETTINGS_FILE}.json"));
    let toml = dir.join(format!("{SETTINGS_FILE}.toml"));
    let written = if json.is_file() && !toml.is_file() {
        std::fs::read_to_string(&json).and_then(|text| {
            let mut value: serde_json::Value =
                serde_json::from_str(&text).map_err(std::io::Error::other)?;
            if let Some(map) = value.as_object_mut() {
                for (key, size) in keys {
                    map.insert(key.to_string(), size.into());
                }
            }
            let text = serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?;
            std::fs::write(&json, text)
        })
    } else {
        let text = std::fs::read_to_string(&toml).unwrap_or_else(|_| SETTINGS_TEMPLATE.into());
        let text = keys
            .iter()
            .fold(text, |text, (key, size)| set_toml_key(&text, key, *size));
        std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&toml, text))
    };
    if let Err(error) = written {
        crate::log::error(format_args!("could not save panel sizes: {error}"));
    }
}

/// `text` with the top-level `key = value` line (or its commented-out
/// form) replaced by `key = <value>`, or appended when neither exists.
/// Only lines before the first `[table]` count, so a nested key with the
/// same name is never touched.
fn set_toml_key(text: &str, key: &str, value: u32) -> String {
    let line = format!("{key} = {value}");
    let matches = |l: &str| {
        let l = l.trim_start().trim_start_matches('#').trim_start();
        l.strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    };
    let top = text
        .lines()
        .position(|l| l.trim_start().starts_with('['))
        .unwrap_or(usize::MAX);
    let mut lines: Vec<&str> = text.lines().collect();
    match lines.iter().take(top).position(|l| matches(l)) {
        Some(i) => lines[i] = &line,
        None => lines.insert(top.min(lines.len()), &line),
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
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
        crate::log::error(format_args!("could not save session: {error}"));
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

    #[test]
    fn set_toml_key_should_uncomment_the_template_line() {
        let text = set_toml_key(
            "# a\n# sidebar_width = 280\nsoft_wrap = true\n",
            "sidebar_width",
            300,
        );
        assert_eq!(text, "# a\nsidebar_width = 300\nsoft_wrap = true\n");
    }

    #[test]
    fn set_toml_key_should_append_before_the_first_table() {
        let text = set_toml_key(
            "soft_wrap = true\n[x]\nright_width = 1\n",
            "right_width",
            500,
        );
        assert_eq!(
            text,
            "soft_wrap = true\nright_width = 500\n[x]\nright_width = 1\n"
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
