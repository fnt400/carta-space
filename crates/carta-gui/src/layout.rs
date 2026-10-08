//! Disposable, indexed presentation projection for virtual GUI scrolling.
//! No authored text is copied into the layout, and only visible rows are
//! submitted to Iced's text shaper and software rasterizer.
use carta_app::{App, Cursor, View};
use carta_core::DocumentId;
use std::collections::HashMap;

#[derive(Debug)]
struct DocumentRows {
    starts: Vec<usize>, // byte start of each soft/hard wrapped visual row
    start: usize,       // absolute first row, including generated separator
    content_start: usize,
    prefix: usize,
    locked: bool,
    generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Gap,
    Rule { region: usize },
    Text { region: usize, start: usize, end: usize, hard_break: bool },
}

/// Cache lifetime matches the window, not the Archive. It can be discarded
/// without changing authored data or any session state.
#[derive(Default)]
pub struct Layout {
    editor_ptr: usize,
    region_count: usize,
    editor_revision: u64,
    columns: usize,
    view_kind: u8,
    docs: Vec<DocumentRows>,
    indices: HashMap<DocumentId, usize>,
    total_rows: usize,
    serial: u64,
}

fn view_kind(view: &View) -> u8 {
    match view {
        View::CreationDate(_) => 1,
        View::ModificationDate => 2,
        View::Work(_) => 3,
        _ => 4,
    }
}

fn prefix_rows(view: &View, index: usize, locked: bool) -> usize {
    match view {
        View::CreationDate(_) | View::ModificationDate => 2 + usize::from(index > 0),
        View::Work(_) if index > 0 || locked => 2 + usize::from(index > 0),
        _ if index > 0 => 2,
        _ => 0,
    }
}

fn row_starts(text: &str, width: usize) -> Vec<usize> {
    let mut starts = vec![0];
    let mut col = 0;
    let width = width.max(1);
    for (byte, ch) in text.char_indices() {
        if ch == '\n' {
            starts.push(byte + 1);
            col = 0;
        } else {
            if col >= width {
                starts.push(byte);
                col = 0;
            }
            col += 1;
        }
    }
    starts
}

impl Layout {
    fn next_generation(&mut self) -> u64 {
        self.serial = self.serial.wrapping_add(1);
        self.serial
    }

    fn recalculate_starts(&mut self) {
        let mut current = 0;
        for doc in &mut self.docs {
            doc.start = current;
            doc.content_start = current + doc.prefix;
            current = doc.content_start + doc.starts.len();
        }
        self.total_rows = current;
    }

    pub fn sync(&mut self, app: &App, columns: usize) {
        let regions = app.editor.regions();
        let ptr = regions.as_ptr() as usize;
        let revision = app.editor.content_revision();
        let kind = view_kind(&app.view);
        let rebuild = self.editor_ptr != ptr
            || self.region_count != regions.len()
            || self.columns != columns
            || self.view_kind != kind;

        if !rebuild && self.editor_revision == revision {
            return;
        }

        if rebuild {
            self.docs.clear();
            self.indices.clear();
            for (i, region) in regions.iter().enumerate() {
                let locked = app.archive.document_is_locked(region.document).unwrap_or(false);
                let prefix = prefix_rows(&app.view, i, locked);
                let generation = self.next_generation();
                self.docs.push(DocumentRows {
                    starts: row_starts(&region.text, columns),
                    start: 0,
                    content_start: 0,
                    prefix,
                    locked,
                    generation,
                });
                self.indices.insert(region.document, i);
            }
        } else {
            // Editing a Document invalidates its rows, not all other Documents.
            // Dirty IDs are supplied by the canonical editor. The current
            // region is a safety fallback if an autosave happened before view.
            let dirty: Vec<_> = app.editor.dirty_documents().collect();
            let current = app.editor.current_document();
            let changed_ids = dirty.into_iter().chain(current);
            for id in changed_ids {
                if let Some(&index) = self.indices.get(&id) {
                    let generation = self.next_generation();
                    self.docs[index].starts = row_starts(&regions[index].text, columns);
                    self.docs[index].generation = generation;
                }
            }
        }

        self.editor_ptr = ptr;
        self.region_count = regions.len();
        self.editor_revision = revision;
        self.columns = columns;
        self.view_kind = kind;
        self.recalculate_starts();
    }

    pub fn total_rows(&self) -> usize {
        self.total_rows
    }

    pub fn generation(&self, region: usize) -> u64 {
        self.docs[region].generation
    }

    pub fn caret_row(&self, app: &App) -> usize {
        let cursor = app.editor.cursor();
        let Some(doc) = self.docs.get(cursor.region) else { return 0; };
        let starts = &doc.starts;
        let byte = cursor.byte;
        let visual_row = starts.partition_point(|&start| start <= byte).saturating_sub(1);
        doc.content_start + visual_row
    }

    pub fn row(&self, app: &App, row: usize) -> Option<Row> {
        if row >= self.total_rows { return None; }
        // Binary search by a Document's first row, independent of View size.
        let index = self.docs.partition_point(|doc| doc.start <= row).saturating_sub(1);
        let doc = &self.docs[index];
        if row < doc.content_start {
            let local = row - doc.start;
            let separator = match &app.view {
                View::CreationDate(_) | View::ModificationDate => true,
                View::Work(_) => index > 0 || doc.locked,
                _ => false,
            };
            if separator && local == doc.prefix - 2 {
                return Some(Row::Rule { region: index });
            }
            return Some(Row::Gap);
        }

        let visual = row - doc.content_start;
        let start = doc.starts[visual];
        let text = &app.editor.regions()[index].text;
        let next = doc.starts.get(visual + 1).copied().unwrap_or(text.len());
        let hard_break = next > start && text.as_bytes()[next - 1] == b'\n';
        let end = if hard_break { next - 1 } else { next };
        Some(Row::Text { region: index, start, end, hard_break })
    }

    /// Byte offset in the visible row, rounded to an insertion cell.
    pub fn hit_test(&self, app: &App, row: usize, column: usize) -> Option<Cursor> {
        let candidate = self.row(app, row.min(self.total_rows.saturating_sub(1)))?;
        match candidate {
            Row::Rule { region } => Some(Cursor { region, byte: 0 }),
            Row::Gap => {
                // A generated rule or gap cannot be edited.
                let region = self.docs.partition_point(|doc| doc.content_start <= row);
                Some(Cursor {
                    region: region.min(self.docs.len().saturating_sub(1)),
                    byte: 0,
                })
            }
            Row::Text { region, start, end, .. } => {
                let text = &app.editor.regions()[region].text;
                let byte = text[start..end]
                    .char_indices()
                    .nth(column)
                    .map(|(byte, _)| start + byte)
                    .unwrap_or(end);
                Some(Cursor { region, byte })
            }
        }
    }

    pub fn window(&self, offset: f32, height: f32, line_height: f32, top_pad: f32) -> (usize, usize) {
        let visible_top = ((offset - top_pad).max(0.0) / line_height).floor() as usize;
        let visible = (height / line_height).ceil() as usize + 2;
        let overscan = visible.max(16);
        let first = visible_top.saturating_sub(overscan).min(self.total_rows);
        let last = visible_top.saturating_add(visible).saturating_add(overscan).min(self.total_rows);
        (first, last.max(first))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_rows_are_indexed_at_utf8_boundaries() {
        assert_eq!(row_starts("abcde", 3), vec![0, 3]);
        assert_eq!(row_starts("éééé", 2), vec![0, 4]);
        assert_eq!(row_starts("abc\ndef", 3), vec![0, 4]);
        assert_eq!(row_starts("abc\n", 20), vec![0, 4]);
        assert_eq!(row_starts("", 10), vec![0]);
    }

    #[test]
    fn stable_window_size_is_independent_of_archive_length() {
        let layout = Layout { total_rows: 1_000_000, ..Layout::default() };
        let (first, last) = layout.window(640_000.0, 800.0, 22.0, 500.0);
        assert!(last - first <= 130, "must shape only bounded visible rows");
        assert!(first > 0 && last < layout.total_rows);
    }
}
