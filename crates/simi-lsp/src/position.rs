use line_index::{LineCol, LineIndex, TextSize, WideEncoding, WideLineCol};
use lsp_types::{Position, Range};
use simi_analysis::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PositionError {
    Invalid(Position),
    OffsetTooLarge(usize),
}

pub(crate) fn offset(text: &str, position: Position) -> Result<usize, PositionError> {
    let index = LineIndex::new(text);
    let wide = WideLineCol {
        line: position.line,
        col: position.character,
    };
    let utf8 = index
        .to_utf8(WideEncoding::Utf16, wide)
        .ok_or(PositionError::Invalid(position))?;
    let raw = index.offset(utf8).ok_or(PositionError::Invalid(position))?;
    let verified = index
        .try_line_col(raw)
        .filter(|actual| actual.line == position.line && *actual == utf8)
        .ok_or(PositionError::Invalid(position))?;
    let line = index
        .line(position.line)
        .ok_or(PositionError::Invalid(position))?;
    if raw > line.end() || (raw == line.end() && verified.col != u32::from(line.len())) {
        return Err(PositionError::Invalid(position));
    }
    Ok(u32::from(raw) as usize)
}

pub(crate) fn position(text: &str, offset: usize) -> Result<Position, PositionError> {
    let raw = u32::try_from(offset)
        .map(TextSize::from)
        .map_err(|_| PositionError::OffsetTooLarge(offset))?;
    let index = LineIndex::new(text);
    let utf8: LineCol = index
        .try_line_col(raw)
        .ok_or(PositionError::OffsetTooLarge(offset))?;
    let wide = index
        .to_wide(WideEncoding::Utf16, utf8)
        .ok_or(PositionError::OffsetTooLarge(offset))?;
    Ok(Position::new(wide.line, wide.col))
}

pub(crate) fn range(text: &str, span: Span) -> Result<Range, PositionError> {
    Ok(Range::new(
        position(text, span.start)?,
        position(text, span.end)?,
    ))
}

pub(crate) fn apply_changes(
    text: &str,
    changes: &[lsp_types::TextDocumentContentChangeEvent],
) -> Result<String, PositionError> {
    let mut current = text.to_owned();
    for change in changes {
        if let Some(range) = change.range {
            let start = offset(&current, range.start)?;
            let end = offset(&current, range.end)?;
            if start > end {
                return Err(PositionError::Invalid(range.start));
            }
            current.replace_range(start..end, &change.text);
        } else {
            current = change.text.clone();
        }
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_utf16_and_rejects_split_surrogates_and_oversized_columns() {
        let text = "a😀z\n猫";
        assert_eq!(offset(text, Position::new(0, 0)), Ok(0));
        assert_eq!(offset(text, Position::new(0, 1)), Ok(1));
        assert_eq!(offset(text, Position::new(0, 3)), Ok(5));
        assert_eq!(position(text, 5), Ok(Position::new(0, 3)));
        assert!(offset(text, Position::new(0, 2)).is_err());
        assert!(offset(text, Position::new(0, 5)).is_err());
        assert!(offset(text, Position::new(9, 0)).is_err());
    }

    #[test]
    fn applies_ordered_changes_against_each_intermediate_version() {
        let changes = vec![
            lsp_types::TextDocumentContentChangeEvent {
                range: Some(Range::new(Position::new(0, 1), Position::new(0, 3))),
                range_length: None,
                text: "猫".into(),
            },
            lsp_types::TextDocumentContentChangeEvent {
                range: Some(Range::new(Position::new(0, 2), Position::new(0, 3))),
                range_length: None,
                text: "!".into(),
            },
        ];
        assert_eq!(apply_changes("a😀z", &changes), Ok("a猫!".into()));
    }
}
