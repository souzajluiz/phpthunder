//! Byte offset <-> LSP position conversion.
//!
//! LSP positions are (line, UTF-16 code unit column); tree-sitter works in bytes.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}

pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(src: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        Self { starts }
    }

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

    pub fn span(&self, src: &str, start: usize, end: usize) -> Span {
        Span {
            start: self.pos(src, start),
            end: self.pos(src, end),
        }
    }

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
