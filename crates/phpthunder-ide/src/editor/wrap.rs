//! Soft wrap geometry: which visual row each line and column lands on when
//! long lines wrap at the editor's width. With a monospace font and
//! per-character wrapping, a line of `n` columns takes `ceil(n / cols)`
//! rows, so every overlay (caret band, marks, popups, gutter) can be
//! placed exactly. Without wrapping each line is one row.

/// Row layout of a document for one wrap width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapLayout {
    /// Columns per row; `usize::MAX` when wrapping is off.
    cols: usize,
    /// First row of each line, plus the total row count at the end.
    starts: Vec<usize>,
}

impl WrapLayout {
    /// The layout of lines with the given visual widths (in columns),
    /// wrapping at `cols` columns (`None`: no wrapping).
    pub fn new(widths: impl IntoIterator<Item = usize>, cols: Option<usize>) -> Self {
        let cols = cols.map_or(usize::MAX, |c| c.max(1));
        let mut starts = vec![0];
        let mut row = 0;
        for width in widths {
            row += if cols == usize::MAX {
                1
            } else {
                width.div_ceil(cols).max(1)
            };
            starts.push(row);
        }
        Self { cols, starts }
    }

    /// Whether long lines wrap.
    pub fn wraps(&self) -> bool {
        self.cols != usize::MAX
    }

    /// Visual row of `col` on `line`.
    pub fn row(&self, line: usize, col: usize) -> usize {
        let base = self
            .starts
            .get(line)
            .copied()
            .unwrap_or_else(|| self.total_rows());
        let max_in_line = self.rows_of(line).saturating_sub(1);
        base + if self.wraps() {
            (col / self.cols).min(max_in_line)
        } else {
            0
        }
    }

    /// Column of `col` within its row (its x position in columns).
    pub fn column_in_row(&self, col: usize) -> usize {
        if self.wraps() { col % self.cols } else { col }
    }

    /// Number of rows `line` takes.
    pub fn rows_of(&self, line: usize) -> usize {
        match (self.starts.get(line), self.starts.get(line + 1)) {
            (Some(a), Some(b)) => b - a,
            _ => 1,
        }
    }

    /// Total rows of the document.
    pub fn total_rows(&self) -> usize {
        self.starts.last().copied().unwrap_or(0)
    }

    /// Line and first column shown on visual `row`.
    pub fn line_at_row(&self, row: usize) -> (usize, usize) {
        let line = self.starts.partition_point(|&s| s <= row).saturating_sub(1);
        let line = line.min(self.starts.len().saturating_sub(2));
        let within = row.saturating_sub(self.starts[line]);
        (line, if self.wraps() { within * self.cols } else { 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_should_map_lines_and_columns_to_rows() {
        let layout = WrapLayout::new([5, 25, 0, 10], Some(10));
        assert_eq!(layout.total_rows(), 6);
        assert_eq!(layout.row(1, 0), 1);
        assert_eq!(layout.row(1, 12), 2);
        assert_eq!(layout.row(1, 25), 3);
        assert_eq!(layout.column_in_row(12), 2);
        assert_eq!(layout.row(2, 0), 4);
        assert_eq!(layout.line_at_row(3), (1, 20));
        assert_eq!(layout.line_at_row(5), (3, 0));
    }

    #[test]
    fn layout_without_wrapping_should_be_one_row_per_line() {
        let layout = WrapLayout::new([500, 3], None);
        assert_eq!(
            (layout.row(0, 400), layout.row(1, 2), layout.total_rows()),
            (0, 1, 2)
        );
        assert_eq!(layout.column_in_row(400), 400);
    }
}
