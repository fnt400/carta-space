mod input;
mod layout;
mod markdown;
mod profile;
mod session;
mod ui;
mod viewport;

use base64::Engine as _;
use carta_app::{App, AppMode, Session, View};
use carta_core::Archive;
use iced::event::{self, Status};
use iced::theme::Mode as ThemeMode;
use iced::widget::operation::{self, AbsoluteOffset};
use iced::{system, window, Event, Font, Point, Settings, Size, Subscription, Task, Theme};
use input::{ClipboardRequest, GuiInputState};
use std::cell::RefCell;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() -> iced::Result {
    iced::application(Gui::boot, update, ui::view)
        .title("Carta Space")
        .theme(theme)
        .settings(Settings {
            default_font: Font::with_name("Iosevka"),
            ..Settings::default()
        })
        .subscription(subscription)
        .run()
}

pub(crate) struct Gui {
    pub(crate) app: Option<App>,
    pub(crate) input: GuiInputState,
    pub(crate) error: Option<String>,
    pub(crate) theme_mode: ThemeMode,
    pub(crate) window_size: Size,
    pub(crate) font_size: f32,
    pub(crate) markdown: RefCell<markdown::Cache>,
    pub(crate) layout: RefCell<layout::Layout>,
    pub(crate) scroll_y: f32,
    session_root: Option<PathBuf>,
    last_saved_session: Option<Session>,
    pointer: Option<Point>,
    pointer_down: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Raw(Event),
    Tick(Instant),
    SystemTheme(ThemeMode),
    FontLoaded(bool),
    WindowOpened(window::Id),
    WindowSize(Size),
    PasteText(Option<String>),
    PointerMoved(Point),
    PointerPressed,
    PointerReleased,
    Scrolled(f32),
}

impl Gui {
    fn boot() -> (Self, Task<Message>) {
        // Keep the desktop appearance independent of installed system fonts.
        let font_bytes = base64::engine::general_purpose::STANDARD
            .decode(include_str!("../assets/iosevka-regular.ttf.b64").trim())
            .expect("bundled Iosevka font must be valid base64");
        (
            Self::load(),
            Task::batch([
                system::theme().map(Message::SystemTheme),
                iced::font::load(font_bytes).map(|result| Message::FontLoaded(result.is_ok())),
            ]),
        )
    }

    fn load() -> Self {
        let missing = || Self {
            app: None,
            input: GuiInputState::default(),
            error: Some("Usage: carta-gui <archive-path>".into()),
            theme_mode: ThemeMode::Dark,
            window_size: Size::new(1024.0, 768.0),
            font_size: viewport::FONT_SIZE,
            markdown: RefCell::new(markdown::Cache::default()),
            layout: RefCell::new(layout::Layout::default()),
            scroll_y: 0.0,
            session_root: None,
            last_saved_session: None,
            pointer: None,
            pointer_down: false,
        };
        let Some(path) = env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .or_else(|| session::default_archive_path().ok())
        else {
            return missing();
        };

        let root = session::data_root().ok();
        let result = Archive::open(&path)
            .map_err(|error| error.to_string())
            .and_then(|archive| {
                let previous = root.as_deref().and_then(|root| {
                    session::load(root, archive.metadata().archive_id())
                        .ok()
                        .flatten()
                });
                App::open(archive, previous.as_ref(), Instant::now())
                    .map(|app| (app, previous))
                    .map_err(|error| error.to_string())
            });

        let (app, error, previous) = match result {
            Ok((app, previous)) => (Some(app), None, previous),
            Err(error) => (None, Some(error), None),
        };
        Self {
            app,
            input: GuiInputState::default(),
            error,
            theme_mode: ThemeMode::Dark,
            window_size: Size::new(1024.0, 768.0),
            font_size: viewport::FONT_SIZE,
            markdown: RefCell::new(markdown::Cache::default()),
            layout: RefCell::new(layout::Layout::default()),
            scroll_y: 0.0,
            session_root: root,
            last_saved_session: previous,
            pointer: None,
            pointer_down: false,
        }
    }

    fn save_session(&mut self) -> std::io::Result<()> {
        let (Some(root), Some(app)) = (&self.session_root, &self.app) else {
            return Ok(());
        };
        if app.kill_switch_triggered() {
            return Ok(());
        }
        let current = app.session();
        if self.last_saved_session.as_ref() != Some(&current) {
            session::save(root, app.archive.metadata().archive_id(), &current)?;
            self.last_saved_session = Some(current);
        }
        Ok(())
    }
}

impl Drop for Gui {
    fn drop(&mut self) {
        if let Some(app) = &mut self.app {
            if !app.kill_switch_triggered() {
                if let Err(error) = app.autosave() {
                    eprintln!("carta-gui: final autosave failed: {error}");
                }
            }
        }
        if let Err(error) = self.save_session() {
            eprintln!("carta-gui: final session save failed: {error}");
        }
    }
}

fn subscription(state: &Gui) -> Subscription<Message> {
    let keyboard = event::listen_raw(|event, _status: Status, _window: window::Id| {
        matches!(event, Event::Keyboard(_)).then_some(Message::Raw(event))
    });

    // Avoid periodic reconstruction of the entire rich-text view while idle.
    // Dirty buffers are saved promptly; transient statuses still expire; an
    // otherwise idle window needs only infrequent local maintenance.
    let interval = if state.app.as_ref().is_some_and(|app| app.editor.is_dirty()) {
        Duration::from_secs(1)
    } else if state.app.as_ref().is_some_and(|app| !app.status.is_empty()) {
        Duration::from_secs(2)
    } else {
        Duration::from_secs(15)
    };
    // A mouse drag can end outside the text sheet; release the selection
    // state even if the sheet's mouse area does not receive the event.
    let mouse_release = event::listen_raw(|event, _status, _window| {
        matches!(
            event,
            Event::Mouse(iced::mouse::Event::ButtonReleased(
                iced::mouse::Button::Left
            ))
        )
        .then_some(Message::PointerReleased)
    });
    Subscription::batch([
        keyboard,
        mouse_release,
        iced::time::every(interval).map(Message::Tick),
        window::open_events().map(Message::WindowOpened),
        window::resize_events().map(|(_id, size)| Message::WindowSize(size)),
        system::theme_changes().map(Message::SystemTheme),
    ])
}

fn scroll_to_caret(state: &mut Gui) -> Task<Message> {
    let Some(app) = &state.app else { return Task::none(); };
    let started = profile::enabled().then(Instant::now);
    let mut layout = state.layout.borrow_mut();
    layout.sync(app, viewport::columns(state.window_size.width, state.font_size));
    let row = layout.caret_row(app);
    let offset = viewport::VERTICAL_PADDING + row as f32 * viewport::line_height(state.font_size);
    drop(layout);
    state.scroll_y = offset;
    if let Some(started) = started {
        profile::record("scroll", started);
    }
    operation::scroll_to(
        ui::EDITOR_SCROLL_ID,
        AbsoluteOffset { x: None, y: Some(offset) },
    )
}

/// Map pointer coordinates in the *rendered window* to the Archive's
/// canonical insertion point. No scanning over preceding Documents.
fn indexed_hit_test(
    app: &App,
    cache: &RefCell<layout::Layout>,
    scroll_y: f32,
    point: Point,
    window: Size,
    font_size: f32,
) -> Option<carta_app::Cursor> {
    let line_height = viewport::line_height(font_size);
    let height = viewport::writing_area_height(window.height);
    let top = height * (2.0 / 3.0) + viewport::VERTICAL_PADDING;
    let mut layout = cache.borrow_mut();
    layout.sync(app, viewport::columns(window.width, font_size));
    let (first, _) = layout.window(scroll_y, height, line_height, top);
    let row = first.saturating_add((point.y.max(0.0) / line_height) as usize);
    let left = (window.width - window.width.min(viewport::PAGE_WIDTH)) / 2.0
        + viewport::HORIZONTAL_PADDING;
    let col = ((point.x - left).max(0.0) / viewport::mono_advance(font_size))
        .round() as usize;
    layout.hit_test(app, row, col)
}

fn normalize_clipboard(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn update(state: &mut Gui, message: Message) -> Task<Message> {
    match message {
        Message::Scrolled(offset) => {
            // Native wheel and scrollbar position remains the source of truth.
            // Only the bounded visible-row window is recomputed.
            state.scroll_y = offset.max(0.0);
            return Task::none();
        }
        Message::PointerMoved(point) => {
            state.pointer = Some(point);
            if state.pointer_down {
                if let Some(app) = &mut state.app {
                    if matches!(app.mode, AppMode::Editing)
                        && !app.collapsed
                        && matches!(
                            app.view,
                            View::CreationDate(_) | View::ModificationDate | View::Work(_)
                        )
                    {
                        if let Some(cursor) =
                            indexed_hit_test(app, &state.layout, state.scroll_y, point, state.window_size, state.font_size)
                        {
                            if cursor != app.editor.cursor() {
                                app.editor.set_cursor(cursor, true);
                            }
                        }
                    }
                }
            }
            return Task::none();
        }
        Message::PointerPressed => {
            let Some(point) = state.pointer else {
                return Task::none();
            };
            if let Some(app) = &mut state.app {
                if matches!(app.mode, AppMode::Editing)
                    && !app.collapsed
                    && matches!(
                        app.view,
                        View::CreationDate(_) | View::ModificationDate | View::Work(_)
                    )
                {
                    if let Some(cursor) =
                        indexed_hit_test(app, &state.layout, state.scroll_y, point, state.window_size, state.font_size)
                    {
                        app.cat_navigation();
                        app.editor.set_cursor(cursor, false);
                        state.pointer_down = true;
                    }
                }
            }
            return Task::none();
        }
        Message::PointerReleased => {
            state.pointer_down = false;
            return Task::none();
        }
        Message::Tick(now) => {
            if let Some(app) = &mut state.app {
                let started = profile::enabled().then(Instant::now);
                if let Err(error) = app.tick_without_remote_sync(now) {
                    app.status = format!("Error: {error}");
                }
                if let Some(started) = started {
                    profile::record("maintenance", started);
                }
            }
            if let Err(error) = state.save_session() {
                if let Some(app) = &mut state.app {
                    app.status = format!("Session save: {error}");
                }
            }
            return Task::none();
        }
        Message::SystemTheme(mode) => {
            state.theme_mode = mode;
            return Task::none();
        }
        Message::FontLoaded(false) => {
            if let Some(app) = &mut state.app {
                app.status = "Bundled Iosevka font could not be loaded".into();
            }
            return Task::none();
        }
        Message::FontLoaded(true) => return scroll_to_caret(state),
        Message::WindowOpened(id) => {
            return window::size(id).map(Message::WindowSize);
        }
        Message::WindowSize(size) => {
            state.window_size = size;
            return scroll_to_caret(state);
        }
        Message::PasteText(value) => {
            if let Some(app) = &mut state.app {
                if matches!(app.mode, carta_app::AppMode::Editing) {
                    if let Some(value) = value {
                        let value = normalize_clipboard(&value);
                        if !value.is_empty() {
                            app.dispatch_action(
                                carta_app::Action::InsertText(value),
                                Instant::now(),
                            );
                            return scroll_to_caret(state);
                        }
                    }
                    app.status = "No text available in system clipboard".into();
                }
            }
            return Task::none();
        }
        _ => {}
    }

    let columns = viewport::columns(state.window_size.width, state.font_size);
    let Some(app) = &mut state.app else {
        return Task::none();
    };
    let keyboard_event = matches!(message, Message::Raw(Event::Keyboard(_)));
    let before_cursor = app.editor.cursor();
    let before_region_count = app.editor.regions().len();
    let before_view = std::mem::discriminant(&app.view);
    let mut clipboard = Task::none();
    let mut zoomed = false;
    let result = match message {
        Message::Raw(Event::Keyboard(event)) => {
            let started = profile::enabled().then(Instant::now);
            let result = state.input.handle(app, event, columns);
            let zoom = state.input.take_zoom_request();
            if zoom != 0 {
                let next = (state.font_size + f32::from(zoom) * viewport::FONT_STEP)
                    .clamp(viewport::MIN_FONT_SIZE, viewport::MAX_FONT_SIZE);
                zoomed = next != state.font_size;
                state.font_size = next;
                app.status = format!("Writing font: {next:.0} px");
            }
            if let Some(started) = started {
                profile::record("input", started);
            }
            clipboard = match state.input.take_clipboard_request() {
                Some(ClipboardRequest::Read) => iced::clipboard::read().map(Message::PasteText),
                Some(ClipboardRequest::Write(text)) => iced::clipboard::write(text),
                None => Task::none(),
            };
            result
        }
        _ => Ok(()),
    };
    if let Err(error) = result {
        app.status = format!("Error: {error}");
    }
    let follow_caret = zoomed
        || (keyboard_event
            && (app.editor.cursor() != before_cursor
                || app.editor.regions().len() != before_region_count
                || std::mem::discriminant(&app.view) != before_view));
    if app.quit {
        iced::exit()
    } else if follow_caret {
        Task::batch([clipboard, scroll_to_caret(state)])
    } else {
        clipboard
    }
}

fn theme(state: &Gui) -> Theme {
    match state.theme_mode {
        ThemeMode::Light => Theme::Light,
        ThemeMode::Dark | ThemeMode::None => Theme::Dark,
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_clipboard;

    #[test]
    fn pasted_line_endings_are_normalized() {
        assert_eq!(normalize_clipboard("a\r\nb\rc\n"), "a\nb\nc\n");
    }
}
