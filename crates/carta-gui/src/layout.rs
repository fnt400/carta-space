//! Disposable, indexed presentation projection for virtual GUI scrolling.
//! No authored text is copied into the layout, and only visible rows are
//! submitted to Iced's text shaper and software rasterizer.
use carta_app::{App, Cursor, View};
use carta_core::DocumentId;
use std::collections::HashMap;

#[derive(Debug)]
struct DocumentRows {
    starts: Vec<usize>, // byte start of each soft/hard wrapped visual row
    prefix: usize,
    locked: bool,
    generation: u64,
    content_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Gap,
    Rule {
        region: usize,
    },
    Text {
        region: usize,
        start: usize,
        end: usize,
        hard_break: bool,
    },
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
    totals: RowTotals,
    serial: u64,
}

/// Fenwick tree of Document block heights. Prefix lookups, row-to-Document
/// searches and single-Document height changes take O(log Documents). Every
/// block includes generated separators and at least one visual text row.
#[derive(Default)]
struct RowTotals {
    tree: Vec<i64>,
}

impl RowTotals {
    fn reset(&mut self, lengths: impl IntoIterator<Item = usize>) {
        let lengths: Vec<_> = lengths.into_iter().collect();
        self.tree = vec![0; lengths.len() + 1];
        for (i, length) in lengths.into_iter().enumerate() {
            self.add(i, length as i64);
        }
    }

    fn add(&mut self, index: usize, delta: i64) {
        let mut i = index + 1;
        while i < self.tree.len() {
            self.tree[i] += delta;
            i += i & (!i + 1);
        }
    }

    fn prefix(&self, count: usize) -> usize {
        let mut i = count;
        let mut sum = 0_i64;
        while i > 0 {
            sum += self.tree[i];
            i &= i - 1;
        }
        sum as usize
    }

    fn total(&self) -> usize {
        self.prefix(self.tree.len().saturating_sub(1))
    }

    /// Locate a 0-based row, returning (Document index, local block row).
    /// Caller ensures row < total.
    fn find(&self, row: usize) -> (usize, usize) {
        let mut index = 0;
        let mut count = 0_i64;
        let mut step = self.tree.len().next_power_of_two() / 2;
        while step > 0 {
            let next = index + step;
            if next < self.tree.len() && count + self.tree[next] <= row as i64 {
                index = next;
                count += self.tree[next];
            }
            step /= 2;
        }
        (index, row - count as usize)
    }
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
    // Use the editor's canonical word-wrap boundaries so cursor navigation,
    // pointer hit-testing and displayed rows agree. This is derived layout;
    // no newlines are inserted into authored text.
    carta_app::editor::visual_ranges(text, width)
        .into_iter()
        .map(|(start, _)| start)
        .collect()
}

impl Layout {
    fn next_generation(&mut self) -> u64 {
        self.serial = self.serial.wrapping_add(1);
        self.serial
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
                let locked = app
                    .archive
                    .document_is_locked(region.document)
                    .unwrap_or(false);
                let prefix = prefix_rows(&app.view, i, locked);
                let generation = self.next_generation();
                self.docs.push(DocumentRows {
                    starts: row_starts(&region.text, columns),
                    prefix,
                    locked,
                    generation,
                    content_version: app.editor.document_revision(region.document),
                });
                self.indices.insert(region.document, i);
            }
            self.totals
                .reset(self.docs.iter().map(|doc| doc.prefix + doc.starts.len()));
        } else {
            // Editing a Document invalidates its rows, not all other Documents.
            // Dirty IDs are supplied by the canonical editor. The current
            // region is a safety fallback if an autosave happened before view.
            let dirty: Vec<_> = app.editor.dirty_documents().collect();
            let current = app.editor.current_document();
            let changed_ids = dirty.into_iter().chain(current);
            for id in changed_ids {
                if let Some(&index) = self.indices.get(&id) {
                    let version = app.editor.document_revision(id);
                    if self.docs[index].content_version == version {
                        continue;
                    }
                    let generation = self.next_generation();
                    let new_rows = row_starts(&regions[index].text, columns);
                    let previous = self.docs[index].starts.len() as i64;
                    let updated = new_rows.len() as i64;
                    self.docs[index].starts = new_rows;
                    self.docs[index].generation = generation;
                    self.docs[index].content_version = version;
                    self.totals.add(index, updated - previous);
                }
            }
        }

        self.editor_ptr = ptr;
        self.region_count = regions.len();
        self.editor_revision = revision;
        self.columns = columns;
        self.view_kind = kind;
    }

    pub fn total_rows(&self) -> usize {
        self.totals.total()
    }

    pub fn generation(&self, region: usize) -> u64 {
        self.docs[region].generation
    }

    pub fn caret_row(&self, app: &App) -> usize {
        let cursor = app.editor.cursor();
        let Some(doc) = self.docs.get(cursor.region) else {
            return 0;
        };
        let starts = &doc.starts;
        let byte = cursor.byte;
        let visual_row = starts
            .partition_point(|&start| start <= byte)
            .saturating_sub(1);
        self.totals.prefix(cursor.region) + doc.prefix + visual_row
    }

    pub fn row(&self, app: &App, row: usize) -> Option<Row> {
        if row >= self.total_rows() {
            return None;
        }
        let (index, local) = self.totals.find(row);
        let doc = &self.docs[index];
        if local < doc.prefix {
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

        let visual = local - doc.prefix;
        let start = doc.starts[visual];
        let text = &app.editor.regions()[index].text;
        let next = doc.starts.get(visual + 1).copied().unwrap_or(text.len());
        let hard_break = next > start && text.as_bytes()[next - 1] == b'\n';
        let end = if hard_break { next - 1 } else { next };
        Some(Row::Text {
            region: index,
            start,
            end,
            hard_break,
        })
    }

    /// Byte offset in the visible row, rounded to an insertion cell.
    pub fn hit_test(&self, app: &App, row: usize, column: usize) -> Option<Cursor> {
        let candidate = self.row(app, row.min(self.total_rows().saturating_sub(1)))?;
        match candidate {
            Row::Rule { region } => Some(Cursor { region, byte: 0 }),
            Row::Gap => {
                // A generated rule or gap cannot be edited.
                let (region, _) = self.totals.find(row.min(self.total_rows() - 1));
                Some(Cursor { region, byte: 0 })
            }
            Row::Text {
                region, start, end, ..
            } => {
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

    pub fn window(
        &self,
        offset: f32,
        height: f32,
        line_height: f32,
        top_pad: f32,
    ) -> (usize, usize) {
        let visible_top = ((offset - top_pad).max(0.0) / line_height).floor() as usize;
        let visible = (height / line_height).ceil() as usize + 2;
        let overscan = visible.max(16);
        let total = self.total_rows();
        let first = visible_top.saturating_sub(overscan).min(total);
        let last = visible_top
            .saturating_add(visible)
            .saturating_add(overscan)
            .min(total);
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
        assert_eq!(row_starts("one two three", 7), vec![0, 4, 8]);
        assert_eq!(row_starts("prima éé dopo", 8), vec![0, 6]);
    }

    #[test]
    fn updating_one_document_does_not_scan_or_reposition_other_documents() {
        let mut totals = RowTotals::default();
        totals.reset([5, 10, 20, 3, 11]);
        assert_eq!(totals.total(), 49);
        assert_eq!(totals.prefix(3), 35);
        assert_eq!(totals.find(0), (0, 0));
        assert_eq!(totals.find(14), (1, 9));
        assert_eq!(totals.find(15), (2, 0));
        totals.add(1, -6); // Document 1 got six visual rows shorter.
        assert_eq!(totals.total(), 43);
        assert_eq!(totals.prefix(3), 29);
        assert_eq!(totals.find(9), (2, 0));
        totals.add(2, 9);
        assert_eq!(totals.total(), 52);
        assert_eq!(totals.find(51), (4, 10));
    }

    #[test]
    fn indexing_many_documents_does_not_increase_visible_render_work() {
        let mut layout = Layout::default();
        layout.totals.reset(std::iter::repeat_n(6, 20_000));
        assert_eq!(layout.total_rows(), 120_000);
        let (first, last) = layout.window(1_200_000.0, 720.0, 22.0, 480.0);
        assert!(
            last - first < 120,
            "rendered rows cannot grow with Document count"
        );
        assert!(first > 0 && last < layout.total_rows());
    }

    #[test]
    fn stable_window_size_is_independent_of_archive_length() {
        let mut layout = Layout::default();
        layout.totals.reset([1_000_000]);
        let (first, last) = layout.window(640_000.0, 800.0, 22.0, 500.0);
        assert!(last - first <= 130, "must shape only bounded visible rows");
        assert!(first > 0 && last < layout.total_rows());
    }
}
