//! Claude plan limits for the Claude panel. The `claude` CLI hands its
//! status line command a JSON snapshot that includes `rate_limits` (share
//! of the 5-hour and weekly windows used, and when each resets). The panel
//! starts the CLI with an extra settings file whose status line command
//! copies that JSON to a file, then pipes it on to the user's own status
//! line command, so their status line keeps working. The IDE reads the
//! copy; no credentials or network calls are involved.

use std::path::{Path, PathBuf};

/// One rate-limit window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    /// Share of the window used, 0–100.
    pub used_percent: f64,
    /// When the window resets, in Unix seconds; `None` when not reported.
    pub resets_at: Option<i64>,
}

/// The plan limits reported by the CLI. Both windows are `None` before the
/// first answer of a session and on plans without such limits (API keys).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Usage {
    /// The rolling 5-hour session window.
    pub five_hour: Option<Window>,
    /// The weekly window.
    pub seven_day: Option<Window>,
}

impl Usage {
    /// Whether any window is known.
    pub fn is_known(&self) -> bool {
        self.five_hour.is_some() || self.seven_day.is_some()
    }
}

/// File the status line command copies the CLI's JSON to; one per IDE
/// process so two open windows never read each other's session.
pub fn status_file() -> PathBuf {
    std::env::temp_dir().join(format!(
        "phpthunder-claude-status-{}.json",
        std::process::id()
    ))
}

/// Writes the extra settings file for the Claude panel and returns the
/// arguments that load it (`--settings <file>`), to append to the panel's
/// command. `None` when the file cannot be written: the panel then starts
/// without limits rather than failing.
pub fn settings_args() -> Option<String> {
    let path = std::env::temp_dir().join(format!(
        "phpthunder-claude-settings-{}.json",
        std::process::id()
    ));
    let command = status_line_command(&status_file(), user_status_line().as_deref());
    let settings = serde_json::json!({
        "statusLine": { "type": "command", "command": command }
    });
    std::fs::write(&path, settings.to_string()).ok()?;
    Some(format!(
        "--settings {}",
        shell_quote(&path.to_string_lossy())
    ))
}

/// Status line command that saves its input to `file` and then runs `user`
/// (the user's own status line command) on the same input, or prints
/// nothing when there is none.
fn status_line_command(file: &Path, user: Option<&str>) -> String {
    let file = shell_quote(&file.to_string_lossy());
    match user {
        Some(user) => format!("tee {file} | {user}"),
        None => format!("cat > {file}"),
    }
}

/// The `statusLine.command` of the user's `~/.claude/settings.json`, if
/// set, so the panel keeps the status line they configured.
fn user_status_line() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let text = std::fs::read_to_string(Path::new(&home).join(".claude/settings.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let command = json.get("statusLine")?.get("command")?.as_str()?;
    (!command.trim().is_empty()).then(|| command.to_string())
}

/// `text` single-quoted for a POSIX shell.
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Reads the limits from the last status line snapshot in `file`; the
/// default (nothing known) when it is missing or not yet written.
pub fn read(file: &Path) -> Usage {
    std::fs::read_to_string(file)
        .ok()
        .map(|text| parse(&text))
        .unwrap_or_default()
}

/// The `rate_limits` of one status line JSON snapshot.
fn parse(text: &str) -> Usage {
    let json: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let limits = json.get("rate_limits");
    let window = |key: &str| {
        let w = limits?.get(key)?;
        Some(Window {
            used_percent: w.get("used_percentage")?.as_f64()?,
            resets_at: w.get("resets_at").and_then(|r| r.as_i64()),
        })
    };
    Usage {
        five_hour: window("five_hour"),
        seven_day: window("seven_day"),
    }
}

/// Time left until `resets_at` (Unix seconds) as `2h 13m`, `13m` or `3d 4h`;
/// `None` once it has passed.
pub fn time_left(resets_at: i64, now: i64) -> Option<String> {
    let secs = resets_at.checked_sub(now).filter(|s| *s > 0)?;
    let (days, hours, minutes) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    Some(if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{}m", minutes.max(1))
    })
}

/// The current time in Unix seconds.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_should_read_both_windows() {
        let usage = parse(
            r#"{"model":{},"rate_limits":{"five_hour":{"used_percentage":42.5,"resets_at":1700000000},"seven_day":{"used_percentage":18,"resets_at":1700500000}}}"#,
        );
        assert_eq!(
            usage.five_hour,
            Some(Window {
                used_percent: 42.5,
                resets_at: Some(1_700_000_000)
            })
        );
    }

    #[test]
    fn parse_should_know_nothing_without_rate_limits() {
        assert!(!parse(r#"{"model":{}}"#).is_known());
    }

    #[test]
    fn time_left_should_show_hours_and_minutes() {
        assert_eq!(time_left(10_000, 10_000 - 7_980).as_deref(), Some("2h 13m"));
    }

    #[test]
    fn time_left_should_be_none_after_the_reset() {
        assert_eq!(time_left(100, 200), None);
    }

    #[test]
    fn status_line_command_should_pipe_into_the_user_command() {
        assert_eq!(
            status_line_command(Path::new("/tmp/s.json"), Some("~/bin/line")),
            "tee '/tmp/s.json' | ~/bin/line"
        );
    }

    #[test]
    fn shell_quote_should_escape_single_quotes() {
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }
}
