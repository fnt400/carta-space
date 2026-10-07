use std::collections::VecDeque;

use iced::advanced::input_method::Event as InputMethodEvent;
use iced::event::{self, Status};
use iced::keyboard::key::{Code, Physical};
use iced::keyboard::Event as KeyboardEvent;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Element, Event, Length, Subscription, window};

const MAX_LOG_LINES: usize = 160;

pub fn main() -> iced::Result {
    iced::application(Probe::default, update, view)
        .title("Carta Space · Iced input probe")
        .subscription(subscription)
        .run()
}

#[derive(Debug, Default)]
struct Probe {
    left_control: bool,
    right_control: bool,
    left_alt: bool,
    right_alt: bool,
    text_value: String,
    ime_preedit: String,
    last_ime_commit: String,
    log: VecDeque<String>,
}

#[derive(Debug, Clone)]
enum Message {
    Raw(ProbeEvent),
    TextChanged(String),
    ClearLog,
}

#[derive(Debug, Clone)]
enum ProbeEvent {
    Key {
        line: String,
        tracked: Option<TrackedKey>,
        pressed: bool,
    },
    Log(String),
    ImePreedit {
        value: String,
        selection: String,
    },
    ImeCommit(String),
    ImeClosed,
}

#[derive(Debug, Clone, Copy)]
enum TrackedKey {
    LeftControl,
    RightControl,
    LeftAlt,
    RightAlt,
}

fn subscription(_state: &Probe) -> Subscription<Message> {
    event::listen_raw(raw_event)
}

fn raw_event(event: Event, status: Status, _window: window::Id) -> Option<Message> {
    match event {
        Event::Keyboard(event) => Some(Message::Raw(keyboard_event(event, status))),
        Event::InputMethod(event) => Some(Message::Raw(input_method_event(event, status))),
        _ => None,
    }
}

fn keyboard_event(event: KeyboardEvent, status: Status) -> ProbeEvent {
    let status = status_name(status);

    match event {
        KeyboardEvent::KeyPressed {
            physical_key,
            location,
            modifiers,
            text,
            repeat,
            ..
        } => {
            let tracked = tracked_key(&physical_key);
            let line = if let Some(key) = tracked {
                format!(
                    "{} DOWN physical={physical_key:?} location={location:?} modifiers={modifiers:?} repeat={repeat} text={:?} status={status}",
                    tracked_name(key),
                    text.as_deref()
                )
            } else {
                format!(
                    "KEY DOWN physical={physical_key:?} location={location:?} modifiers={modifiers:?} repeat={repeat} text={:?} status={status}",
                    text.as_deref()
                )
            };
            ProbeEvent::Key {
                line,
                tracked,
                pressed: true,
            }
        }
        KeyboardEvent::KeyReleased {
            physical_key,
            location,
            modifiers,
            ..
        } => {
            let tracked = tracked_key(&physical_key);
            let line = if let Some(key) = tracked {
                format!(
                    "{} UP   physical={physical_key:?} location={location:?} modifiers={modifiers:?} status={status}",
                    tracked_name(key)
                )
            } else {
                format!(
                    "KEY UP   physical={physical_key:?} location={location:?} modifiers={modifiers:?} status={status}"
                )
            };
            ProbeEvent::Key {
                line,
                tracked,
                pressed: false,
            }
        }
        KeyboardEvent::ModifiersChanged(modifiers) => {
            ProbeEvent::Log(format!("MODIFIERS {modifiers:?} status={status}"))
        }
    }
}

fn input_method_event(event: InputMethodEvent, status: Status) -> ProbeEvent {
    let status = status_name(status);

    match event {
        InputMethodEvent::Opened => ProbeEvent::Log(format!("IME OPENED status={status}")),
        InputMethodEvent::Preedit(value, selection) => ProbeEvent::ImePreedit {
            value,
            selection: format!("{selection:?} status={status}"),
        },
        InputMethodEvent::Commit(value) => {
            ProbeEvent::ImeCommit(format!("{value}\nstatus={status}"))
        }
        InputMethodEvent::Closed => ProbeEvent::ImeClosed,
    }
}

fn tracked_key(physical: &Physical) -> Option<TrackedKey> {
    match physical {
        Physical::Code(Code::ControlLeft) => Some(TrackedKey::LeftControl),
        Physical::Code(Code::ControlRight) => Some(TrackedKey::RightControl),
        Physical::Code(Code::AltLeft) => Some(TrackedKey::LeftAlt),
        Physical::Code(Code::AltRight) => Some(TrackedKey::RightAlt),
        _ => None,
    }
}

fn tracked_name(key: TrackedKey) -> &'static str {
    match key {
        TrackedKey::LeftControl => "LeftCtrl",
        TrackedKey::RightControl => "RightCtrl",
        TrackedKey::LeftAlt => "LeftAlt",
        TrackedKey::RightAlt => "RightAlt",
    }
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Ignored => "ignored",
        Status::Captured => "captured",
    }
}

fn update(state: &mut Probe, message: Message) {
    match message {
        Message::Raw(event) => match event {
            ProbeEvent::Key {
                line,
                tracked,
                pressed,
            } => {
                if let Some(key) = tracked {
                    match key {
                        TrackedKey::LeftControl => state.left_control = pressed,
                        TrackedKey::RightControl => state.right_control = pressed,
                        TrackedKey::LeftAlt => state.left_alt = pressed,
                        TrackedKey::RightAlt => state.right_alt = pressed,
                    }
                }
                push_log(state, line);
            }
            ProbeEvent::Log(line) => push_log(state, line),
            ProbeEvent::ImePreedit { value, selection } => {
                state.ime_preedit.clone_from(&value);
                push_log(state, format!("IME PREEDIT {value:?} {selection}"));
            }
            ProbeEvent::ImeCommit(value) => {
                state.ime_preedit.clear();
                state.last_ime_commit.clone_from(&value);
                push_log(state, format!("IME COMMIT {value:?}"));
            }
            ProbeEvent::ImeClosed => {
                state.ime_preedit.clear();
                push_log(state, "IME CLOSED".into());
            }
        },
        Message::TextChanged(value) => {
            state.text_value = value;
        }
        Message::ClearLog => state.log.clear(),
    }
}

fn push_log(state: &mut Probe, line: String) {
    state.log.push_back(line);
    while state.log.len() > MAX_LOG_LINES {
        state.log.pop_front();
    }
}

fn view(state: &Probe) -> Element<'_, Message> {
    let modifiers = row![
        text(key_state("Left Ctrl / LEAP back", state.left_control)),
        text(key_state("Right Ctrl", state.right_control)),
        text(key_state("Left Alt / LEAP forward", state.left_alt)),
        text(key_state("Right Alt / AltGr", state.right_alt)),
    ]
    .spacing(24);

    let input = text_input(
        "Click here, then type normal text, accents, AltGr, and IME text…",
        &state.text_value,
    )
    .on_input(Message::TextChanged)
    .padding(10);

    let ime = column![
        text(format!("IME preedit: {:?}", state.ime_preedit)),
        text(format!("Last IME commit: {:?}", state.last_ime_commit)),
    ]
    .spacing(4);

    let log_text = if state.log.is_empty() {
        "No input events yet.".to_owned()
    } else {
        state.log.iter().cloned().collect::<Vec<_>>().join("\n")
    };

    let content = column![
        text("Carta Space · Iced 0.14 input probe").size(28),
        text(
            "Required: independent Left/Right Ctrl and Alt DOWN/UP events. Right Alt must remain AltGr/text input, never LEAP."
        ),
        text(
            "Click the text field before testing AltGr or an IME. The raw event log receives captured events too."
        ),
        modifiers,
        input,
        ime,
        row![
            button("Clear event log").on_press(Message::ClearLog),
            text(format!("{} logged events", state.log.len())),
        ]
        .spacing(16),
        scrollable(text(log_text).size(14))
            .height(Length::Fill)
            .width(Length::Fill),
    ]
    .spacing(14)
    .padding(20)
    .height(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn key_state(label: &str, pressed: bool) -> String {
    format!("{label}: {}", if pressed { "DOWN" } else { "up" })
}
