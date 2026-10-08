mod input;
mod presentation;

use carta_app::{App, AppMode};
use carta_core::Archive;
use iced::event::{self, Status};
use iced::widget::{column, container, rich_text, row, scrollable, span, text};
use iced::{color, window, Element, Event, Length, Subscription, Task, Theme};
use input::GuiInputState;
use presentation::SegmentKind;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() -> iced::Result {
    iced::application(Gui::load, update, view)
        .title("Carta Space")
        .theme(|_| Theme::Light)
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
    Tick(Instant),
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
    // App::tick owns the shared autosave, checkpoint and sync schedule.
    // Without an idle timer, a GUI that stops receiving keys never saves.
    Subscription::batch([
        keyboard,
        iced::time::every(Duration::from_millis(250)).map(Message::Tick),
    ])
}

fn update(state: &mut Gui, message: Message) -> Task<Message> {
    let Some(app) = &mut state.app else {
        return Task::none();
    };
    let result = match message {
        Message::Raw(Event::Keyboard(event)) => state.input.handle(app, event),
        Message::Tick(now) => app.tick(now).map(|_| ()),
        _ => Ok(()),
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

    let archive = state
        .archive_path
        .as_ref()
        .map_or_else(String::new, |path| path.display().to_string());
    let mode = mode_label(&app.mode);
    let current = app.editor.cursor();
    let document = app.editor.current_text().unwrap_or("");
    let selection = app
        .cat_render_highlight()
        .or_else(|| app.editor.selection())
        .and_then(|(start, end)| {
            (start.region == current.region && end.region == current.region)
                .then_some((start.byte, end.byte))
        });
    let spans: Vec<iced::widget::text::Span<'_, ()>> =
        presentation::segments(document, current.byte, selection)
            .into_iter()
            .map(|segment| match segment.kind {
                SegmentKind::Text => span(segment.content),
                SegmentKind::Selection => span(segment.content)
                    .background(color!(0xBFD5F2))
                    .color(color!(0x182C45)),
                SegmentKind::Caret => span("■■").color(color!(0xB54835)),
            })
            .collect();

    let editor = scrollable(
        container(rich_text(spans).size(19).width(Length::Fill))
            .padding([34, 42])
            .width(Length::Fill)
            .style(iced::widget::container::rounded_box),
    )
    .height(Length::Fill)
    .width(Length::Fill);

    let save_state = if app.editor.is_dirty() {
        "Unsaved"
    } else {
        "Saved"
    };
    let header = row![
        text("Carta").size(18),
        text(format!("{mode} · {save_state}")).size(12),
    ]
    .spacing(18);

    let context = text(archive).size(11);
    let status = if app.status.is_empty() {
        text("Esc · Commands     Left Ctrl / Left Alt · LEAP     Right Ctrl · Again").size(12)
    } else {
        text(&app.status).size(12)
    };

    let body = column![
        header,
        context,
        editor,
        mode_panel(app),
        status,
    ]
    .spacing(10)
    .padding([14, 18])
    .height(Length::Fill);

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn mode_panel(app: &App) -> Element<'_, Message> {
    match &app.mode {
        AppMode::Editing => text("").size(1).into(),
        AppMode::Palette { query, selected } => {
            let commands = app.palette_commands(query);
            let mut panel = column![text(format!("Commands  › {query}")).size(18)].spacing(4);
            if commands.is_empty() {
                panel = panel.push(text("No matching commands").size(14));
            }
            for (index, command) in commands
                .iter()
                .enumerate()
                .skip(selected.saturating_sub(5))
                .take(11)
            {
                let marker = if index == *selected { "▶" } else { " " };
                panel = panel.push(text(format!("{marker} {}", command.label())).size(15));
            }
            container(panel)
                .padding(14)
                .width(Length::Fill)
                .style(iced::widget::container::rounded_box)
                .into()
        }
        AppMode::Prompt {
            title,
            input,
            cursor,
            details,
            ..
        } => {
            let mut panel = column![text(title).size(18)].spacing(5);
            for detail in details {
                panel = panel.push(text(detail).size(14));
            }
            panel = panel.push(text(format!("› {}", with_caret(input, *cursor))).size(16));
            container(panel)
                .padding(14)
                .width(Length::Fill)
                .style(iced::widget::container::rounded_box)
                .into()
        }
        AppMode::Confirm { title, details, .. } => {
            let mut panel = column![text(title).size(18)].spacing(5);
            for detail in details {
                panel = panel.push(text(detail).size(14));
            }
            panel = panel.push(text("Y · Confirm      N / Esc · Cancel").size(14));
            container(panel)
                .padding(14)
                .width(Length::Fill)
                .style(iced::widget::container::rounded_box)
                .into()
        }
        AppMode::Selector {
            title,
            query,
            selected,
            choices,
            ..
        } => {
            let matches: Vec<_> = choices
                .iter()
                .filter(|choice| carta_app::palette::matches(query, &choice.label))
                .collect();
            let mut panel = column![text(format!("{title}  › {query}")).size(18)].spacing(4);
            if matches.is_empty() {
                panel = panel.push(text("No matching items").size(14));
            }
            for (index, choice) in matches
                .iter()
                .enumerate()
                .skip(selected.saturating_sub(5))
                .take(11)
            {
                let marker = if index == *selected { "▶" } else { " " };
                panel = panel.push(text(format!("{marker} {}", choice.label)).size(15));
            }
            container(panel)
                .padding(14)
                .width(Length::Fill)
                .style(iced::widget::container::rounded_box)
                .into()
        }
        AppMode::Leap { session, .. } => {
            let direction = match session.direction() {
                carta_core::LeapDirection::Forward => "Forward",
                carta_core::LeapDirection::Backward => "Backward",
            };
            container(text(format!("LEAP {direction}  › {}", session.query())).size(16))
                .padding(12)
                .width(Length::Fill)
                .style(iced::widget::container::rounded_box)
                .into()
        }
    }
}

fn with_caret(input: &str, cursor: usize) -> String {
    let mut cursor = cursor.min(input.len());
    while !input.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let mut value = input.to_owned();
    value.insert(cursor, '│');
    value
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_caret_is_utf8_safe() {
        assert_eq!(with_caret("caffè", 4), "caff│è");
        assert_eq!(with_caret("é", 1), "│é");
        assert_eq!(with_caret("é", 2), "é│");
    }
}
