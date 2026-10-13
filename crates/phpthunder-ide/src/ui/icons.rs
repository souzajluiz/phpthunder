//! Lucide stroke icons (ISC license, `assets/icons/LICENSE`), embedded in
//! the binary and tinted at draw time, plus the PhpThunder brand artwork
//! from `assets/brand`, drawn in its own colors.

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
        }
    }
}

/// Which brand artwork to draw with `logo`; both live in `assets/brand`.
#[derive(Clone, Copy)]
pub enum Logo {
    /// Spider mark alone (`logo-mark.svg`, 1:1): title bar.
    Mark,
    /// Spider over the PhpThunder wordmark (`logo.svg`, 100:116): welcome screen.
    Lockup,
}

impl Logo {
    /// The SVG source for this artwork.
    fn source(self) -> &'static [u8] {
        match self {
            Logo::Mark => include_bytes!("../../assets/brand/logo-mark.svg"),
            Logo::Lockup => include_bytes!("../../assets/brand/logo.svg"),
        }
    }

    /// Height divided by width of the artwork's viewBox.
    fn aspect(self) -> f32 {
        match self {
            Logo::Mark => 1.0,
            Logo::Lockup => 1.16,
        }
    }
}

/// Builds an SVG widget for the brand artwork `which`, `width` px wide with
/// the height following its aspect ratio. Unlike `icon` it keeps its own
/// colors. Used by the title bar and the welcome screen.
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
