//! GUI text metrics shared by the virtual layout, mouse and keyboard.
//! Authored text is never padded or copied for positioning.
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
    font_size * 0.5
}

pub fn columns(window_width: f32, font_size: f32) -> usize {
    let usable = (window_width.min(PAGE_WIDTH) - 2.0 * HORIZONTAL_PADDING).max(90.0);
    (usable / mono_advance(font_size)).floor().max(1.0) as usize
}

pub fn writing_area_height(window_height: f32) -> f32 {
    (window_height - 42.0).max(100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn larger_font_changes_wrap_count_without_changing_page_width() {
        assert_eq!(columns(1000.0, FONT_SIZE), columns(1600.0, FONT_SIZE));
        assert!(columns(800.0, 24.0) < columns(800.0, 16.0));
        assert!(line_height(24.0) > line_height(16.0));
    }
}
