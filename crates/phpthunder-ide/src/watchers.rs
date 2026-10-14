//! File watchers: commands from `.phpthunder/project.toml` run when a
//! matching file is saved (compile Sass, minify scripts, regenerate code),
//! in the project folder, with the saved file's paths filled in.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// One `[[watcher]]` of the project file.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Watcher {
    /// Name shown when the command fails.
    pub name: String,
    /// Which saved files trigger it: an EditorConfig-style glob, matched
    /// against the file name, or the project-relative path when it holds `/`.
    pub files: String,
    /// The command line, run by the shell in the project folder. `$file`,
    /// `$dir`, `$name`, `$stem` and `$root` are replaced (quoted).
    pub command: String,
}

/// Quotes `s` as one shell word.
fn quote(s: &str) -> String {
    if cfg!(windows) {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// `watcher`'s command line for the saved `file`, variables replaced.
pub fn command_line(watcher: &Watcher, root: &Path, file: &Path) -> String {
    let text = |p: &Path| p.to_string_lossy().into_owned();
    let name = file
        .file_name()
        .map(Path::new)
        .map(text)
        .unwrap_or_default();
    let stem = file
        .file_stem()
        .map(Path::new)
        .map(text)
        .unwrap_or_default();
    let dir = file.parent().map(text).unwrap_or_default();
    // Longest names first, so `$file` is not cut by a shorter variable.
    [
        ("$file", text(file)),
        ("$stem", stem),
        ("$name", name),
        ("$root", text(root)),
        ("$dir", dir),
    ]
    .iter()
    .fold(watcher.command.clone(), |line, (var, value)| {
        line.replace(var, &quote(value))
    })
}

/// The watchers of `watchers` that `file` (inside `root`) triggers.
pub fn matching<'a>(watchers: &'a [Watcher], root: &Path, file: &Path) -> Vec<&'a Watcher> {
    let rel = file
        .strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/");
    watchers
        .iter()
        .filter(|w| crate::formatter::editorconfig::matches(&w.files, &rel))
        .collect()
}

/// Runs `line` with the system shell in `root`.
///
/// # Errors
/// Returns `name: ` plus the command's last output line (or the start
/// error) when it cannot start or exits with a failure.
pub async fn run(name: String, root: PathBuf, line: String) -> Result<(), String> {
    let mut command = if cfg!(windows) {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(&line);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(&line);
        c
    };
    let out = command
        .current_dir(&root)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| format!("{name}: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let last = stderr
        .lines()
        .chain(stdout.lines())
        .rfind(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string();
    Err(format!("{name}: {last}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A watcher compiling Sass files.
    fn sass() -> Watcher {
        Watcher {
            name: "Sass".into(),
            files: "*.scss".into(),
            command: "sass $file $dir/$stem.css".into(),
        }
    }

    #[test]
    fn matching_should_use_the_file_name_or_path() {
        let root = Path::new("/p");
        let scripts = Watcher {
            files: "resources/js/**/*.js".into(),
            ..sass()
        };
        let watchers = [sass(), scripts];
        let found = |f: &str| matching(&watchers, root, Path::new(f)).len();
        assert_eq!(
            [
                found("/p/a/b.scss"),
                found("/p/resources/js/x/app.js"),
                found("/p/app.js")
            ],
            [1, 1, 0]
        );
    }

    #[cfg(unix)]
    #[test]
    fn command_line_should_quote_the_paths() {
        let line = command_line(&sass(), Path::new("/p"), Path::new("/p/my dir/a.scss"));
        assert_eq!(line, "sass '/p/my dir/a.scss' '/p/my dir'/'a'.css");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_should_report_the_last_error_line() {
        let result = run(
            "Fail".into(),
            std::env::temp_dir(),
            "echo bad >&2; exit 3".into(),
        )
        .await;
        assert_eq!(result, Err("Fail: bad".to_string()));
    }
}
