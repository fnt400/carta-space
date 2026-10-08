//! Presentation-only geometry for the desktop editor and mouse hit testing.
//! The document model never stores screen coordinates or synthetic padding.
use carta_app::{App, Cursor, View};

pub const FONT_SIZE: f32 = 18.0;
pub const MIN_FONT_SIZE: f32 = 12.0;
pub const MAX_FONT_SIZE: f32 = 32.0;
pub const FONT_STEP: f32 = 2.0;
pub const PAGE_WIDTH: f32 = 820.0;
pub const HORIZONTAL_PADDING: f32 = 18.0;
pub const VERTICAL_PADDING: f32 = 26.0;

pub fn line_height(font_size: f32) -> f32 {
    font_size * (22.0 / FONT_SIZE)
}

pub fn mono_advance(font_size: f32) -> f32 {
    font_size * 0.5 // Iosevka Regular: same approximation as the existing GUI.
}

pub fn columns(window_width: f32, font_size: f32) -> usize {
    let usable = (window_width.min(PAGE_WIDTH) - 2.0 * HORIZONTAL_PADDING).max(90.0);
    (usable / mono_advance(font_size)).floor().max(1.0) as usize
}

pub fn writing_area_height(window_height: f32) -> f32 {
    (window_height - 42.0).max(100.0)
}

/// Counts the visual rows before a byte offset using the GUI's monospace
/// approximation. Tabs and multi-cell Unicode require renderer hit testing
/// when a precise shaping API becomes available.
pub fn visual_rows(text: &str, byte: usize, width: usize) -> usize {
    let mut rows = 0;
    let mut column = 0;
    let width = width.max(1);
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) { byte -= 1; }
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

fn prefix_rows(view: &View, region_index: usize, locked: bool) -> usize {
    let has_separator = match view {
        View::CreationDate(_) | View::ModificationDate => true,
        View::Work(_) => region_index > 0 || locked,
        _ => false,
    };
    if has_separator {
        // First separator is "rule\n\n" (2 rows); subsequent ones have an
        // additional preceding newline (3 rows).
        2 + usize::from(region_index > 0)
    } else if region_index > 0 {
        2
    } else {
        0
    }
}

pub fn caret_row(app: &App, width: usize) -> usize {
    let cursor = app.editor.cursor();
    let mut rows = 0;
    for (index, region) in app.editor.regions().iter().enumerate() {
        let locked = app.archive.document_is_locked(region.document).unwrap_or(false);
        rows += prefix_rows(&app.view, index, locked);
        if index == cursor.region {
            rows += visual_rows(&region.text, cursor.byte, width);
            break;
        }
        rows += visual_rows(&region.text, region.text.len(), width);
    }
    rows
}

pub fn scroll_offset(app: &App, window_width: f32, font_size: f32) -> f32 {
    let row = caret_row(app, columns(window_width, font_size)) as f32;
    (VERTICAL_PADDING + row * line_height(font_size)).max(0.0)
}

/// Compute a UTF-8 insertion point from a visual row/column, rounding to the
/// nearest character cell. This is intentionally independent of the archive.
fn byte_at_cell(text: &str, target_row: usize, target_col: usize, width: usize) -> usize {
    let width = width.max(1);
    let mut row = 0;
    let mut col = 0;
    for (byte, ch) in text.char_indices() {
        if ch == '\n' {
            if row == target_row { return byte; }
            row += 1;
            col = 0;
            continue;
        }
        if col >= width {
            row += 1;
            col = 0;
        }
        if row > target_row || (row == target_row && col >= target_col) {
            return byte;
        }
        col += 1;
    }
    text.len()
}

/// `point` is relative to the centered full-width page row, at the beginning
/// of the actual text sheet (not including the top virtual scroll spacer).
pub fn hit_test(app: &App, point: iced::Point, window_width: f32, font_size: f32) -> Option<Cursor> {
    if app.editor.regions().is_empty() { return None; }
    let width = columns(window_width, font_size);
    let page_width = window_width.min(PAGE_WIDTH);
    let left = ((window_width - page_width) / 2.0) + HORIZONTAL_PADDING;
    let x = (point.x - left).max(0.0);
    let y = (point.y - VERTICAL_PADDING).max(0.0);
    let col = (x / mono_advance(font_size)).round().max(0.0) as usize;
    let mut row = (y / line_height(font_size)).floor().max(0.0) as usize;
    let regions = app.editor.regions();
    for (index, region) in regions.iter().enumerate() {
        let locked = app.archive.document_is_locked(region.document).unwrap_or(false);
        let prefix = prefix_rows(&app.view, index, locked);
        if row < prefix {
            return Some(Cursor { region: index, byte: 0 });
        }
        row -= prefix;
        let last_row = visual_rows(&region.text, region.text.len(), width);
        if row <= last_row || index + 1 == regions.len() {
            return Some(Cursor {
                region: index,
                byte: byte_at_cell(&region.text, row, col, width),
            });
        }
        row -= last_row;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewport_scales_with_font_size() {
        assert!(columns(320.0, FONT_SIZE) > 0);
        assert_eq!(columns(1000.0, FONT_SIZE), columns(1600.0, FONT_SIZE));
        assert!(columns(800.0, 24.0) < columns(800.0, 16.0));
        assert!(line_height(24.0) > line_height(16.0));
    }
    #[test]
    fn wrapped_rows_and_utf8_hit_testing() {
        assert_eq!(visual_rows("abcd", 4, 2), 1);
        assert_eq!(visual_rows("abc\ndef", 4, 20), 1);
        assert_eq!(visual_rows("abc\ndef", 7, 20), 1);
        assert_eq!(visual_rows("éé", 2, 50), 0);
        assert_eq!(byte_at_cell("école\ntext", 0, 1, 20), 2);
        assert_eq!(byte_at_cell("école\ntext", 0, 99, 20), 6);
        assert_eq!(byte_at_cell("abcdef", 1, 1, 3), 4);
    }
}
