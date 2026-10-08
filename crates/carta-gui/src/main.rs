mod input;
mod presentation;
mod ui;

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
            default_font: Font::MONOSPACE,
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
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Raw(Event),
    Tick(Instant),
    SystemTheme(ThemeMode),
}

impl Gui {
    fn boot() -> (Self, Task<Message>) {
        (
            Self::load(),
            system::theme().map(Message::SystemTheme),
        )
    }

    fn load() -> Self {
        let Some(path) = env::args_os().nth(1).map(PathBuf::from) else {
            return Self {
                app: None,
                input: GuiInputState::default(),
                error: Some("Usage: carta-gui <archive-path>".into()),
                theme_mode: ThemeMode::Dark,
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
            },
            Err(error) => Self {
                app: None,
                input: GuiInputState::default(),
                error: Some(error),
                theme_mode: ThemeMode::Dark,
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
        iced::time::every(Duration::from_millis(250)).map(Message::Tick),
        system::theme_changes().map(Message::SystemTheme),
    ])
}

fn update(state: &mut Gui, message: Message) -> Task<Message> {
    if let Message::SystemTheme(mode) = message {
        state.theme_mode = mode;
        return Task::none();
    }

    let Some(app) = &mut state.app else {
        return Task::none();
    };

    let result = match message {
        Message::Raw(Event::Keyboard(event)) => state.input.handle(app, event),
        Message::Tick(now) => app.tick(now).map(|_| ()),
        Message::Raw(_) | Message::SystemTheme(_) => Ok(()),
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
