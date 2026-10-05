use carta_core::{Archive, ArchiveId, LeapDirection};
use carta_tui::app::{AppMode, View};
use carta_tui::editor::{visual_ranges, Cursor};
use carta_tui::session::{
    data_root, default_archive_path, load_session, migrate_legacy_state, save_session, Session,
};
use carta_tui::{App, Command};
use clap::Parser;
use eframe::egui;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const EDITOR_COLUMNS: usize = 80;
const PAGE_LINES: usize = 24;
const FONT_SIZE: f32 = 17.0;
const ROW_HEIGHT: f32 = 22.0;
const SIDE_PADDING: f32 = 18.0;

#[derive(Parser)]
#[command(
    name = "carta-gui",
    version,
    about = "Experimental Carta Space graphical frontend"
)]
struct Args {
    /// Archive directory. Without it, use the XDG Carta data archive.
    path: Option<PathBuf>,

    /// Create the Archive at PATH before opening it.
    #[arg(long, requires = "path")]
    create: bool,
}

struct GuiApp {
    app: App,
    data_root: PathBuf,
    archive_id: ArchiveId,
    last_saved_session: Option<Session>,
    last_session_save: Instant,
    scroll_cursor_into_view: bool,
}

impl GuiApp {
    fn new(
        app: App,
        data_root: PathBuf,
        archive_id: ArchiveId,
        last_saved_session: Option<Session>,
    ) -> Self {
        Self {
            app,
            data_root,
            archive_id,
            last_saved_session,
            last_session_save: Instant::now(),
            scroll_cursor_into_view: true,
        }
    }

    fn set_error(&mut self, error: impl std::fmt::Display) {
        self.app.status = error.to_string();
        self.app.mode = AppMode::Editing;
    }

    fn run_command(&mut self, command: Command) {
        if let Err(error) = self.app.execute(command) {
            self.set_error(error);
        }
        self.scroll_cursor_into_view = true;
    }

    fn save_now(&mut self) {
        if let Err(error) = self.app.autosave() {
            self.set_error(error);
        }
    }

    fn save_session_if_due(&mut self) {
        if self.last_session_save.elapsed() < Duration::from_secs(1) {
            return;
        }
        let current = self.app.session();
        if self.last_saved_session.as_ref() != Some(&current) {
            match save_session(&self.data_root, self.archive_id, &current) {
                Ok(()) => self.last_saved_session = Some(current),
                Err(error) => self.set_error(format!("Session save failed: {error}")),
            }
        }
        self.last_session_save = Instant::now();
    }

    fn mark_edited(&mut self, changed: bool) {
        if changed {
            self.app.edited(Instant::now());
            self.scroll_cursor_into_view = true;
        }
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|input| input.events.clone());
        for event in events {
            match event {
                egui::Event::Text(text) => {
                    if text.is_empty() {
                        continue;
                    }
                    if matches!(self.app.mode, AppMode::Leap { .. }) {
                        self.app.leap_input(&text);
                        self.scroll_cursor_into_view = true;
                    } else if matches!(self.app.mode, AppMode::Editing) {
                        let changed = self.app.cat_insert(&text);
                        self.mark_edited(changed);
                    }
                }
                egui::Event::Paste(text) => {
                    if matches!(self.app.mode, AppMode::Leap { .. }) {
                        self.app.leap_input(&text);
                        self.scroll_cursor_into_view = true;
                    } else if matches!(self.app.mode, AppMode::Editing) {
                        let changed = self.app.cat_insert(&text);
                        self.mark_edited(changed);
                    }
                }
                egui::Event::Copy => {
                    if let Some(text) = self.app.editor.selected_text() {
                        ctx.copy_text(text);
                    }
                }
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if self.handle_control_key(key, modifiers) {
                        continue;
                    }
                    self.handle_navigation_key(key, modifiers);
                }
                _ => {}
            }
        }
    }

    fn handle_control_key(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> bool {
        if !modifiers.ctrl {
            return false;
        }

        match key {
            egui::Key::B if matches!(self.app.mode, AppMode::Editing) => {
                self.app.start_portable_leap(LeapDirection::Backward);
            }
            egui::Key::F if matches!(self.app.mode, AppMode::Editing) => {
                self.app.start_portable_leap(LeapDirection::Forward);
            }
            egui::Key::R => {
                if matches!(self.app.mode, AppMode::Leap { .. }) {
                    self.app.leap_again_active();
                } else if matches!(self.app.mode, AppMode::Editing) {
                    self.app.portable_leap_again();
                }
            }
            egui::Key::P if matches!(self.app.mode, AppMode::Leap { .. }) => {
                self.app.cancel_leap();
            }
            egui::Key::Z if matches!(self.app.mode, AppMode::Editing) => {
                self.run_command(Command::Undo);
            }
            egui::Key::Y if matches!(self.app.mode, AppMode::Editing) => {
                self.run_command(Command::Redo);
            }
            egui::Key::S if matches!(self.app.mode, AppMode::Editing) => {
                self.save_now();
            }
            _ => return false,
        }
        self.scroll_cursor_into_view = true;
        true
    }

    fn handle_navigation_key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        if matches!(self.app.mode, AppMode::Leap { .. }) {
            match key {
                egui::Key::Enter => self.app.end_leap(),
                egui::Key::Backspace => self.app.leap_backspace(),
                egui::Key::Escape => self.app.cancel_leap(),
                _ => return,
            }
            self.scroll_cursor_into_view = true;
            return;
        }

        if !matches!(self.app.mode, AppMode::Editing) {
            if key == egui::Key::Escape {
                self.app.cancel_mode();
            }
            return;
        }

        let selecting = modifiers.shift;
        let changed = match key {
            egui::Key::Enter => self.app.cat_insert_newline(),
            egui::Key::Backspace => self.app.cat_backspace(),
            egui::Key::Delete => self.app.cat_erase(),
            _ => false,
        };
        if changed {
            self.mark_edited(true);
            return;
        }

        match key {
            egui::Key::ArrowLeft if modifiers.ctrl => {
                self.app.editor.move_word(false, selecting);
                self.app.cat_navigation();
            }
            egui::Key::ArrowRight if modifiers.ctrl => {
                self.app.editor.move_word(true, selecting);
                self.app.cat_navigation();
            }
            egui::Key::ArrowLeft => {
                self.app.editor.move_horizontal(false, selecting);
                self.app.cat_navigation();
            }
            egui::Key::ArrowRight => {
                self.app.editor.move_horizontal(true, selecting);
                self.app.cat_navigation();
            }
            egui::Key::ArrowUp => {
                self.app
                    .editor
                    .move_visual(false, EDITOR_COLUMNS, selecting);
                self.app.cat_navigation();
            }
            egui::Key::ArrowDown => {
                self.app
                    .editor
                    .move_visual(true, EDITOR_COLUMNS, selecting);
                self.app.cat_navigation();
            }
            egui::Key::Home => {
                self.app.editor.visual_home(EDITOR_COLUMNS, selecting);
                self.app.cat_navigation();
            }
            egui::Key::End => {
                self.app.editor.visual_end(EDITOR_COLUMNS, selecting);
                self.app.cat_navigation();
            }
            egui::Key::PageUp => {
                self.app
                    .editor
                    .page_visual(false, PAGE_LINES, EDITOR_COLUMNS, selecting);
                self.app.cat_navigation();
            }
            egui::Key::PageDown => {
                self.app
                    .editor
                    .page_visual(true, PAGE_LINES, EDITOR_COLUMNS, selecting);
                self.app.cat_navigation();
            }
            egui::Key::Escape => self.app.cancel_mode(),
            _ => return,
        }
        self.scroll_cursor_into_view = true;
    }

    fn draw_toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("carta-toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("New").clicked() {
                    self.run_command(Command::NewDocument);
                }
                if ui.button("Creation").clicked() {
                    self.run_command(Command::OpenCreationDateView);
                }
                if ui.button("Modified").clicked() {
                    self.run_command(Command::OpenModificationDateView);
                }
                ui.separator();
                if ui.button("Undo").clicked() {
                    self.run_command(Command::Undo);
                }
                if ui.button("Redo").clicked() {
                    self.run_command(Command::Redo);
                }
                if ui.button("Save").clicked() {
                    self.save_now();
                }
            });
        });
    }

    fn draw_status(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("carta-status").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.monospace(self.view_label());

                if self.app.editor.is_dirty() {
                    ui.label("• modified");
                }
                if self.app.current_document_is_locked() {
                    ui.label("• locked");
                }
                if let AppMode::Leap { session, .. } = &self.app.mode {
                    let direction = match session.direction() {
                        LeapDirection::Backward => "LEAP ←",
                        LeapDirection::Forward => "LEAP →",
                    };
                    ui.monospace(format!("{direction} {}", session.query()));
                }
                if !self.app.status.is_empty() {
                    ui.separator();
                    ui.label(&self.app.status);
                }
            });
        });
    }

    fn view_label(&self) -> String {
        match &self.app.view {
            View::CreationDate(volume) => {
                format!("Creation {:04}-{:02}", volume.year(), volume.month())
            }
            View::ModificationDate => "Modification Date".to_owned(),
            View::Work(id) => self
                .app
                .archive
                .work(*id)
                .map_or_else(|| "Work".to_owned(), |work| format!("Work: {}", work.title())),
            View::Search { query, .. } => format!("Search: {query}"),
            View::History { .. } => "Document History — use TUI for this screen".to_owned(),
            View::WorkHistory { .. } => "Work History — use TUI for this screen".to_owned(),
            View::Trash { .. } => "Trash — use TUI for this screen".to_owned(),
            View::Conflicts { .. } => "Conflicts — use TUI for this screen".to_owned(),
            View::Help { .. } => "Help — use TUI for this screen".to_owned(),
        }
    }

    fn draw_editor(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if !matches!(
                self.app.view,
                View::CreationDate(_)
                    | View::ModificationDate
                    | View::Work(_)
                    | View::Search { .. }
            ) {
                ui.label(
                    "This first GUI experiment only renders editable Carta views.                      Open this screen in the TUI for now.",
                );
                return;
            }

            let font = egui::FontId::monospace(FONT_SIZE);
            let text_color = ui.visuals().text_color();
            let selection_color = ui.visuals().selection.bg_fill;
            let cursor_color = ui.visuals().strong_text_color();
            let character_width = ui
                .fonts(|fonts| fonts.glyph_width(&font, 'M'))
                .max(1.0);
            let editor_width = character_width * EDITOR_COLUMNS as f32;
            let highlight = self
                .app
                .editor
                .selection()
                .or_else(|| self.app.cat_render_highlight());
            let cursor = self.app.editor.cursor();
            let should_scroll = self.scroll_cursor_into_view;

            let (clicked_cursor, scrolled) = {
                let regions = self.app.editor.regions();
                let mut clicked_cursor = None;
                let mut scrolled = false;

                egui::ScrollArea::vertical()
                    .id_salt("carta-editor-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (region_index, region) in regions.iter().enumerate() {
                            let ranges = visual_ranges(&region.text, EDITOR_COLUMNS);
                            for (line_start, line_end) in ranges {
                                let available = ui.available_width();
                                let (row_rect, response) = ui.allocate_exact_size(
                                    egui::vec2(available, ROW_HEIGHT),
                                    egui::Sense::click(),
                                );
                                let left = row_rect.left()
                                    + ((available - editor_width) / 2.0).max(SIDE_PADDING);
                                let top = row_rect.top() + 1.0;
                                let line = &region.text[line_start..line_end];

                                if let Some((start, end)) = highlight {
                                    if let Some((from, to)) = highlight_intersection(
                                        start,
                                        end,
                                        region_index,
                                        line_start,
                                        line_end,
                                    ) {
                                        let x0 = left
                                            + display_width(&region.text[line_start..from])
                                                as f32
                                                * character_width;
                                        let x1 = left
                                            + display_width(&region.text[line_start..to])
                                                as f32
                                                * character_width;
                                        ui.painter().rect_filled(
                                            egui::Rect::from_min_max(
                                                egui::pos2(x0, row_rect.top()),
                                                egui::pos2(x1.max(x0 + 2.0), row_rect.bottom()),
                                            ),
                                            0.0,
                                            selection_color,
                                        );
                                    }
                                }

                                ui.painter().text(
                                    egui::pos2(left, top),
                                    egui::Align2::LEFT_TOP,
                                    line,
                                    font.clone(),
                                    text_color,
                                );

                                if cursor.region == region_index
                                    && cursor.byte >= line_start
                                    && cursor.byte <= line_end
                                {
                                    let x = left
                                        + display_width(
                                            &region.text[line_start..cursor.byte.min(line_end)],
                                        ) as f32
                                            * character_width;
                                    let cursor_rect = egui::Rect::from_min_max(
                                        egui::pos2(x, row_rect.top() + 2.0),
                                        egui::pos2(x + 1.5, row_rect.bottom() - 2.0),
                                    );
                                    ui.painter().rect_filled(cursor_rect, 0.0, cursor_color);
                                    if should_scroll && !scrolled {
                                        ui.scroll_to_rect(row_rect, Some(egui::Align::Center));
                                        scrolled = true;
                                    }
                                }

                                if response.clicked() {
                                    if let Some(pointer) = response.interact_pointer_pos() {
                                        let column =
                                            ((pointer.x - left).max(0.0) / character_width).round()
                                                as usize;
                                        clicked_cursor = Some(Cursor {
                                            region: region_index,
                                            byte: byte_at_display_column(
                                                &region.text,
                                                line_start,
                                                line_end,
                                                column,
                                            ),
                                        });
                                    }
                                }
                            }

                            if region_index + 1 < regions.len() {
                                ui.add_space(ROW_HEIGHT * 0.6);
                                ui.separator();
                                ui.add_space(ROW_HEIGHT * 0.6);
                            }
                        }
                    });

                (clicked_cursor, scrolled)
            };

            if let Some(cursor) = clicked_cursor {
                self.app.editor.set_cursor(cursor, false);
                self.app.cat_navigation();
                self.scroll_cursor_into_view = true;
            } else if scrolled {
                self.scroll_cursor_into_view = false;
            }
        });
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_events(ctx);

        if let Err(error) = self.app.tick(Instant::now()) {
            self.set_error(error);
        }
        self.save_session_if_due();

        self.draw_toolbar(ctx);
        self.draw_status(ctx);
        self.draw_editor(ctx);

        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

impl Drop for GuiApp {
    fn drop(&mut self) {
        let _ = self.app.autosave();
        let session = self.app.session();
        let _ = save_session(&self.data_root, self.archive_id, &session);
        let _ = self.app.archive.sync();
    }
}

fn highlight_intersection(
    start: Cursor,
    end: Cursor,
    region: usize,
    line_start: usize,
    line_end: usize,
) -> Option<(usize, usize)> {
    if region < start.region || region > end.region {
        return None;
    }

    let from = if region == start.region {
        start.byte.max(line_start)
    } else {
        line_start
    };
    let to = if region == end.region {
        end.byte.min(line_end)
    } else {
        line_end
    };
    (from < to).then_some((from, to))
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

fn byte_at_display_column(
    text: &str,
    line_start: usize,
    line_end: usize,
    target_column: usize,
) -> usize {
    let line = &text[line_start..line_end];
    let mut column = 0;
    for (relative, character) in line.char_indices() {
        let width = character.width().unwrap_or(0);
        if column + width / 2 >= target_column {
            return line_start + relative;
        }
        column += width;
        if column >= target_column {
            return line_start + relative + character.len_utf8();
        }
    }
    line_end
}

fn open_archive(args: &Args) -> Result<Archive, Box<dyn Error>> {
    let explicit = args.path.is_some();
    let path = match &args.path {
        Some(path) => path.clone(),
        None => default_archive_path()?,
    };

    if args.create {
        return Ok(Archive::create(&path)?);
    }

    if !explicit && path_is_missing(&path)? {
        return Err(format!(
            "No Carta Space Archive found at {}. Initialize or import the default archive with the TUI first.",
            path.display()
        )
        .into());
    }

    Ok(Archive::open(&path)?)
}

fn path_is_missing(path: &Path) -> std::io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let data = data_root()?;
    fs::create_dir_all(&data)?;
    migrate_legacy_state(&data)?;

    let archive = open_archive(&args)?;
    let archive_id = archive.metadata().archive_id();
    let session = load_session(&data, archive_id)?;
    let app = App::open(archive, session.as_ref(), Instant::now())?;

    let gui = GuiApp::new(app, data, archive_id, session);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 760.0])
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Carta Space",
        options,
        Box::new(move |_creation_context| Ok(Box::new(gui))),
    )?;
    Ok(())
}
