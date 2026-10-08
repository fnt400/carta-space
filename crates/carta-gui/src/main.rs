mod input;
mod presentation;
mod ui;
mod viewport;

use base64::Engine as _;
use carta_app::App;
use carta_core::Archive;
use iced::event::{self, Status};
use iced::theme::Mode as ThemeMode;
use iced::{system, window, Event, Font, Settings, Size, Subscription, Task, Theme};
use iced::widget::operation::{self, AbsoluteOffset};
use input::{ClipboardRequest, GuiInputState};
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
    pub(crate) caret_visible: bool,
    pub(crate) window_size: Size,
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
        let Some(path) = env::args_os().nth(1).map(PathBuf::from) else {
            return Self {
                app: None,
                input: GuiInputState::default(),
                error: Some("Usage: carta-gui <archive-path>".into()),
                theme_mode: ThemeMode::Dark,
                caret_visible: true,
                window_size: Size::new(1024.0, 768.0),
            };
        };

        match Archive::open(&path)
            .map_err(|error| error.to_string())
            .and_then(|archive| {
                App::open(archive, None, Instant::now()).map_err(|error| error.to_string())
            }) {
            Ok(app) => Self {
                app: Some(app),
                input: GuiInputState::default(),
                error: None,
                theme_mode: ThemeMode::Dark,
                caret_visible: true,
                window_size: Size::new(1024.0, 768.0),
            },
            Err(error) => Self {
                app: None,
                input: GuiInputState::default(),
                error: Some(error),
                theme_mode: ThemeMode::Dark,
                caret_visible: true,
                window_size: Size::new(1024.0, 768.0),
            },
        }
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
    }
}

fn subscription(_state: &Gui) -> Subscription<Message> {
    let keyboard = event::listen_raw(|event, _status: Status, _window: window::Id| {
        matches!(event, Event::Keyboard(_)).then_some(Message::Raw(event))
    });

    Subscription::batch([
        keyboard,
        // No animation loop: only maintain autosave/status on a low-frequency timer.
        iced::time::every(Duration::from_secs(2)).map(Message::Tick),
        window::open_events().map(Message::WindowOpened),
        window::resize_events().map(|(_id, size)| Message::WindowSize(size)),
        system::theme_changes().map(Message::SystemTheme),
    ])
}

fn scroll_to_caret(state: &Gui) -> Task<Message> {
    let Some(app) = &state.app else {
        return Task::none();
    };
    let offset = viewport::scroll_offset(
        app,
        state.window_size.width,
        state.window_size.height,
    );
    operation::scroll_to(
        ui::EDITOR_SCROLL_ID,
        AbsoluteOffset { x: None, y: Some(offset) },
    )
}

fn normalize_clipboard(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn update(state: &mut Gui, message: Message) -> Task<Message> {
    match message {
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

    let columns = viewport::columns(state.window_size.width);
    let Some(app) = &mut state.app else {
        return Task::none();
    };
    let keyboard_event = matches!(message, Message::Raw(Event::Keyboard(_)));
    let mut clipboard = Task::none();
    let result = match message {
        Message::Raw(Event::Keyboard(event)) => {
            let result = state.input.handle(app, event, columns);
            clipboard = match state.input.take_clipboard_request() {
                Some(ClipboardRequest::Read) => iced::clipboard::read().map(Message::PasteText),
                Some(ClipboardRequest::Write(text)) => iced::clipboard::write(text),
                None => Task::none(),
            };
            result
        }
        Message::Tick(now) => app.tick(now).map(|_| ()),
        _ => Ok(()),
    };
    if let Err(error) = result {
        app.status = format!("Error: {error}");
    }
    if app.quit {
        iced::exit()
    } else if keyboard_event {
        Task::batch([clipboard, scroll_to_caret(state)])
    } else {
        clipboard
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

fn theme(state: &Gui) -> Theme {
    match state.theme_mode {
        ThemeMode::Light => Theme::Light,
        ThemeMode::Dark | ThemeMode::None => Theme::Dark,
    }
}
