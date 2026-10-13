//! Lucide stroke icons (ISC license, `assets/icons/LICENSE`), embedded in
//! the binary and tinted at draw time, plus the brand artwork from
//! `assets/brand` (PhpThunder's and the Claude symbol), drawn in its own
//! colors.

use iced::widget::{Svg, svg};
use iced::{Color, Length};

/// Which icon to draw; each variant maps to one Lucide SVG in `assets/icons`.
#[derive(Clone, Copy)]
pub enum Icon {
    /// Bolt (`zap`): indexing status in the status bar.
    Bolt,
    /// Project tool in the rail.
    Folder,
    /// Search tool in the rail.
    Search,
    /// Symbols tool in the rail (`code`).
    Symbols,
    /// Git branch: rail and title bar.
    Branch,
    /// Terminal tool in the rail (`square-terminal`).
    Terminal,
    /// Move a tool panel to the right side (`panel-right`).
    PanelRight,
    /// Move a tool panel below the editor (`panel-bottom`).
    PanelBottom,
    /// Collapsed directory in the tree; hidden replace row in the find bar.
    ChevronRight,
    /// Expanded directory in the tree; shown replace row in the find bar.
    ChevronDown,
    /// Close buttons on tabs, the bottom panel and the find bar (`x`).
    Close,
    /// Error marker (`circle-x`).
    Error,
    /// Warning marker (`triangle-alert`).
    Warning,
    /// Type hierarchy (`network`).
    Hierarchy,
    /// Tests tool in the rail (`flask-conical`).
    Tests,
    /// Refresh the Git status (`refresh-cw`).
    Refresh,
    /// Git pull (`arrow-down-to-line`).
    Pull,
    /// Git push (`arrow-up-from-line`).
    Push,
    /// Discard a file's changes (`undo-2`).
    Undo,
    /// Staged checkbox (`check`).
    Check,
    /// Commit tool in the rail (`git-commit-horizontal`).
    Commit,
    /// Debugger: debug a test (`bug`).
    Bug,
    /// Debugger: resume (`play`).
    Resume,
    /// Debugger: step over (`redo-dot`).
    StepOver,
    /// Debugger: step into (`arrow-down-to-dot`).
    StepInto,
    /// Debugger: step out (`arrow-up-from-dot`).
    StepOut,
    /// Debugger: stop (`square`).
    Stop,
    /// JSON and lock files (`braces`).
    Braces,
    /// XML and HTML files (`code-xml`).
    Markup,
    /// Plain text and config files (`text-align-start`).
    Lines,
    /// Ignore files (`ban`).
    Ignore,
    /// Expanded folder (`folder-open`).
    FolderOpen,
    /// Image files (`image`).
    Image,
    /// Markdown arrow and dropdowns (`arrow-down`).
    ArrowDown,
    /// SQL and database files, migrations (`database`).
    Database,
    /// Environment files (`key-round`).
    Key,
    /// Docker files (`container`).
    Docker,
    /// Package manifests: Composer, npm (`package`).
    Package,
    /// New file or folder in the project tree header (`plus`).
    Plus,
    /// Bookmarked line in the gutter (`bookmark`).
    Bookmark,
    /// Markdown preview only (`eye`).
    Eye,
    /// Markdown source beside its preview (`columns-2`).
    Columns,
}

impl Icon {
    /// The Lucide SVG source for this icon.
    fn source(self) -> &'static str {
        match self {
            Icon::Bolt => include_str!("../../assets/icons/zap.svg"),
            Icon::Folder => include_str!("../../assets/icons/folder.svg"),
            Icon::Search => include_str!("../../assets/icons/search.svg"),
            Icon::Symbols => include_str!("../../assets/icons/code.svg"),
            Icon::Branch => include_str!("../../assets/icons/git-branch.svg"),
            Icon::Terminal => include_str!("../../assets/icons/square-terminal.svg"),
            Icon::ChevronRight => include_str!("../../assets/icons/chevron-right.svg"),
            Icon::ChevronDown => include_str!("../../assets/icons/chevron-down.svg"),
            Icon::Close => include_str!("../../assets/icons/x.svg"),
            Icon::Error => include_str!("../../assets/icons/circle-x.svg"),
            Icon::Warning => include_str!("../../assets/icons/triangle-alert.svg"),
            Icon::Hierarchy => include_str!("../../assets/icons/network.svg"),
            Icon::Tests => include_str!("../../assets/icons/flask-conical.svg"),
            Icon::Refresh => include_str!("../../assets/icons/refresh-cw.svg"),
            Icon::Pull => include_str!("../../assets/icons/arrow-down-to-line.svg"),
            Icon::Push => include_str!("../../assets/icons/arrow-up-from-line.svg"),
            Icon::Undo => include_str!("../../assets/icons/undo-2.svg"),
            Icon::Check => include_str!("../../assets/icons/check.svg"),
            Icon::Commit => include_str!("../../assets/icons/git-commit-horizontal.svg"),
            Icon::Bug => include_str!("../../assets/icons/bug.svg"),
            Icon::Resume => include_str!("../../assets/icons/play.svg"),
            Icon::StepOver => include_str!("../../assets/icons/redo-dot.svg"),
            Icon::StepInto => include_str!("../../assets/icons/arrow-down-to-dot.svg"),
            Icon::StepOut => include_str!("../../assets/icons/arrow-up-from-dot.svg"),
            Icon::Stop => include_str!("../../assets/icons/square.svg"),
            Icon::Braces => include_str!("../../assets/icons/braces.svg"),
            Icon::Markup => include_str!("../../assets/icons/code-xml.svg"),
            Icon::Lines => include_str!("../../assets/icons/text-align-start.svg"),
            Icon::Ignore => include_str!("../../assets/icons/ban.svg"),
            Icon::FolderOpen => include_str!("../../assets/icons/folder-open.svg"),
            Icon::Image => include_str!("../../assets/icons/image.svg"),
            Icon::ArrowDown => include_str!("../../assets/icons/arrow-down.svg"),
            Icon::Database => include_str!("../../assets/icons/database.svg"),
            Icon::Key => include_str!("../../assets/icons/key-round.svg"),
            Icon::Docker => include_str!("../../assets/icons/container.svg"),
            Icon::Package => include_str!("../../assets/icons/package.svg"),
            Icon::PanelRight => include_str!("../../assets/icons/panel-right.svg"),
            Icon::PanelBottom => include_str!("../../assets/icons/panel-bottom.svg"),
            Icon::Plus => include_str!("../../assets/icons/plus.svg"),
            Icon::Eye => include_str!("../../assets/icons/eye.svg"),
            Icon::Bookmark => include_str!("../../assets/icons/bookmark.svg"),
            Icon::Columns => include_str!("../../assets/icons/columns-2.svg"),
        }
    }
}

/// Which brand artwork to draw with `logo`; all live in `assets/brand`.
#[derive(Clone, Copy)]
pub enum Logo {
    /// Spider mark alone (`logo-mark.svg`, 1:1): bottom of the tool rail.
    Mark,
    /// The app tile, spider over the PhpThunder wordmark (`logo.svg`, 1:1):
    /// welcome screen.
    Lockup,
    /// The Claude symbol (`claude.svg`, 1:1, Claude's orange): the Claude
    /// panel button in the rail and its status in the status bar.
    Claude,
}

impl Logo {
    /// The SVG source for this artwork.
    fn source(self) -> &'static [u8] {
        match self {
            Logo::Mark => include_bytes!("../../assets/brand/logo-mark.svg"),
            Logo::Lockup => include_bytes!("../../assets/brand/logo.svg"),
            Logo::Claude => include_bytes!("../../assets/brand/claude.svg"),
        }
    }

    /// Height divided by width of the artwork's viewBox.
    fn aspect(self) -> f32 {
        match self {
            Logo::Mark => 1.0,
            Logo::Lockup => 1.0,
            Logo::Claude => 1.0,
        }
    }
}

/// Builds an SVG widget for the brand artwork `which`, `width` px wide with
/// the height following its aspect ratio. Unlike `icon` it keeps its own
/// colors. Used by the title bar, the welcome screen, the rail and the
/// status bar.
pub fn logo<'a>(which: Logo, width: f32) -> Svg<'a> {
    svg(svg::Handle::from_memory(which.source()))
        .width(Length::Fixed(width))
        .height(Length::Fixed(width * which.aspect()))
}

/// Builds an SVG widget for `which`, `size` px square with the given stroke
/// width, tinted with `color`. Called throughout the UI.
///
/// Lucide draws with `currentColor`, which the SVG renderer cannot resolve,
/// so the stroke is set to white and the svg style recolors it; one source
/// serves every tint.
pub fn icon<'a>(which: Icon, size: f32, stroke: f32, color: Color) -> Svg<'a> {
    let source = which.source().replace("currentColor", "white").replace(
        r#"stroke-width="2""#,
        &format!(r#"stroke-width="{stroke}""#),
    );
    svg(svg::Handle::from_memory(source.into_bytes()))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style { color: Some(color) })
}
