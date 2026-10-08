mod input;
mod presentation;
mod ui;

use base64::Engine as _;
use carta_app::App;
use carta_core::Archive;
use iced::event::{self, Status};
use iced::theme::Mode as ThemeMode;
use iced::{system, window, Event, Font, Settings, Subscription, Task, Theme};
use input::GuiInputState;
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
    pub(crate) blink_origin: Instant,
    pub(crate) caret_visible: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Raw(Event),
    Tick(Instant),
    SystemTheme(ThemeMode),
    FontLoaded(bool),
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
                blink_origin: Instant::now(),
                caret_visible: true,
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
                blink_origin: Instant::now(),
                caret_visible: true,
            },
            Err(error) => Self {
                app: None,
                input: GuiInputState::default(),
                error: Some(error),
                theme_mode: ThemeMode::Dark,
                blink_origin: Instant::now(),
                caret_visible: true,
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
        iced::time::every(Duration::from_millis(83)).map(Message::Tick),
        system::theme_changes().map(Message::SystemTheme),
    ])
}

fn update(state: &mut Gui, message: Message) -> Task<Message> {
    if let Message::SystemTheme(mode) = message {
        state.theme_mode = mode;
        return Task::none();
    }
    if let Message::FontLoaded(success) = message {
        if !success {
            if let Some(app) = &mut state.app {
                app.status = "Bundled Iosevka font could not be loaded".into();
            }
        }
        return Task::none();
    }

    let Some(app) = &mut state.app else {
        return Task::none();
    };

    let result = match message {
        Message::Raw(Event::Keyboard(event)) => {
            state.blink_origin = Instant::now();
            state.caret_visible = true;
            state.input.handle(app, event)
        }
        Message::Tick(now) => {
            // Canon Cat: about 3 Hz when saved, about 1 Hz when dirty.
            let half_period_ms = if app.editor.is_dirty() { 500 } else { 167 };
            state.caret_visible =
                (now.saturating_duration_since(state.blink_origin).as_millis()
                    / half_period_ms) % 2 == 0;
            app.tick(now).map(|_| ())
        },
        Message::Raw(_) | Message::SystemTheme(_) | Message::FontLoaded(_) => Ok(()),
    };

    if let Err(error) = result {
        app.status = format!("Error: {error}");
    }

    if app.quit {
        iced::exit()
    } else {
        Task::none()
    }
}

fn theme(state: &Gui) -> Theme {
    match state.theme_mode {
        ThemeMode::Light => Theme::Light,
        ThemeMode::Dark | ThemeMode::None => Theme::Dark,
    }
}
