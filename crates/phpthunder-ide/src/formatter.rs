//! Reformatting PHP with the project's own formatter (Pint, PHP-CS-Fixer
//! or PHPCBF), so the result matches its configured code style.

use crate::project::vendor_bin;
use std::path::{Path, PathBuf};

/// Formatter found in `vendor/bin`, in order of preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formatter {
    /// Laravel Pint.
    Pint,
    /// PHP-CS-Fixer.
    PhpCsFixer,
    /// PHP_CodeSniffer's fixer.
    Phpcbf,
}

impl Formatter {
    /// Display name for notices.
    pub fn name(self) -> &'static str {
        match self {
            Formatter::Pint => "Pint",
            Formatter::PhpCsFixer => "PHP-CS-Fixer",
            Formatter::Phpcbf => "PHPCBF",
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
/// # Errors
/// Returns a message when no formatter is installed or it fails.
pub async fn format_php(
    root: PathBuf,
    file_name: String,
    text: String,
) -> Result<(String, Formatter), String> {
    let (formatter, bin) = detect_formatter(&root).ok_or("No formatter found (install laravel/pint, friendsofphp/php-cs-fixer or squizlabs/php_codesniffer)")?;
    let dir = std::env::temp_dir().join(format!("phpthunder-format-{}", std::process::id()));
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;
    let file = dir.join(&file_name);
    tokio::fs::write(&file, &text)
        .await
        .map_err(|e| e.to_string())?;
    let mut cmd = tokio::process::Command::new("php");
    cmd.current_dir(&root).arg(&bin);
    match formatter {
        Formatter::Pint => cmd.arg(&file).arg("--no-interaction"),
        Formatter::PhpCsFixer => cmd
            .arg("fix")
            .arg(&file)
            .arg("--using-cache=no")
            .arg("--quiet"),
        Formatter::Phpcbf => cmd.arg("-q").arg(&file),
    };
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("cannot run php: {e}"))?;
    let formatted = tokio::fs::read_to_string(&file)
        .await
        .map_err(|e| e.to_string())?;
    let _ = tokio::fs::remove_file(&file).await;
    // PHPCBF exits with 1 when it fixed something; only a missing result is an error.
    if formatted.is_empty() && !text.is_empty() {
        return Err(format!(
            "{} failed: {}",
            formatter.name(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok((formatted, formatter))
}
