//! Reformatting any file. The file's EditorConfig style is the base layer.
//! On top of it each file type gets the first of:
//!
//! 1. the formatter the project itself installs: Pint / PHP-CS-Fixer /
//!    PHPCBF for PHP, Prettier for JavaScript, TypeScript, Vue, Astro and
//!    JSON, shfmt (on `PATH`) for shell scripts;
//! 2. a built-in one: Blade and HTML (re-indented by element and directive
//!    nesting), JSON, XML, Markdown tables, `.env` assignments;
//! 3. whitespace clean-up (trailing spaces, final newline, line endings).
//!
//! No tool is bundled or downloaded: the project's own versions and
//! configuration decide the style. YAML, CSS, Dockerfiles and plain text
//! get the clean-up only, as indentation carries meaning or no formatter
//! is installed.

pub mod dotenv;
pub mod editorconfig;
pub mod json;
pub mod markdown;
pub mod markup;
pub mod whitespace;
pub mod xml;

use crate::project::vendor_bin;
use crate::tr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

/// Formatter found in `vendor/bin`, in order of preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formatter {
    /// Laravel Pint.
    Pint,
    /// PHP-CS-Fixer.
    PhpCsFixer,
    /// PHP_CodeSniffer's fixer.
    Phpcbf,
    /// Prettier from the project's `node_modules`.
    Prettier,
    /// shfmt found on `PATH`.
    Shfmt,
    /// The built-in Blade formatter.
    Blade,
    /// The built-in HTML formatter.
    Html,
    /// The built-in JSON formatter.
    Json,
    /// The built-in XML formatter.
    Xml,
    /// The built-in Markdown formatter (tables).
    Markdown,
    /// The built-in `.env` formatter.
    DotEnv,
    /// No formatter for the file type: whitespace clean-up only.
    Whitespace,
}

impl Formatter {
    /// Display name for notices.
    pub fn name(self) -> &'static str {
        match self {
            Formatter::Pint => "Pint",
            Formatter::PhpCsFixer => "PHP-CS-Fixer",
            Formatter::Phpcbf => "PHPCBF",
            Formatter::Prettier => "Prettier",
            Formatter::Shfmt => "shfmt",
            Formatter::Blade => "Blade",
            Formatter::Html => "HTML",
            Formatter::Json => "JSON",
            Formatter::Xml => "XML",
            Formatter::Markdown => "Markdown",
            Formatter::DotEnv => ".env",
            Formatter::Whitespace => "EditorConfig",
        }
    }
}

/// The formatter installed in the project, if any.
pub fn detect_formatter(root: &Path) -> Option<(Formatter, PathBuf)> {
    [
        ("pint", Formatter::Pint),
        ("php-cs-fixer", Formatter::PhpCsFixer),
        ("phpcbf", Formatter::Phpcbf),
    ]
    .into_iter()
    .find_map(|(bin, f)| vendor_bin(root, bin).map(|p| (f, p)))
}

/// Formats `text` (the unsaved buffer of `file_name`) and returns the new
/// text and the formatter used. The buffer is written to a temporary file,
/// formatted there with the project as working directory (so the project's
/// configuration applies), then read back.
///
/// `Ok(None)` when the project has no formatter installed.
///
/// # Errors
/// Returns a message when the formatter cannot run or fails.
pub async fn format_php(
    root: PathBuf,
    file_name: String,
    text: String,
) -> Result<Option<(String, Formatter)>, String> {
    let Some((formatter, bin)) = detect_formatter(&root) else {
        return Ok(None);
    };
    let interpreter = crate::interpreter::Interpreter::of(&root);
    // The formatter must see the copy: in a container, inside the project.
    let dir = interpreter.scratch_dir(&root).join("format");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;
    let file = dir.join(&file_name);
    tokio::fs::write(&file, &text)
        .await
        .map_err(|e| e.to_string())?;
    let mut args: Vec<std::ffi::OsString> = vec![bin.into_os_string()];
    match formatter {
        Formatter::Pint => args.extend([file.clone().into_os_string(), "--no-interaction".into()]),
        Formatter::PhpCsFixer => args.extend([
            "fix".into(),
            file.clone().into_os_string(),
            "--using-cache=no".into(),
            "--quiet".into(),
        ]),
        Formatter::Phpcbf => args.extend(["-q".into(), file.clone().into_os_string()]),
        // `detect_formatter` only yields the PHP tools above.
        _ => {}
    }
    let mut cmd = interpreter.command(&root, "php", &args);
    let output = cmd
        .output()
        .await
        .map_err(|e| tr!("error-run-php", error = e.to_string()))?;
    let formatted = tokio::fs::read_to_string(&file)
        .await
        .map_err(|e| e.to_string())?;
    let _ = tokio::fs::remove_file(&file).await;
    // PHPCBF exits with 1 when it fixed something; only a missing result is an error.
    if formatted.is_empty() && !text.is_empty() {
        return Err(tr!(
            "error-formatter-failed",
            formatter = formatter.name(),
            error = String::from_utf8_lossy(&output.stderr).trim().to_string()
        ));
    }
    Ok(Some((formatted, formatter)))
}

/// Extensions Prettier takes when the project installs it: Prettier's
/// usual IDE scope (scripts, Vue, Astro, JSON). Other types it supports
/// keep their built-in formatting unless configured otherwise.
const PRETTIER_EXTENSIONS: &[&str] = &[
    "js", "ts", "jsx", "tsx", "cjs", "cts", "mjs", "mts", "json", "vue", "astro", "svelte", "css",
    "scss", "less",
];

/// Extensions the built-in JSON formatter takes (`composer.lock` too).
const JSON_EXTENSIONS: &[&str] = &["json", "jsonc", "json5", "lock"];

/// Extensions the built-in XML formatter takes (`phpunit.xml.dist` too).
const XML_EXTENSIONS: &[&str] = &["xml", "dist", "xsd", "xsl", "svg", "xlf", "xliff"];

/// The executable `node_modules/.bin/<name>` of the project, if installed.
pub(crate) fn node_bin(root: &Path, name: &str) -> Option<PathBuf> {
    let path = root.join("node_modules").join(".bin").join(name);
    path.is_file().then_some(path)
}

/// `name` on `PATH`, if installed.
pub(crate) fn path_bin(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// Indentation the file already uses: a tab, or the smallest run of spaces
/// among its first indented lines. `None` for a file with no indentation.
fn detected_indent(text: &str) -> Option<String> {
    let mut leads = text.lines().filter_map(|line| {
        let lead = line.len() - line.trim_start_matches([' ', '\t']).len();
        (lead > 0 && !line.trim().is_empty()).then(|| &line[..lead])
    });
    let first = leads.next()?;
    if first.starts_with('\t') {
        return Some("\t".into());
    }
    let smallest = std::iter::once(first)
        .chain(leads.take(20))
        .filter(|w| !w.starts_with('\t'))
        .map(str::len)
        .min()?;
    Some(" ".repeat(smallest))
}

/// Runs `cmd` in `root` with `input` on stdin and returns its stdout.
///
/// # Errors
/// Returns a message when the tool cannot start or exits with an error.
async fn pipe(
    mut cmd: tokio::process::Command,
    root: &Path,
    input: &str,
    formatter: Formatter,
) -> Result<String, String> {
    let failed = |error: String| {
        tr!(
            "error-formatter-failed",
            formatter = formatter.name(),
            error = error
        )
    };
    let mut child = cmd
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| failed(e.to_string()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|e| failed(e.to_string()))?;
    }
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| failed(e.to_string()))?;
    if !output.status.success() {
        return Err(failed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| failed(e.to_string()))
}

/// Reformats `text`, the (possibly unsaved) buffer of `path` in the project
/// at `root`, choosing the formatter by file type as the module docs
/// describe. Called by the Reformat command for every file type. A file
/// type without a formatter, or a JSON/XML file the built-in formatter
/// cannot read (a syntax error), gets whitespace clean-up per its
/// EditorConfig style ([`Formatter::Whitespace`]).
///
/// # Errors
/// Returns a message when a project tool fails.
pub async fn format_file(
    root: PathBuf,
    path: PathBuf,
    text: String,
) -> Result<(String, Formatter), String> {
    let style = editorconfig::style_for(&path, &root);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e).to_string();
    let clean = |text: &str, formatter| {
        let markdown = matches!(ext.as_str(), "md" | "markdown");
        (whitespace::normalize(text, &style, markdown), formatter)
    };
    let filepath = path.to_string_lossy().into_owned();

    if ext == "php" && !name.ends_with(".blade.php") {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return match format_php(root, file_name, text.clone()).await? {
            Some(done) => Ok(done),
            None => Ok(clean(&text, Formatter::Whitespace)),
        };
    }
    if PRETTIER_EXTENSIONS.contains(&ext.as_str())
        && let Some(bin) = node_bin(&root, "prettier")
    {
        let mut cmd = tokio::process::Command::new(bin);
        cmd.arg("--stdin-filepath").arg(&filepath);
        let out = pipe(cmd, &root, &text, Formatter::Prettier).await?;
        return Ok((out, Formatter::Prettier));
    }
    if matches!(ext.as_str(), "sh" | "bash" | "zsh")
        && let Some(bin) = path_bin("shfmt")
    {
        let mut cmd = tokio::process::Command::new(bin);
        cmd.arg("-filename").arg(&filepath);
        // shfmt reads .editorconfig itself only when given a real file;
        // pass the indentation explicitly.
        if style.indent_style.is_some() || style.indent_size.is_some() {
            let width = match style.indent_style {
                Some(editorconfig::IndentStyle::Tab) => 0,
                _ => style.indent_size.unwrap_or(4),
            };
            cmd.arg("-i").arg(width.to_string());
        }
        let out = pipe(cmd, &root, &text, Formatter::Shfmt).await?;
        return Ok((out, Formatter::Shfmt));
    }
    // Built-ins indent per EditorConfig, else as the file already does,
    // else with the usual width for the language.
    let indent = |default: usize| {
        if style.indent_style.is_some() || style.indent_size.is_some() {
            style.indent_unit(default)
        } else {
            detected_indent(&text).unwrap_or_else(|| " ".repeat(default))
        }
    };
    let builtin = if name.ends_with(".blade.php") {
        Some((markup::format(&text, &indent(4), true), Formatter::Blade))
    } else if matches!(ext.as_str(), "html" | "htm") {
        Some((markup::format(&text, &indent(4), false), Formatter::Html))
    } else if JSON_EXTENSIONS.contains(&ext.as_str()) {
        json::format(&text, &indent(2))
            .ok()
            .map(|t| (t, Formatter::Json))
    } else if XML_EXTENSIONS.contains(&ext.as_str()) && text.trim_start().starts_with('<') {
        xml::format(&text, &indent(4))
            .ok()
            .map(|t| (t, Formatter::Xml))
    } else if matches!(ext.as_str(), "md" | "markdown") {
        Some((markdown::align_tables(&text), Formatter::Markdown))
    } else if name == ".env" || name.starts_with(".env.") {
        Some((dotenv::format(&text), Formatter::DotEnv))
    } else {
        None
    };
    Ok(match builtin {
        Some((formatted, formatter)) => clean(&formatted, formatter),
        None => clean(&text, Formatter::Whitespace),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_indent_should_use_the_smallest_run() {
        let json = "{\n    \"a\": {\n        \"b\": 1\n    }\n}";
        assert_eq!(detected_indent(json), Some("    ".into()));
        assert_eq!(detected_indent("<a>\n\t<b/>\n</a>"), Some("\t".into()));
        assert_eq!(detected_indent("plain"), None);
    }

    /// Formats `text` as the file `name` in an empty temporary project.
    async fn run(name: &str, text: &str) -> (String, Formatter) {
        let root =
            std::env::temp_dir().join(format!("phpthunder-fmt-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let out = format_file(root.clone(), root.join(name), text.into())
            .await
            .unwrap();
        let _ = std::fs::remove_dir_all(&root);
        out
    }

    #[tokio::test]
    async fn format_file_should_pick_a_builtin_by_file_type() {
        assert_eq!(
            run("package.json", "{\"a\":[1]}").await,
            ("{\n  \"a\": [1]\n}\n".into(), Formatter::Json)
        );
        assert_eq!(
            run("phpunit.xml.dist", "<a><b/></a>").await,
            ("<a>\n    <b/>\n</a>\n".into(), Formatter::Xml)
        );
        assert_eq!(
            run(".env", "A = 1   \n\n").await,
            ("A=1\n".into(), Formatter::DotEnv)
        );
        assert_eq!(
            run("welcome.blade.php", "@if ($a)\n<p>x</p>\n@endif").await,
            ("@if ($a)\n    <p>x</p>\n@endif\n".into(), Formatter::Blade)
        );
        assert_eq!(
            run("docker-compose.yml", "a:\n  b: 1   \n").await,
            ("a:\n  b: 1\n".into(), Formatter::Whitespace)
        );
    }

    #[tokio::test]
    async fn format_file_should_only_clean_whitespace_of_broken_json() {
        assert_eq!(
            run("x.json", "{\"a\": [  ").await,
            ("{\"a\": [\n".into(), Formatter::Whitespace)
        );
    }

    #[tokio::test]
    async fn format_file_should_follow_editorconfig() {
        let root = std::env::temp_dir().join(format!("phpthunder-fmt-ec-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join(".editorconfig"),
            "root = true\n[*.json]\nindent_style = tab\n",
        )
        .unwrap();
        let out = format_file(root.clone(), root.join("a.json"), "{\"a\":1}".into())
            .await
            .unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(out, ("{\n\t\"a\": 1\n}\n".into(), Formatter::Json));
    }
}
