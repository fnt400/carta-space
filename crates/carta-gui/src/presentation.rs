//! A presentation-only projection of the canonical editor state.
//! Byte offsets are converted to spans without changing document contents.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    Text,
    Selection,
    Caret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment<'a> {
    pub content: &'a str,
    pub kind: SegmentKind,
}

/// Splits a UTF-8 document into text, selection and caret spans.
///
/// The caret is a zero-width position in the application model; the GUI
/// paints the character at that position without inserting a glyph into the text. The returned text spans reconstruct
/// the original document byte-for-byte, including embedded newlines.
pub fn segments(text: &str, cursor: usize, selection: Option<(usize, usize)>) -> Vec<Segment<'_>> {
    let cursor = boundary_at_or_before(text, cursor);
    let selection = selection.and_then(|(start, end)| {
        if start < end
            && end <= text.len()
            && text.is_char_boundary(start)
            && text.is_char_boundary(end)
        {
            Some((start, end))
        } else {
            None
        }
    });

    // Paint the character at the insertion point instead of inserting a cursor
    // glyph into the text flow (which would move all subsequent characters).
    let caret_end = text[cursor..]
        .chars()
        .next()
        .map_or(cursor, |ch| cursor + ch.len_utf8());
    let mut boundaries = vec![0, cursor, caret_end, text.len()];
    if let Some((start, end)) = selection {
        boundaries.extend([start, end]);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut result = Vec::with_capacity(boundaries.len() * 2);
    for pair in boundaries.windows(2) {
        let start = pair[0];
        let end = pair[1];
        result.push(Segment {
            content: &text[start..end],
            kind: if start == cursor {
                SegmentKind::Caret
            } else if selection.is_some_and(|(a, b)| start >= a && end <= b) {
                SegmentKind::Selection
            } else {
                SegmentKind::Text
            },
        });
    }
    if cursor == text.len() {
        result.push(Segment {
            content: "",
            kind: SegmentKind::Caret,
        });
    }
    result
}

fn boundary_at_or_before(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_document_still_has_caret() {
        assert_eq!(
            segments("", 0, None),
            vec![Segment {
                content: "",
                kind: SegmentKind::Caret
            }]
        );
    }

    #[test]
    fn multibyte_selection_preserves_text_and_caret() {
        let text = "caffè\n東京";
        let spans = segments(text, 6, Some((4, 6)));
        assert_eq!(
            spans
                .iter()
                .filter(|s| s.kind == SegmentKind::Selection)
                .map(|s| s.content)
                .collect::<String>(),
            "è"
        );
        assert_eq!(
            spans
                .iter()
                .filter(|s| s.kind != SegmentKind::Caret)
                .map(|s| s.content)
                .collect::<String>(),
            text
        );
        assert_eq!(
            spans
                .iter()
                .filter(|s| s.kind == SegmentKind::Caret)
                .count(),
            1
        );
    }

    #[test]
    fn caret_uses_original_character_without_shifting_following_text() {
        let original = "abè文z";
        let result = segments(original, 2, None);
        assert_eq!(result.iter().filter(|s| s.kind == SegmentKind::Caret).count(), 1);
        assert_eq!(result.iter().find(|s| s.kind == SegmentKind::Caret).unwrap().content, "è");
        assert_eq!(result.iter().map(|s| s.content).collect::<String>(), original);
    }

    #[test]
    fn caret_at_end_and_invalid_selection() {
        let spans = segments("abc", 3, Some((2, 10)));
        assert_eq!(spans.last().unwrap().kind, SegmentKind::Caret);
        assert!(!spans.iter().any(|s| s.kind == SegmentKind::Selection));
    }

    #[test]
    fn non_boundary_caret_is_clamped_to_character_start() {
        let spans = segments("éx", 1, None);
        assert_eq!(spans.first().unwrap().kind, SegmentKind::Caret);
    }
}
