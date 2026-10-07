mod input;

use carta_app::{App, AppMode};
use carta_core::Archive;
use iced::event::{self, Status};
use iced::widget::{column, container, scrollable, text};
use iced::{window, Element, Event, Length, Subscription};
use input::GuiInputState;
use std::env;
use std::path::PathBuf;
use std::time::Instant;

fn main() -> iced::Result {
    iced::application(Gui::load, update, view)
        .title("Carta Space")
        .subscription(subscription)
        .run()
}

struct Gui {
    app: Option<App>,
    input: GuiInputState,
    error: Option<String>,
    archive_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
enum Message {
    Raw(Event),
}

impl Gui {
    fn load() -> Self {
        let Some(path) = env::args_os().nth(1).map(PathBuf::from) else {
            return Self {
                app: None,
                input: GuiInputState::default(),
                error: Some("Usage: carta-gui <archive-path>".into()),
                archive_path: None,
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
                archive_path: Some(path),
            },
            Err(error) => Self {
                app: None,
                input: GuiInputState::default(),
                error: Some(error),
                archive_path: Some(path),
            },
        }
    }
}

impl Drop for Gui {
    fn drop(&mut self) {
        if let Some(app) = &mut self.app {
            let _ = app.autosave();
        }
    }
}

fn subscription(_state: &Gui) -> Subscription<Message> {
    event::listen_raw(|event, _status: Status, _window: window::Id| {
        matches!(event, Event::Keyboard(_)).then_some(Message::Raw(event))
    })
}

fn update(state: &mut Gui, message: Message) {
    let Message::Raw(Event::Keyboard(event)) = message else {
        return;
    };
    let Some(app) = &mut state.app else {
        return;
    };

    let _ = state.input.handle(app, event);
    let _ = app.tick(Instant::now());
}

fn view(state: &Gui) -> Element<'_, Message> {
    if let Some(error) = &state.error {
        return container(
            column![
                text("Carta Space").size(32),
                text("The graphical shell could not open the Archive."),
                text(error),
            ]
            .spacing(12)
            .padding(24),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }

    let Some(app) = &state.app else {
        return container(text("Carta Space")).into();
    };

    let document = app.editor.current_text().unwrap_or("");
    let mode = mode_label(&app.mode);
    let archive = state
        .archive_path
        .as_ref()
        .map_or_else(String::new, |path| path.display().to_string());

    let body = column![
        text("Carta Space").size(28),
        text(format!("{archive} · {mode}")).size(13),
        scrollable(
            container(text(document).size(19))
                .padding(24)
                .width(Length::Fill)
        )
        .height(Length::Fill)
        .width(Length::Fill),
        text(&app.status).size(13),
    ]
    .spacing(8)
    .padding(16)
    .height(Length::Fill);

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn mode_label(mode: &AppMode) -> &'static str {
    match mode {
        AppMode::Editing => "Editing",
        AppMode::Palette { .. } => "Palette",
        AppMode::Prompt { .. } => "Prompt",
        AppMode::Confirm { .. } => "Confirm",
        AppMode::Selector { .. } => "Selector",
        AppMode::Leap { .. } => "LEAP",
    }
}
