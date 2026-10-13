//! File-type icons for the project tree, tabs and status bar: small text
//! badges for code files (`php`, `JS`, `M↓`), line icons for the rest, and
//! folders tinted by their role in a Laravel/PHP project.
//!
//! A file's kind is decided in order: exact file name, compound suffix
//! (`.blade.php` before `.php`, `Test.php`, `.env.*`), location (database
//! migrations), plain extension, then plain text.

use super::*;
use crate::app::Message;
use iced::widget::{container, row, text};
use iced::{Center, Color, Element, Font, Theme};
use std::path::Path;

/// Width reserved for a file icon, so names line up.
pub const FILE_ICON_WIDTH: f32 = 22.0;

/// PHP badge color.
const PHP: Color = rgb(0x8C9EFF);
/// Markdown badge color.
const MARKDOWN: Color = rgb(0x598BFF);
/// JavaScript square color.
const JAVASCRIPT: Color = rgb(0xCABA2D);
/// TypeScript square color.
const TYPESCRIPT: Color = rgb(0x4D85FF);
/// Blade templates.
const BLADE: Color = rgb(0xF05340);
/// Tests and test configuration.
const TEST: Color = rgb(0x5FB865);
/// Package manifests.
const PACKAGE: Color = rgb(0xD19A66);
/// Neutral line icons.
const NEUTRAL: Color = rgb(0xA4A1B3);

/// What kind of icon a file gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
    /// PHP source (and `artisan`).
    Php,
    /// Blade template.
    Blade,
    /// PHP test class (`*Test.php`) or test configuration (`phpunit.xml`, `Pest.php`).
    Test,
    /// Database migration.
    Migration,
    /// Composer or npm manifest and lock file.
    Package,
    /// `.env` and `.env.*`.
    Env,
    /// Dockerfile and compose files.
    Docker,
    /// Markdown.
    Markdown,
    /// JavaScript.
    JavaScript,
    /// TypeScript.
    TypeScript,
    /// JSON.
    Json,
    /// XML, HTML, Vue and SVG.
    Markup,
    /// Shell scripts.
    Shell,
    /// Ignore files.
    Ignore,
    /// Images.
    Image,
    /// SQL and SQLite.
    Database,
    /// Text and config files (`.yml`, `.gitattributes`, …).
    Text,
}

/// Exact file names (lowercase) with a kind of their own.
const NAMES: &[(&str, FileKind)] = &[
    ("artisan", FileKind::Php),
    ("composer.json", FileKind::Package),
    ("composer.lock", FileKind::Package),
    ("package.json", FileKind::Package),
    ("package-lock.json", FileKind::Package),
    ("phpunit.xml", FileKind::Test),
    ("phpunit.xml.dist", FileKind::Test),
    ("pest.php", FileKind::Test),
    (".env", FileKind::Env),
    ("dockerfile", FileKind::Docker),
    ("docker-compose.yml", FileKind::Docker),
    ("docker-compose.yaml", FileKind::Docker),
    ("compose.yaml", FileKind::Docker),
];

/// Plain extensions and their kinds.
const EXTENSIONS: &[(&[&str], FileKind)] = &[
    (&["php"], FileKind::Php),
    (&["md", "markdown"], FileKind::Markdown),
    (&["js", "mjs", "cjs", "jsx"], FileKind::JavaScript),
    (&["ts", "tsx", "mts"], FileKind::TypeScript),
    (&["json", "lock"], FileKind::Json),
    (
        &["xml", "html", "htm", "vue", "svg", "dist"],
        FileKind::Markup,
    ),
    (&["sh", "bash", "zsh"], FileKind::Shell),
    (
        &["png", "jpg", "jpeg", "gif", "webp", "ico"],
        FileKind::Image,
    ),
    (&["sql", "sqlite"], FileKind::Database),
];

/// The icon kind for the file at `path`.
pub(crate) fn file_kind(path: &Path) -> FileKind {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if let Some((_, kind)) = NAMES.iter().find(|(n, _)| *n == name) {
        return *kind;
    }
    if name.ends_with(".blade.php") {
        return FileKind::Blade;
    }
    if name.ends_with("test.php") && name.len() > "test.php".len() {
        return FileKind::Test;
    }
    if name.starts_with(".env.") {
        return FileKind::Env;
    }
    if name.ends_with("ignore") {
        return FileKind::Ignore;
    }
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    let in_migrations = path
        .parent()
        .is_some_and(|p| p.ends_with("database/migrations"));
    if ext == "php" && in_migrations {
        return FileKind::Migration;
    }
    EXTENSIONS
        .iter()
        .find(|(exts, _)| exts.contains(&ext))
        .map_or(FileKind::Text, |(_, kind)| *kind)
}

/// An italic text badge (`php`) in `color`.
fn badge<'a>(word: &'a str, color: Color, size: f32) -> Element<'a, Message> {
    text(word)
        .size(size)
        .font(Font {
            style: iced::font::Style::Italic,
            ..UI_BOLD
        })
        .color(color)
        .into()
}

/// A filled rounded square with a short label (`JS`).
fn square<'a>(word: &'a str, background: Color, size: f32) -> Element<'a, Message> {
    container(
        text(word)
            .size(size * 0.5)
            .font(UI_BOLD)
            .color(rgb(0x1D1D26)),
    )
    .width(size)
    .height(size)
    .center(size)
    .style(move |_: &Theme| container::Style {
        background: Some(background.into()),
        border: iced::border::rounded(2),
        ..Default::default()
    })
    .into()
}

/// Icon for the file at `path`, about `size` px tall, centred in a fixed
/// width so labels after it align.
pub(crate) fn file_icon<'a>(path: &Path, size: f32) -> Element<'a, Message> {
    let line = |which: Icon, color: Color| -> Element<'a, Message> {
        icon(which, size, 1.8, color).into()
    };
    let inner: Element<'a, Message> = match file_kind(path) {
        FileKind::Php => badge("php", PHP, size * 0.72),
        FileKind::Blade => badge("blade", BLADE, size * 0.6),
        FileKind::Test => line(Icon::Tests, TEST),
        FileKind::Migration => line(Icon::Database, PHP),
        FileKind::Package => line(Icon::Package, PACKAGE),
        FileKind::Env => line(Icon::Key, rgb(0xC7A65D)),
        FileKind::Docker => line(Icon::Docker, rgb(0x4D85FF)),
        FileKind::Markdown => row![
            text("M").size(size * 0.8).font(UI_BOLD).color(MARKDOWN),
            icon(Icon::ArrowDown, size * 0.6, 3.0, MARKDOWN)
        ]
        .align_y(Center)
        .into(),
        FileKind::JavaScript => square("JS", JAVASCRIPT, size * 0.9),
        FileKind::TypeScript => square("TS", TYPESCRIPT, size * 0.9),
        FileKind::Json => line(Icon::Braces, NEUTRAL),
        FileKind::Markup => line(Icon::Markup, rgb(0xE6967A)),
        FileKind::Shell => line(Icon::Terminal, NEUTRAL),
        FileKind::Ignore => line(Icon::Ignore, NEUTRAL),
        FileKind::Image => line(Icon::Image, rgb(0x54BFBF)),
        FileKind::Database => line(Icon::Database, rgb(0xC7A65D)),
        FileKind::Text => line(Icon::Lines, NEUTRAL),
    };
    container(inner)
        .width(FILE_ICON_WIDTH)
        .center_x(FILE_ICON_WIDTH)
        .into()
}

/// Role of a folder in a PHP project; all share one shape, tinted per role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FolderRole {
    /// Application source (`app`, `src`).
    Source,
    /// Tests (`tests`).
    Test,
    /// Templates and assets (`resources`, `public`, `lang`).
    Resources,
    /// Dependencies (`vendor`, `node_modules`).
    Library,
    /// Generated or runtime files (`storage`, `bootstrap/cache`).
    Excluded,
    /// Anything else.
    Plain,
}

impl FolderRole {
    /// Tint of the folder icon.
    fn color(self) -> Color {
        match self {
            FolderRole::Source => rgb(0x548AF7),
            FolderRole::Test => TEST,
            FolderRole::Resources => rgb(0xB98BE8),
            FolderRole::Library => rgb(0xC98A5B),
            FolderRole::Excluded => rgb(0x6D6A80),
            FolderRole::Plain => rgb(0x9790AD),
        }
    }
}

/// Role of the folder at `path` in the project at `root`: the first path
/// component (from the root) with a known role decides, so `tests/Unit`
/// is a test folder.
pub(crate) fn folder_role(path: &Path, root: Option<&Path>) -> FolderRole {
    let rel = root.and_then(|r| path.strip_prefix(r).ok()).unwrap_or(path);
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();
    match parts.first().map(String::as_str) {
        Some("app" | "src") => FolderRole::Source,
        Some("tests") => FolderRole::Test,
        Some("resources" | "public" | "lang") => FolderRole::Resources,
        Some("vendor" | "node_modules") => FolderRole::Library,
        Some("storage") => FolderRole::Excluded,
        Some("bootstrap") if parts.get(1).is_some_and(|p| p == "cache") => FolderRole::Excluded,
        _ => FolderRole::Plain,
    }
}

/// Icon for the folder at `path`, open when `expanded`, tinted by role.
pub(crate) fn folder_icon<'a>(
    path: &Path,
    root: Option<&Path>,
    expanded: bool,
    size: f32,
) -> Element<'a, Message> {
    let which = if expanded {
        Icon::FolderOpen
    } else {
        Icon::Folder
    };
    container(icon(which, size, 1.8, folder_role(path, root).color()))
        .width(FILE_ICON_WIDTH)
        .center_x(FILE_ICON_WIDTH)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(path: &str) -> FileKind {
        file_kind(Path::new(path))
    }

    #[test]
    fn file_kind_should_prefer_names_then_compound_suffixes_then_extensions() {
        assert_eq!(kind("app/User.php"), FileKind::Php);
        assert_eq!(kind("artisan"), FileKind::Php);
        assert_eq!(kind("resources/views/welcome.blade.php"), FileKind::Blade);
        assert_eq!(kind("tests/Unit/UserTest.php"), FileKind::Test);
        assert_eq!(kind("phpunit.xml.dist"), FileKind::Test);
        assert_eq!(kind("composer.lock"), FileKind::Package);
        assert_eq!(kind("other.lock"), FileKind::Json);
        assert_eq!(kind(".env.testing"), FileKind::Env);
        assert_eq!(
            kind("database/migrations/2024_01_01_create_users.php"),
            FileKind::Migration
        );
        assert_eq!(kind("Dockerfile"), FileKind::Docker);
        assert_eq!(kind("README.md"), FileKind::Markdown);
        assert_eq!(kind("vite.config.js"), FileKind::JavaScript);
        assert_eq!(kind(".gitignore"), FileKind::Ignore);
        assert_eq!(kind("setup.sh"), FileKind::Shell);
        assert_eq!(kind(".gitattributes"), FileKind::Text);
    }

    #[test]
    fn folder_role_should_follow_the_top_folder() {
        let root = Some(Path::new("/p"));
        assert_eq!(
            folder_role(Path::new("/p/app/Models"), root),
            FolderRole::Source
        );
        assert_eq!(
            folder_role(Path::new("/p/tests/Unit"), root),
            FolderRole::Test
        );
        assert_eq!(
            folder_role(Path::new("/p/vendor"), root),
            FolderRole::Library
        );
        assert_eq!(
            folder_role(Path::new("/p/bootstrap/cache"), root),
            FolderRole::Excluded
        );
        assert_eq!(
            folder_role(Path::new("/p/bootstrap"), root),
            FolderRole::Plain
        );
    }
}
