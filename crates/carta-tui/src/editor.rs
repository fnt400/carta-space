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
    cat_highlight: Option<(Cursor, Cursor)>,
}

#[derive(Debug, Clone)]
pub struct CompositeEditor {
    regions: Vec<Region>,
    cursor: Cursor,
    anchor: Option<Cursor>,
    cat_highlight: Option<(Cursor, Cursor)>,
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
            cat_highlight: None,
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
        self.cat_highlight = None;
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
        self.cat_highlight = None;
        self.begin_selection(selecting);
        self.cursor = cursor;
        self.clamp_cursor();
        self.preferred_column = None;
        self.finish_selection(selecting);
    }

    pub fn set_cursor_preserving_highlight(&mut self, cursor: Cursor) {
        self.cursor = cursor;
        self.clamp_cursor();
        self.preferred_column = None;
    }

    pub fn cancel_selection(&mut self) {
        self.anchor = None;
    }

    pub fn clear_cat_highlight(&mut self) {
        self.cat_highlight = None;
    }

    pub fn cat_highlight(&self) -> Option<(Cursor, Cursor)> {
        self.cat_highlight
    }

    pub fn set_cat_highlight(&mut self, start: Cursor, end: Cursor) -> bool {
        if start.region != end.region || start == end {
            return false;
        }
        let (start, end) = ordered(start, end);
        let Some(region) = self.regions.get(start.region) else {
            return false;
        };
        if end.byte > region.text.len()
            || !region.text.is_char_boundary(start.byte)
            || !region.text.is_char_boundary(end.byte)
        {
            return false;
        }
        self.anchor = None;
        self.cat_highlight = Some((start, end));
        true
    }

    pub fn selection(&self) -> Option<(Cursor, Cursor)> {
        self.cat_highlight.or_else(|| self.conventional_selection())
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
        if self
            .conventional_selection()
            .is_some_and(|(a, b)| a.region != b.region)
        {
            return false;
        }
        self.record();
        self.cat_highlight = None;
        self.delete_selection_inner();
        let region = &mut self.regions[self.cursor.region];
        region.text.insert_str(self.cursor.byte, value);
        self.cursor.byte += value.len();
        self.changed();
        true
    }

    pub fn insert_newline_with_list_continuation(&mut self) -> bool {
        if self.regions.is_empty() {
            return false;
        }
        if self.selection().is_some() {
            return self.insert("\n");
        }
        let text = &self.regions[self.cursor.region].text;
        let line_start = text[..self.cursor.byte]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let before_cursor = &text[line_start..self.cursor.byte];
        let continuation = list_continuation_prefix(before_cursor);
        let mut insertion = String::from("\n");
        if let Some(prefix) = continuation {
            insertion.push_str(&prefix);
        }
        self.insert(&insertion)
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
        if self.cat_highlight.is_some() {
            return self.erase_cat_highlight();
        }
        let Some((start, end)) = self.conventional_selection() else {
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

    pub fn replace_cat_highlight(&mut self, value: &str) -> bool {
        let Some((start, end)) = self.cat_highlight else {
            return false;
        };
        self.record();
        self.regions[start.region]
            .text
            .replace_range(start.byte..end.byte, value);
        self.cursor = Cursor {
            region: start.region,
            byte: start.byte + value.len(),
        };
        self.anchor = None;
        self.cat_highlight = None;
        self.preferred_column = None;
        self.dirty = true;
        true
    }

    pub fn erase_cat_highlight(&mut self) -> bool {
        let Some((start, end)) = self.cat_highlight else {
            return false;
        };
        self.record();
        self.regions[start.region].text.drain(start.byte..end.byte);
        self.cursor = start;
        self.anchor = None;
        self.cat_highlight = None;
        self.preferred_column = None;
        self.dirty = true;
        true
    }

    pub fn copy_cat_highlight(&mut self) -> bool {
        let Some((start, end)) = self.cat_highlight else {
            return false;
        };
        let copy = self.regions[start.region].text[start.byte..end.byte].to_owned();
        if copy.is_empty() {
            return false;
        }
        self.record();
        self.regions[start.region].text.insert_str(end.byte, &copy);
        let copy_start = Cursor {
            region: start.region,
            byte: end.byte,
        };
        let copy_end = Cursor {
            region: start.region,
            byte: end.byte + copy.len(),
        };
        self.cursor = copy_end;
        self.anchor = None;
        self.cat_highlight = Some((copy_start, copy_end));
        self.preferred_column = None;
        self.dirty = true;
        true
    }

    pub fn move_cat_highlight_to(&mut self, destination: Cursor) -> bool {
        let Some((start, end)) = self.cat_highlight else {
            return false;
        };
        if destination.region == start.region
            && destination.byte >= start.byte
            && destination.byte <= end.byte
        {
            self.cat_highlight = None;
            self.cursor = destination;
            return false;
        }
        let Some(target_region) = self.regions.get(destination.region) else {
            return false;
        };
        if destination.byte > target_region.text.len()
            || !target_region.text.is_char_boundary(destination.byte)
        {
            return false;
        }

        let moved = self.regions[start.region].text[start.byte..end.byte].to_owned();
        if moved.is_empty() {
            return false;
        }
        self.record();
        self.regions[start.region].text.drain(start.byte..end.byte);
        let removed_len = end.byte - start.byte;
        let insertion_byte = if destination.region == start.region && destination.byte > end.byte {
            destination.byte - removed_len
        } else {
            destination.byte
        };
        self.regions[destination.region]
            .text
            .insert_str(insertion_byte, &moved);
        let moved_start = Cursor {
            region: destination.region,
            byte: insertion_byte,
        };
        let moved_end = Cursor {
            region: destination.region,
            byte: insertion_byte + moved.len(),
        };
        self.cursor = moved_end;
        self.anchor = None;
        self.cat_highlight = Some((moved_start, moved_end));
        self.preferred_column = None;
        self.dirty = true;
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
        self.cat_highlight = None;
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
        self.cat_highlight = None;
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
        self.cat_highlight = None;
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
        self.cat_highlight = None;
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
        self.cat_highlight = None;
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
        self.cat_highlight = None;
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

    pub fn visual_boundary_cursor(&self, width: usize, forward: bool) -> Option<Cursor> {
        let region = self.regions.get(self.cursor.region)?;
        let ranges = visual_ranges(&region.text, width.max(1));
        let range = ranges
            .iter()
            .rfind(|(start, end)| self.cursor.byte >= *start && self.cursor.byte <= *end)?;
        Some(Cursor {
            region: self.cursor.region,
            byte: if forward { range.1 } else { range.0 },
        })
    }

    pub fn visual_home(&mut self, width: usize, selecting: bool) {
        if self.regions.is_empty() {
            return;
        }
        self.cat_highlight = None;
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
        self.cat_highlight = None;
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
        let mut remaining = lines.max(1);
        while remaining > 0 {
            let before = self.cursor.region;
            self.move_visual(down, width, selecting);
            remaining = remaining.saturating_sub(if self.cursor.region != before { 4 } else { 1 });
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
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn swap_history(&mut self, undo: bool) -> bool {
        let source = if undo { &mut self.undo } else { &mut self.redo };
        let Some(snapshot) = source.pop() else {
            return false;
        };
        let current = Snapshot {
            regions: self.regions.clone(),
            cursor: self.cursor,
            cat_highlight: self.cat_highlight,
        };
        if undo {
            self.redo.push(current);
        } else {
            self.undo.push(current);
        }
        self.regions = snapshot.regions;
        self.cursor = snapshot.cursor;
        self.anchor = None;
        self.cat_highlight = snapshot.cat_highlight;
        self.dirty = true;
        true
    }

    fn record(&mut self) {
        self.undo.push(Snapshot {
            regions: self.regions.clone(),
            cursor: self.cursor,
            cat_highlight: self.cat_highlight,
        });
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    fn changed(&mut self) {
        self.anchor = None;
        self.cat_highlight = None;
        self.preferred_column = None;
        self.dirty = true;
    }
    fn conventional_selection(&self) -> Option<(Cursor, Cursor)> {
        let anchor = self.anchor?;
        (anchor != self.cursor).then(|| ordered(anchor, self.cursor))
    }
    fn delete_selection_inner(&mut self) {
        if let Some((start, end)) = self.conventional_selection() {
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

fn list_continuation_prefix(line_before_cursor: &str) -> Option<String> {
    let indent_len = line_before_cursor
        .char_indices()
        .take_while(|(_, character)| matches!(character, ' ' | '\t'))
        .map(|(index, character)| index + character.len_utf8())
        .last()
        .unwrap_or(0);
    let indent = &line_before_cursor[..indent_len];
    let rest = &line_before_cursor[indent_len..];

    for marker in ["- ", "* ", "+ "] {
        if rest.starts_with(marker) {
            return Some(format!("{indent}{marker}"));
        }
    }

    let digit_len = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digit_len == 0 {
        return None;
    }
    let punctuation = rest.as_bytes().get(digit_len).copied()?;
    if !matches!(punctuation, b'.' | b')') || rest.as_bytes().get(digit_len + 1) != Some(&b' ') {
        return None;
    }
    let number = rest[..digit_len].parse::<u64>().ok()?;
    let next = number.saturating_add(1);
    let next_number = format!("{next:0width$}", width = digit_len);
    Some(format!("{indent}{next_number}{} ", punctuation as char))
}

fn byte_at_column(text: &str, base: usize, column: usize) -> usize {
    text.char_indices()
        .nth(column)
        .map_or(base + text.len(), |(i, _)| base + i)
}

pub fn visual_ranges(text: &str, width: usize) -> Vec<(usize, usize)> {
    let width = width.max(1);
    if text.is_empty() {
        return vec![(0, 0)];
    }

    let mut ranges = Vec::new();
    let mut logical_start = 0;
    loop {
        let logical_end = text[logical_start..]
            .find('\n')
            .map_or(text.len(), |offset| logical_start + offset);
        wrap_logical_line(text, logical_start, logical_end, width, &mut ranges);
        if logical_end == text.len() {
            break;
        }
        logical_start = logical_end + 1;
        if logical_start == text.len() {
            ranges.push((logical_start, logical_start));
            break;
        }
    }
    ranges
}

fn wrap_logical_line(
    text: &str,
    line_start: usize,
    line_end: usize,
    width: usize,
    ranges: &mut Vec<(usize, usize)>,
) {
    if line_start == line_end {
        ranges.push((line_start, line_end));
        return;
    }

    let mut start = line_start;
    while start < line_end {
        let mut used = 0;
        let mut last_break = None;
        let mut overflow = None;

        for (relative, character) in text[start..line_end].char_indices() {
            let byte = start + relative;
            let character_width = character.width().unwrap_or(0);
            if used > 0 && used + character_width > width {
                overflow = Some(byte);
                break;
            }
            used += character_width;
            if character.is_whitespace() {
                last_break = Some(byte + character.len_utf8());
            }
        }

        let Some(overflow_byte) = overflow else {
            ranges.push((start, line_end));
            break;
        };
        let end = last_break
            .filter(|break_byte| *break_byte > start)
            .unwrap_or(overflow_byte);
        ranges.push((start, end));
        start = end;
    }
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
    fn cat_highlight_copies_moves_erases_and_undo_restores_it() {
        let first = id();
        let second = id();
        let mut e = CompositeEditor::new(
            vec![
                Region {
                    document: first,
                    text: "alpha beta".into(),
                },
                Region {
                    document: second,
                    text: "target".into(),
                },
            ],
            Cursor { region: 0, byte: 0 },
        );

        assert!(e.set_cat_highlight(
            Cursor { region: 0, byte: 6 },
            Cursor {
                region: 0,
                byte: 10
            },
        ));
        assert_eq!(e.selected_text().as_deref(), Some("beta"));
        assert!(e.copy_cat_highlight());
        assert_eq!(e.regions()[0].text, "alpha betabeta");
        assert_eq!(e.selected_text().as_deref(), Some("beta"));

        assert!(e.move_cat_highlight_to(Cursor { region: 1, byte: 0 }));
        assert_eq!(e.regions()[0].text, "alpha beta");
        assert_eq!(e.regions()[1].text, "betatarget");
        assert_eq!(e.selected_text().as_deref(), Some("beta"));

        assert!(e.erase_cat_highlight());
        assert_eq!(e.regions()[1].text, "target");
        assert!(e.undo());
        assert_eq!(e.regions()[1].text, "betatarget");
        assert_eq!(e.selected_text().as_deref(), Some("beta"));
    }

    #[test]
    fn cat_highlight_cannot_cross_document_boundary() {
        let mut e = editor();
        assert!(!e.set_cat_highlight(Cursor { region: 0, byte: 0 }, Cursor { region: 1, byte: 0 },));
        assert!(e.cat_highlight().is_none());
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
    fn enter_continues_unordered_lists_without_changing_other_lines() {
        for marker in ["- ", "* ", "+ "] {
            let mut e = CompositeEditor::new(
                vec![Region {
                    document: id(),
                    text: format!("{marker}item"),
                }],
                Cursor {
                    region: 0,
                    byte: marker.len() + 4,
                },
            );
            assert!(e.insert_newline_with_list_continuation());
            assert_eq!(e.regions()[0].text, format!("{marker}item\n{marker}"));
        }

        let mut plain = CompositeEditor::new(
            vec![Region {
                document: id(),
                text: "plain".into(),
            }],
            Cursor { region: 0, byte: 5 },
        );
        assert!(plain.insert_newline_with_list_continuation());
        assert_eq!(plain.regions()[0].text, "plain\n");
    }

    #[test]
    fn enter_increments_ordered_list_markers_and_preserves_indentation() {
        let mut e = CompositeEditor::new(
            vec![Region {
                document: id(),
                text: "  09. item".into(),
            }],
            Cursor {
                region: 0,
                byte: "  09. item".len(),
            },
        );
        assert!(e.insert_newline_with_list_continuation());
        assert_eq!(e.regions()[0].text, "  09. item\n  10. ");

        let mut paren = CompositeEditor::new(
            vec![Region {
                document: id(),
                text: "3) item".into(),
            }],
            Cursor {
                region: 0,
                byte: "3) item".len(),
            },
        );
        assert!(paren.insert_newline_with_list_continuation());
        assert_eq!(paren.regions()[0].text, "3) item\n4) ");
    }

    #[test]
    fn visual_wrap_prefers_word_boundaries_and_covers_source_bytes() {
        let text = "alpha beta gamma";
        let ranges = visual_ranges(text, 6);
        assert_eq!(ranges, vec![(0, 6), (6, 11), (11, 16)]);
        assert_eq!(
            ranges
                .iter()
                .map(|(start, end)| &text[*start..*end])
                .collect::<String>(),
            text
        );

        let long = "abcdefghijk";
        assert_eq!(visual_ranges(long, 5), vec![(0, 5), (5, 10), (10, 11)]);
        assert_eq!(visual_ranges("a\n", 80), vec![(0, 1), (2, 2)]);
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
