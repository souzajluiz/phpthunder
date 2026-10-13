//! Where text sits in the editor: the soft wrap layout of the active
//! document and the pixel position of a (line, column), shared by the view
//! (caret band, marks, popups, gutter) and the update logic (scrolling,
//! mouse hit-testing).

use super::*;
use crate::editor::{Document, WrapLayout};
use crate::ui::{CHAR_WIDTH, CODE_LINE, EDITOR_PAD_LEFT, EDITOR_PAD_TOP, GUTTER_WIDTH};

/// Right padding inside the editor, matching the text editor widget.
pub(crate) const EDITOR_PAD_RIGHT: f32 = 24.0;

/// Top of the visual row holding `col` of `line`, in content pixels.
pub(crate) fn row_top(layout: &WrapLayout, line: usize, col: usize) -> f32 {
    EDITOR_PAD_TOP + layout.row(line, col) as f32 * CODE_LINE
}

/// Left edge of `col` within its row, in editor pixels (gutter included).
pub(crate) fn col_left(layout: &WrapLayout, col: usize) -> f32 {
    GUTTER_WIDTH + EDITOR_PAD_LEFT + layout.column_in_row(col) as f32 * CHAR_WIDTH
}

impl App {
    /// Columns that fit in one editor row at the current width.
    pub(crate) fn wrap_columns(&self) -> usize {
        ((self.editor_width - GUTTER_WIDTH - EDITOR_PAD_LEFT - EDITOR_PAD_RIGHT) / CHAR_WIDTH)
            .floor()
            .max(20.0) as usize
    }

    /// The row layout of `doc`: one row per line, or wrapped at the editor
    /// width when soft wrap is on. Reads the line widths in one pass over
    /// the text: `line_text(i)` per line rescans from the start, which made
    /// every frame quadratic and froze the window on large files.
    pub(crate) fn wrap_layout(&self, doc: &Document) -> WrapLayout {
        let widths = doc
            .text
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).chars().count())
            .chain(std::iter::repeat(0))
            .take(doc.line_count());
        WrapLayout::new(widths, self.soft_wrap.then(|| self.wrap_columns()))
    }

    /// First and last line (inclusive) visible in the editor viewport.
    pub(crate) fn visible_lines(&self, doc: &Document, layout: &WrapLayout) -> (usize, usize) {
        let first_row = ((doc.scroll_y - EDITOR_PAD_TOP) / CODE_LINE)
            .floor()
            .max(0.0) as usize;
        let last_row = first_row + (self.viewport_height / CODE_LINE).ceil() as usize + 1;
        (
            layout.line_at_row(first_row).0,
            layout.line_at_row(last_row).0,
        )
    }
}
