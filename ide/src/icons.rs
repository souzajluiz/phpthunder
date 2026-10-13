//! Stroke icons, tinted at draw time.

use iced::widget::{Svg, svg};
use iced::{Color, Length};

/// Logo bolt: title bar, welcome screen and status bar.
const BOLT: &str = r#"<path d="M13 2 4 14h7l-1 8 9-12h-7l1-8z" stroke-linejoin="round"/>"#;
/// Project tool in the rail.
const FOLDER: &str =
    r#"<path d="M4 5a1 1 0 0 1 1-1h5l2 2h7a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1z"/>"#;
/// Search tool in the rail.
const SEARCH: &str = r#"<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>"#;
/// Symbols tool in the rail.
const SYMBOLS: &str = r#"<path d="M8 4 4 8l4 4M16 12l4 4-4 4M14 4l-4 16"/>"#;
/// Git branch: rail and title bar.
const BRANCH: &str = r#"<circle cx="6" cy="6" r="2"/><circle cx="6" cy="18" r="2"/><circle cx="18" cy="8" r="2"/><path d="M6 8v8M18 10c0 4-6 3-12 6"/>"#;
/// Terminal tool in the rail.
const TERMINAL: &str = r#"<path d="m5 8 4 4-4 4M12 16h7"/>"#;
/// Collapsed directory in the tree; hidden replace row in the find bar.
const CHEVRON_RIGHT: &str = r#"<path d="m9 6 6 6-6 6"/>"#;
/// Expanded directory in the tree; shown replace row in the find bar.
const CHEVRON_DOWN: &str = r#"<path d="m6 9 6 6 6-6"/>"#;
/// Close buttons on tabs, the bottom panel and the find bar.
const CLOSE: &str = r#"<path d="M6 6l12 12M18 6 6 18"/>"#;

/// Which stroke icon to draw; each variant maps to one SVG body above.
#[derive(Clone, Copy)]
pub enum Icon {
    /// Logo bolt.
    Bolt,
    /// Project folder.
    Folder,
    /// Magnifying glass.
    Search,
    /// Code brackets for symbols.
    Symbols,
    /// Git branch.
    Branch,
    /// Terminal prompt.
    Terminal,
    /// Right-pointing chevron (collapsed).
    ChevronRight,
    /// Down-pointing chevron (expanded).
    ChevronDown,
    /// Cross for close buttons.
    Close,
}

impl Icon {
    /// The SVG path markup for this icon, without the `<svg>` wrapper that
    /// `icon` adds.
    fn body(self) -> &'static str {
        match self {
            Icon::Bolt => BOLT,
            Icon::Folder => FOLDER,
            Icon::Search => SEARCH,
            Icon::Symbols => SYMBOLS,
            Icon::Branch => BRANCH,
            Icon::Terminal => TERMINAL,
            Icon::ChevronRight => CHEVRON_RIGHT,
            Icon::ChevronDown => CHEVRON_DOWN,
            Icon::Close => CLOSE,
        }
    }
}

/// Builds an SVG widget for `which`, `size` px square with the given stroke
/// width, tinted with `color`. Called throughout view.rs. The markup is drawn
/// with a white stroke and recolored by the svg style, so one path serves
/// every tint; each call allocates a fresh SVG handle.
pub fn icon<'a>(which: Icon, size: f32, stroke: f32, color: Color) -> Svg<'a> {
    let source = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="{stroke}" stroke-linecap="round">{}</svg>"#,
        which.body()
    );
    svg(svg::Handle::from_memory(source.into_bytes()))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style { color: Some(color) })
}
