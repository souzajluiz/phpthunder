//! Byte offset <-> LSP position conversion.
//!
//! LSP positions are (line, UTF-16 code unit column); tree-sitter works in bytes.

/// A position in LSP coordinates.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Pos {
    /// Zero-based line.
    pub line: u32,
    /// Zero-based column in UTF-16 code units.
    pub col: u32,
}

/// A range between two positions, end exclusive.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Span {
    /// First position covered.
    pub start: Pos,
    /// Position just past the end.
    pub end: Pos,
}

/// Line start offsets of one source text, for converting between byte offsets
/// and [`Pos`]. Build one per text version; it is stale once the text changes.
pub struct LineIndex {
    /// Byte offset where each line starts; the first entry is 0.
    starts: Vec<usize>,
}

impl LineIndex {
    /// Records the line starts of `src` (only `\n` ends a line). Linear in the text length.
    pub fn new(src: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        Self { starts }
    }

    /// Position of byte offset `byte`. Offsets past the end clamp to the end; an
    /// offset inside a multi-byte character counts bytes for the column.
    pub fn pos(&self, src: &str, byte: usize) -> Pos {
        let byte = byte.min(src.len());
        let line = self.starts.partition_point(|&s| s <= byte) - 1;
        let start = self.starts[line];
        let col = src
            .get(start..byte)
            .map(|s| s.encode_utf16().count())
            .unwrap_or(byte - start);
        Pos {
            line: line as u32,
            col: col as u32,
        }
    }

    /// Span between two byte offsets; see [`LineIndex::pos`].
    pub fn span(&self, src: &str, start: usize, end: usize) -> Span {
        Span {
            start: self.pos(src, start),
            end: self.pos(src, end),
        }
    }

    /// Byte offset of `pos`, the inverse of [`LineIndex::pos`]. A line past the end
    /// maps to `src.len()`; a column past the line end clamps to just after that
    /// line's newline.
    pub fn offset(&self, src: &str, pos: Pos) -> usize {
        let Some(&start) = self.starts.get(pos.line as usize) else {
            return src.len();
        };
        let end = self
            .starts
            .get(pos.line as usize + 1)
            .copied()
            .unwrap_or(src.len());
        let mut units = 0u32;
        for (i, ch) in src[start..end].char_indices() {
            if units >= pos.col {
                return start + i;
            }
            units += ch.len_utf16() as u32;
        }
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_multibyte() {
        let src = "<?php\n$é = 'ü';\nfoo();";
        let li = LineIndex::new(src);
        let byte = src.find("foo").unwrap();
        let pos = li.pos(src, byte);
        assert_eq!(pos, Pos { line: 2, col: 0 });
        assert_eq!(li.offset(src, pos), byte);
        let q = src.find('\'').unwrap();
        assert_eq!(li.pos(src, q), Pos { line: 1, col: 5 });
        assert_eq!(li.offset(src, Pos { line: 1, col: 5 }), q);
    }
}
