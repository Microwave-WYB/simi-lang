/// An end-exclusive range of UTF-8 byte offsets in source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Return the smallest span containing both spans.
    pub fn merge(self, other: Span) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

/// Convert a UTF-8 byte offset to a one-based line and character column.
///
/// Offsets beyond the end of `source` are treated as the end of the source.
pub fn line_column(source: &str, byte_offset: usize) -> (usize, usize) {
    let offset = byte_offset.min(source.len());
    let mut line = 1;
    let mut column = 1;

    for (index, character) in source.char_indices() {
        if index >= offset {
            break;
        }

        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_spans_in_either_order() {
        assert_eq!(Span::new(4, 8).merge(Span::new(1, 6)), Span::new(1, 8));
    }

    #[test]
    fn reports_one_based_character_positions_from_byte_offsets() {
        let source = "éx\n猫z";
        assert_eq!(line_column(source, 0), (1, 1));
        assert_eq!(line_column(source, 2), (1, 2));
        assert_eq!(line_column(source, 4), (2, 1));
        assert_eq!(line_column(source, 7), (2, 2));
        assert_eq!(line_column(source, source.len()), (2, 3));
    }
}
