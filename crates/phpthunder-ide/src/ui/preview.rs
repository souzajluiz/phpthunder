//! Tabs that show a file instead of editing it: images are drawn scaled to
//! fit, other binary files get a short note. Opened by clicking them in the
//! project tree like any other file.

use super::*;
use crate::app::{App, Message};
use crate::editor::Document;
use crate::filetype::FileType;
use crate::tr;
use iced::widget::{column, container, image, scrollable, svg};
use iced::{Center, ContentFit, Element, Fill};
use std::path::Path;

/// Size facts about a previewed file, read once when it opens.
#[derive(Debug, Clone, Copy, Default)]
pub struct Preview {
    /// Pixel size, for images whose header could be read.
    pub dimensions: Option<(u32, u32)>,
    /// File size in bytes.
    pub bytes: u64,
}

impl Preview {
    /// Reads the file size and, for images, the pixel size from the header
    /// (the pixels themselves are decoded by the renderer). Unreadable
    /// files give an empty preview rather than an error.
    pub fn read(path: &Path, file_type: FileType) -> Self {
        Self {
            dimensions: (file_type == FileType::Image)
                .then(|| ::image::image_dimensions(path).ok())
                .flatten(),
            bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        }
    }
}

/// `#rrggbb` for an opaque theme color.
fn hex(color: iced::Color) -> String {
    let [r, g, b, _] = color.into_rgba8();
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Colors of class diagrams drawn for the preview, taken from the editor
/// theme so a diagram tab looks like the rest of the window.
pub fn diagram_style() -> phpthunder::diagram::Style {
    phpthunder::diagram::Style {
        background: hex(BG),
        fill: hex(POPUP),
        focus_fill: hex(SELECTED),
        line: hex(MUTED),
        text: hex(TEXT),
        muted: hex(MUTED),
        accent: hex(ACCENT),
        font: "Menlo, Consolas, 'DejaVu Sans Mono', monospace".into(),
    }
}

/// `1536` -> `1.5 KB`.
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

impl App {
    /// The tab body for a document that is shown rather than edited: an
    /// image preview or a binary-file note. `None` for text files, which get
    /// the code editor.
    pub(crate) fn file_preview<'a>(&'a self, doc: &'a Document) -> Option<Element<'a, Message>> {
        let info = self.previews.get(&doc.path).copied().unwrap_or_default();
        let size = human_size(info.bytes);
        match doc.file_type {
            FileType::Image => {
                let is_svg = doc
                    .path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
                let picture: Element<'a, Message> = if is_svg {
                    svg(svg::Handle::from_path(&doc.path))
                        .content_fit(ContentFit::ScaleDown)
                        .width(Fill)
                        .height(Fill)
                        .into()
                } else {
                    image(image::Handle::from_path(&doc.path))
                        .content_fit(ContentFit::ScaleDown)
                        .width(Fill)
                        .height(Fill)
                        .into()
                };
                let caption = match info.dimensions {
                    Some((w, h)) => format!("{w} × {h} px · {size}"),
                    None => size,
                };
                Some(
                    column![
                        container(picture)
                            .padding(24)
                            .width(Fill)
                            .height(Fill)
                            .center(Fill),
                        container(label(caption, 12.0, MUTED))
                            .padding(8)
                            .center_x(Fill),
                    ]
                    .into(),
                )
            }
            FileType::Binary => Some(
                scrollable(
                    container(label(tr!("preview-binary", size = size), 13.0, MUTED))
                        .padding(48)
                        .center_x(Fill),
                )
                .height(Fill)
                .into(),
            ),
            _ => None,
        }
        .map(|body: Element<'a, Message>| {
            container(body)
                .width(Fill)
                .height(Fill)
                .align_y(Center)
                .into()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_should_pick_a_readable_unit() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(3 * 1024 * 1024), "3.0 MB");
    }
}
