//! Presentation-only geometry for the desktop editor.
//! The editor width and explicit line height are shared by keyboard navigation
//! and scrolling; nothing is inserted into the Archive.
use carta_app::{App, View};

pub const FONT_SIZE: f32 = 18.0;
pub const LINE_HEIGHT: f32 = 22.0;
pub const PAGE_WIDTH: f32 = 820.0;
pub const HORIZONTAL_PADDING: f32 = 18.0;
pub const VERTICAL_PADDING: f32 = 26.0;
const MONO_ADVANCE: f32 = 9.0; // Iosevka Regular at 18 logical pixels

pub fn columns(window_width: f32) -> usize {
    let usable = (window_width.min(PAGE_WIDTH) - 2.0 * HORIZONTAL_PADDING).max(90.0);
    (usable / MONO_ADVANCE).floor().max(1.0) as usize
}

pub fn writing_area_height(window_height: f32) -> f32 {
    // The status bar is always present; the LEAP query may use one more line.
    (window_height - 42.0).max(100.0)
}

/// Counts the logical visual rows occupied by text preceding a UTF-8 cursor.
/// Tabs and East-Asian-wide graphemes still need renderer-measured handling
/// when Carta adopts a custom text layout engine.
pub fn visual_rows(text: &str, byte: usize, width: usize) -> usize {
    let mut rows = 0;
    let mut column = 0;
    let width = width.max(1);
    let byte = byte.min(text.len());
    let byte = (0..=byte)
        .rev()
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(0);
    for ch in text[..byte].chars() {
        if ch == '\n' {
            rows += 1;
            column = 0;
        } else {
            if column >= width {
                rows += 1;
                column = 0;
            }
            column += 1;
        }
    }
    rows
}

pub fn caret_row(app: &App, width: usize) -> usize {
    let cursor = app.editor.cursor();
    let mut rows = 0;
    for (index, region) in app.editor.regions().iter().enumerate() {
        let locked = app
            .archive
            .document_is_locked(region.document)
            .unwrap_or(false);
        let separator = match &app.view {
            View::CreationDate(_) | View::ModificationDate => true,
            View::Work(_) => index > 0 || locked,
            _ => false,
        };
        if separator {
            if index > 0 {
                rows += 1;
            }
            rows += 3; // separator plus two generated LF characters
        } else if index > 0 {
            rows += 2;
        }
        if index == cursor.region {
            rows += visual_rows(&region.text, cursor.byte, width);
            break;
        }
        rows += visual_rows(&region.text, region.text.len(), width);
    }
    rows
}

pub fn scroll_offset(app: &App, window_width: f32, window_height: f32) -> f32 {
    let _ = window_height; // Padding tracks the viewport height in the widget.
    let row = caret_row(app, columns(window_width)) as f32;
    // The scrollable begins with a virtual top spacer at two-thirds viewport
    // height. Remove the row's physical displacement, leaving that spacer
    // as the writing position on screen (including at document boundaries).
    (VERTICAL_PADDING + row * LINE_HEIGHT).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewport_width_is_bounded() {
        assert!(columns(320.0) > 0);
        assert_eq!(columns(1000.0), columns(1600.0));
    }
    #[test]
    fn wrap_and_newline_rows_are_not_authored_padding() {
        assert_eq!(visual_rows("abcd", 4, 2), 1);
        assert_eq!(visual_rows("abc\ndef", 4, 20), 1);
        assert_eq!(visual_rows("abc\ndef", 7, 20), 1);
        assert_eq!(visual_rows("éé", 2, 50), 0);
    }
}
