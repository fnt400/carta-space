use carta_core::DocumentId;
use std::cmp::Ordering;
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub document: DocumentId,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub region: usize,
    pub byte: usize,
}

#[derive(Debug, Clone)]
struct Snapshot {
    regions: Vec<Region>,
    cursor: Cursor,
}

#[derive(Debug, Clone)]
pub struct CompositeEditor {
    regions: Vec<Region>,
    cursor: Cursor,
    anchor: Option<Cursor>,
    preferred_column: Option<usize>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    dirty: bool,
}

impl CompositeEditor {
    pub fn new(regions: Vec<Region>, cursor: Cursor) -> Self {
        let mut editor = Self {
            regions,
            cursor,
            anchor: None,
            preferred_column: None,
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
        };
        editor.clamp_cursor();
        editor
    }

    pub fn regions(&self) -> &[Region] {
        &self.regions
    }
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }
    pub fn anchor(&self) -> Option<Cursor> {
        self.anchor
    }
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }
    pub fn scrub_document(&mut self, document: DocumentId) {
        self.regions.retain(|region| region.document != document);
        self.undo.retain(|snapshot| {
            !snapshot
                .regions
                .iter()
                .any(|region| region.document == document)
        });
        self.redo.retain(|snapshot| {
            !snapshot
                .regions
                .iter()
                .any(|region| region.document == document)
        });
        self.anchor = None;
        self.cursor = Cursor { region: 0, byte: 0 };
        self.clamp_cursor();
    }
    pub fn current_document(&self) -> Option<DocumentId> {
        self.regions.get(self.cursor.region).map(|r| r.document)
    }
    pub fn current_text(&self) -> Option<&str> {
        self.regions
            .get(self.cursor.region)
            .map(|r| r.text.as_str())
    }

    pub fn set_cursor(&mut self, cursor: Cursor, selecting: bool) {
        self.begin_selection(selecting);
        self.cursor = cursor;
        self.clamp_cursor();
        self.preferred_column = None;
        self.finish_selection(selecting);
    }

    pub fn cancel_selection(&mut self) {
        self.anchor = None;
    }

    pub fn selection(&self) -> Option<(Cursor, Cursor)> {
        let anchor = self.anchor?;
        (anchor != self.cursor).then(|| ordered(anchor, self.cursor))
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection()?;
        let mut out = String::new();
        for index in start.region..=end.region {
            let text = &self.regions[index].text;
            let from = if index == start.region { start.byte } else { 0 };
            let to = if index == end.region {
                end.byte
            } else {
                text.len()
            };
            out.push_str(&text[from..to]);
        }
        Some(out)
    }

    pub fn insert(&mut self, value: &str) -> bool {
        if self.regions.is_empty() {
            return false;
        }
        if self.selection().is_some_and(|(a, b)| a.region != b.region) {
            return false;
        }
        self.record();
        self.delete_selection_inner();
        let region = &mut self.regions[self.cursor.region];
        region.text.insert_str(self.cursor.byte, value);
        self.cursor.byte += value.len();
        self.changed();
        true
    }

    pub fn backspace(&mut self) -> bool {
        if self.regions.is_empty() {
            return false;
        }
        if self.selection().is_some() {
            return self.delete_selection();
        }
        if self.cursor.byte == 0 {
            return false;
        }
        self.record();
        let text = &mut self.regions[self.cursor.region].text;
        let previous = text[..self.cursor.byte]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i);
        text.drain(previous..self.cursor.byte);
        self.cursor.byte = previous;
        self.changed();
        true
    }

    pub fn delete(&mut self) -> bool {
        if self.regions.is_empty() {
            return false;
        }
        if self.selection().is_some() {
            return self.delete_selection();
        }
        let text = &self.regions[self.cursor.region].text;
        if self.cursor.byte == text.len() {
            return false;
        }
        self.record();
        let text = &mut self.regions[self.cursor.region].text;
        let next = self.cursor.byte + text[self.cursor.byte..].chars().next().unwrap().len_utf8();
        text.drain(self.cursor.byte..next);
        self.changed();
        true
    }

    pub fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection() else {
            return false;
        };
        if start.region != end.region {
            return false;
        }
        self.record();
        self.delete_selection_inner();
        self.changed();
        true
    }

    pub fn indent_less(&mut self) -> bool {
        if self.regions.is_empty() {
            return false;
        }
        if self.selection().is_some_and(|(a, b)| a.region != b.region) {
            return false;
        }
        let text = &self.regions[self.cursor.region].text;
        let line = text[..self.cursor.byte].rfind('\n').map_or(0, |i| i + 1);
        let count = text[line..]
            .chars()
            .take_while(|c| *c == ' ')
            .take(4)
            .count();
        if count == 0 {
            return false;
        }
        self.record();
        self.regions[self.cursor.region]
            .text
            .drain(line..line + count);
        self.cursor.byte = self.cursor.byte.saturating_sub(count);
        if let Some(anchor) = &mut self.anchor {
            anchor.byte = anchor.byte.saturating_sub(count);
        }
        self.changed();
        true
    }

    pub fn move_horizontal(&mut self, forward: bool, selecting: bool) {
        if self.regions.is_empty() {
            return;
        }
        self.begin_selection(selecting);
        let text = &self.regions[self.cursor.region].text;
        if forward {
            if self.cursor.byte < text.len() {
                self.cursor.byte += text[self.cursor.byte..].chars().next().unwrap().len_utf8();
            } else if self.cursor.region + 1 < self.regions.len() {
                self.cursor = Cursor {
                    region: self.cursor.region + 1,
                    byte: 0,
                };
            }
        } else if self.cursor.byte > 0 {
            self.cursor.byte = text[..self.cursor.byte]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
        } else if self.cursor.region > 0 {
            self.cursor.region -= 1;
            self.cursor.byte = self.regions[self.cursor.region].text.len();
        }
        self.preferred_column = None;
        self.finish_selection(selecting);
    }

    pub fn move_word(&mut self, forward: bool, selecting: bool) {
        if self.regions.is_empty() {
            return;
        }
        self.begin_selection(selecting);
        let text = &self.regions[self.cursor.region].text;
        if forward {
            while self.cursor.byte < text.len()
                && text[self.cursor.byte..]
                    .chars()
                    .next()
                    .is_some_and(is_word_character)
            {
                self.cursor.byte += text[self.cursor.byte..].chars().next().unwrap().len_utf8();
            }
            while self.cursor.byte < text.len()
                && text[self.cursor.byte..]
                    .chars()
                    .next()
                    .is_some_and(|character| !is_word_character(character))
            {
                self.cursor.byte += text[self.cursor.byte..].chars().next().unwrap().len_utf8();
            }
        } else {
            while self.cursor.byte > 0 {
                let (previous, character) =
                    text[..self.cursor.byte].char_indices().next_back().unwrap();
                if is_word_character(character) {
                    break;
                }
                self.cursor.byte = previous;
            }
            while self.cursor.byte > 0 {
                let (previous, character) =
                    text[..self.cursor.byte].char_indices().next_back().unwrap();
                if !is_word_character(character) {
                    break;
                }
                self.cursor.byte = previous;
            }
        }
        self.preferred_column = None;
        self.finish_selection(selecting);
    }

    pub fn move_line(&mut self, down: bool, selecting: bool) {
        if self.regions.is_empty() {
            return;
        }
        self.begin_selection(selecting);
        let text = &self.regions[self.cursor.region].text;
        let line_start = text[..self.cursor.byte].rfind('\n').map_or(0, |i| i + 1);
        let column = self
            .preferred_column
            .unwrap_or_else(|| text[line_start..self.cursor.byte].chars().count());
        self.preferred_column = Some(column);
        if down {
            let Some(end_rel) = text[self.cursor.byte..].find('\n') else {
                if self.cursor.region + 1 < self.regions.len() {
                    self.cursor = Cursor {
                        region: self.cursor.region + 1,
                        byte: 0,
                    };
                }
                self.finish_selection(selecting);
                return;
            };
            let start = self.cursor.byte + end_rel + 1;
            self.cursor.byte = byte_at_column(&text[start..], start, column);
        } else if line_start == 0 {
            if self.cursor.region > 0 {
                self.cursor.region -= 1;
                self.cursor.byte = self.regions[self.cursor.region].text.len();
            }
        } else {
            let previous_end = line_start - 1;
            let start = text[..previous_end].rfind('\n').map_or(0, |i| i + 1);
            self.cursor.byte = byte_at_column(&text[start..previous_end], start, column);
        }
        self.finish_selection(selecting);
    }

    pub fn home(&mut self, selecting: bool) {
        if self.regions.is_empty() {
            return;
        }
        self.begin_selection(selecting);
        let text = &self.regions[self.cursor.region].text;
        self.cursor.byte = text[..self.cursor.byte].rfind('\n').map_or(0, |i| i + 1);
        self.preferred_column = None;
        self.finish_selection(selecting);
    }

    pub fn end(&mut self, selecting: bool) {
        if self.regions.is_empty() {
            return;
        }
        self.begin_selection(selecting);
        let text = &self.regions[self.cursor.region].text;
        self.cursor.byte = text[self.cursor.byte..]
            .find('\n')
            .map_or(text.len(), |i| self.cursor.byte + i);
        self.preferred_column = None;
        self.finish_selection(selecting);
    }

    pub fn page(&mut self, down: bool, lines: usize, selecting: bool) {
        for _ in 0..lines.max(1) {
            self.move_line(down, selecting);
        }
    }

    pub fn move_visual(&mut self, down: bool, width: usize, selecting: bool) {
        if self.regions.is_empty() {
            return;
        }
        self.begin_selection(selecting);
        let ranges = visual_ranges(&self.regions[self.cursor.region].text, width.max(1));
        let row = ranges
            .iter()
            .rposition(|(start, end)| self.cursor.byte >= *start && self.cursor.byte <= *end)
            .unwrap_or(0);
        let (start, _) = ranges[row];
        let column = self.preferred_column.unwrap_or_else(|| {
            self.regions[self.cursor.region].text[start..self.cursor.byte]
                .chars()
                .map(|character| character.width().unwrap_or(0))
                .sum()
        });
        self.preferred_column = Some(column);
        if down && row + 1 < ranges.len() {
            self.cursor.byte = byte_at_visual_column(
                &self.regions[self.cursor.region].text,
                ranges[row + 1],
                column,
            );
        } else if !down && row > 0 {
            self.cursor.byte = byte_at_visual_column(
                &self.regions[self.cursor.region].text,
                ranges[row - 1],
                column,
            );
        } else if down && self.cursor.region + 1 < self.regions.len() {
            self.cursor.region += 1;
            let next = visual_ranges(&self.regions[self.cursor.region].text, width.max(1));
            self.cursor.byte =
                byte_at_visual_column(&self.regions[self.cursor.region].text, next[0], column);
        } else if !down && self.cursor.region > 0 {
            self.cursor.region -= 1;
            let previous = visual_ranges(&self.regions[self.cursor.region].text, width.max(1));
            self.cursor.byte = byte_at_visual_column(
                &self.regions[self.cursor.region].text,
                *previous.last().unwrap(),
                column,
            );
        }
        self.finish_selection(selecting);
    }

    pub fn visual_home(&mut self, width: usize, selecting: bool) {
        if self.regions.is_empty() {
            return;
        }
        self.begin_selection(selecting);
        let ranges = visual_ranges(&self.regions[self.cursor.region].text, width.max(1));
        self.cursor.byte = ranges
            .iter()
            .rfind(|(start, end)| self.cursor.byte >= *start && self.cursor.byte <= *end)
            .map_or(0, |range| range.0);
        self.preferred_column = None;
        self.finish_selection(selecting);
    }

    pub fn visual_end(&mut self, width: usize, selecting: bool) {
        if self.regions.is_empty() {
            return;
        }
        self.begin_selection(selecting);
        let ranges = visual_ranges(&self.regions[self.cursor.region].text, width.max(1));
        self.cursor.byte = ranges
            .iter()
            .rfind(|(start, end)| self.cursor.byte >= *start && self.cursor.byte <= *end)
            .map_or(self.regions[self.cursor.region].text.len(), |range| range.1);
        self.preferred_column = None;
        self.finish_selection(selecting);
    }

    pub fn page_visual(&mut self, down: bool, lines: usize, width: usize, selecting: bool) {
        for _ in 0..lines.max(1) {
            self.move_visual(down, width, selecting);
        }
    }

    pub fn document_home(&mut self, selecting: bool) {
        if self.regions.is_empty() {
            return;
        }
        self.set_cursor(
            Cursor {
                region: self.cursor.region,
                byte: 0,
            },
            selecting,
        );
    }

    pub fn document_end(&mut self, selecting: bool) {
        let Some(region) = self.regions.get(self.cursor.region) else {
            return;
        };
        self.set_cursor(
            Cursor {
                region: self.cursor.region,
                byte: region.text.len(),
            },
            selecting,
        );
    }

    pub fn move_document(&mut self, forward: bool) {
        if self.regions.is_empty() {
            return;
        }
        let region = if forward {
            (self.cursor.region + 1).min(self.regions.len() - 1)
        } else {
            self.cursor.region.saturating_sub(1)
        };
        if region != self.cursor.region {
            self.set_cursor(Cursor { region, byte: 0 }, false);
        }
    }

    pub fn undo(&mut self) -> bool {
        self.swap_history(true)
    }
    pub fn redo(&mut self) -> bool {
        self.swap_history(false)
    }

    fn swap_history(&mut self, undo: bool) -> bool {
        let source = if undo { &mut self.undo } else { &mut self.redo };
        let Some(snapshot) = source.pop() else {
            return false;
        };
        let current = Snapshot {
            regions: self.regions.clone(),
            cursor: self.cursor,
        };
        if undo {
            self.redo.push(current);
        } else {
            self.undo.push(current);
        }
        self.regions = snapshot.regions;
        self.cursor = snapshot.cursor;
        self.anchor = None;
        self.dirty = true;
        true
    }

    fn record(&mut self) {
        self.undo.push(Snapshot {
            regions: self.regions.clone(),
            cursor: self.cursor,
        });
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    fn changed(&mut self) {
        self.anchor = None;
        self.preferred_column = None;
        self.dirty = true;
    }
    fn delete_selection_inner(&mut self) {
        if let Some((start, end)) = self.selection() {
            self.regions[start.region].text.drain(start.byte..end.byte);
            self.cursor = start;
            self.anchor = None;
        }
    }
    fn begin_selection(&mut self, selecting: bool) {
        if selecting && self.anchor.is_none() {
            self.anchor = Some(self.cursor);
        }
    }
    fn finish_selection(&mut self, selecting: bool) {
        if !selecting || self.anchor == Some(self.cursor) {
            self.anchor = None;
        }
    }
    fn clamp_cursor(&mut self) {
        if self.regions.is_empty() {
            self.cursor = Cursor { region: 0, byte: 0 };
            return;
        }
        self.cursor.region = self.cursor.region.min(self.regions.len() - 1);
        let text = &self.regions[self.cursor.region].text;
        self.cursor.byte = self.cursor.byte.min(text.len());
        while !text.is_char_boundary(self.cursor.byte) {
            self.cursor.byte -= 1;
        }
    }
}

fn ordered(a: Cursor, b: Cursor) -> (Cursor, Cursor) {
    if compare(a, b) == Ordering::Greater {
        (b, a)
    } else {
        (a, b)
    }
}
fn compare(a: Cursor, b: Cursor) -> Ordering {
    (a.region, a.byte).cmp(&(b.region, b.byte))
}
fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}
fn byte_at_column(text: &str, base: usize, column: usize) -> usize {
    text.char_indices()
        .nth(column)
        .map_or(base + text.len(), |(i, _)| base + i)
}

fn visual_ranges(text: &str, width: usize) -> Vec<(usize, usize)> {
    if text.is_empty() {
        return vec![(0, 0)];
    }
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut used = 0;
    for (byte, character) in text.char_indices() {
        if character == '\n' {
            ranges.push((start, byte));
            start = byte + 1;
            used = 0;
            continue;
        }
        let character_width = character.width().unwrap_or(0);
        if used > 0 && used + character_width > width {
            ranges.push((start, byte));
            start = byte;
            used = 0;
        }
        used += character_width;
    }
    ranges.push((start, text.len()));
    ranges
}

fn byte_at_visual_column(text: &str, range: (usize, usize), column: usize) -> usize {
    let mut used = 0;
    for (relative, character) in text[range.0..range.1].char_indices() {
        let width = character.width().unwrap_or(0);
        if used + width > column {
            return range.0 + relative;
        }
        used += width;
    }
    range.1
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id() -> DocumentId {
        DocumentId::new_v7()
    }
    fn editor() -> CompositeEditor {
        CompositeEditor::new(
            vec![
                Region {
                    document: id(),
                    text: "aé".into(),
                },
                Region {
                    document: id(),
                    text: "二b".into(),
                },
            ],
            Cursor { region: 0, byte: 0 },
        )
    }

    #[test]
    fn cursor_crosses_but_backspace_cannot_remove_boundary() {
        let mut e = editor();
        e.end(false);
        e.move_horizontal(true, false);
        assert_eq!(e.cursor(), Cursor { region: 1, byte: 0 });
        assert!(!e.backspace());
        assert_eq!(e.regions().len(), 2);
    }
    #[test]
    fn cross_boundary_selection_copies_without_separator_and_cannot_destroy() {
        let mut e = editor();
        e.set_cursor(Cursor { region: 0, byte: 1 }, false);
        e.set_cursor(Cursor { region: 1, byte: 3 }, true);
        assert_eq!(e.selected_text().as_deref(), Some("é二"));
        assert!(!e.delete_selection());
        assert!(!e.insert("x"));
    }
    #[test]
    fn unicode_edit_undo_redo_and_indent() {
        let mut e = editor();
        assert!(e.insert("λ"));
        assert_eq!(e.cursor().byte, 2);
        assert!(e.undo());
        assert!(e.redo());
        assert_eq!(&e.regions()[0].text, "λaé");
        e.home(false);
        assert!(e.insert("    "));
        assert!(e.indent_less());
    }

    #[test]
    fn word_selection_uses_predictable_boundaries() {
        let mut e = CompositeEditor::new(
            vec![Region {
                document: id(),
                text: "alpha, beta gamma".into(),
            }],
            Cursor {
                region: 0,
                byte: 12,
            },
        );
        e.move_word(false, true);
        assert_eq!(e.selected_text().as_deref(), Some("beta "));
        e.move_word(false, true);
        assert_eq!(e.selected_text().as_deref(), Some("alpha, beta "));

        e.set_cursor(Cursor { region: 0, byte: 0 }, false);
        e.move_word(true, true);
        assert_eq!(e.selected_text().as_deref(), Some("alpha, "));
        e.move_word(true, true);
        assert_eq!(e.selected_text().as_deref(), Some("alpha, beta "));
    }

    #[test]
    fn zero_region_editor_accepts_normal_input_without_panicking() {
        let mut e = CompositeEditor::new(Vec::new(), Cursor { region: 0, byte: 0 });
        assert!(!e.insert("x"));
        assert!(!e.backspace());
        assert!(!e.delete());
        assert!(!e.indent_less());
        e.move_horizontal(true, true);
        e.move_word(true, true);
        e.move_line(true, true);
        e.move_visual(true, 80, true);
        e.home(true);
        e.end(true);
        e.visual_home(80, true);
        e.visual_end(80, true);
        e.page_visual(true, 20, 80, true);
        e.document_home(true);
        e.document_end(true);
        e.move_document(true);
        assert_eq!(e.cursor(), Cursor { region: 0, byte: 0 });
        assert!(e.selection().is_none());
    }
}
