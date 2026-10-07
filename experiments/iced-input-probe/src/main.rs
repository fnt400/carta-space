use std::collections::VecDeque;

use iced::advanced::input_method::Event as InputMethodEvent;
use iced::event::{self, Status};
use iced::keyboard::Event as KeyboardEvent;
use iced::keyboard::key::{Code, Physical};
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
    left_control: KeyState,
    right_control: KeyState,
    left_alt: KeyState,
    right_alt: KeyState,
    text_value: String,
    text_input_seen: bool,
    unicode_seen: bool,
    altgr_text_seen: bool,
    ime_preedit: String,
    last_ime_commit: String,
    ime_preedit_seen: bool,
    ime_commit_seen: bool,
    log: VecDeque<String>,
}

#[derive(Debug, Default)]
struct KeyState {
    pressed: bool,
    saw_down: bool,
    saw_up: bool,
}

impl KeyState {
    fn complete(&self) -> bool {
        self.saw_down && self.saw_up
    }

    fn observe(&mut self, pressed: bool) {
        self.pressed = pressed;
        if pressed {
            self.saw_down = true;
        } else {
            self.saw_up = true;
        }
    }
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
        produced_text: Option<String>,
    },
    Log(String),
    ImePreedit {
        value: String,
        selection: String,
    },
    ImeCommit {
        value: String,
        status: String,
    },
    ImeClosed(String),
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
            let produced_text = text.as_ref().map(ToString::to_string);
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
                produced_text,
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
                produced_text: None,
            }
        }
        KeyboardEvent::ModifiersChanged(modifiers) => {
            ProbeEvent::Log(format!("MODIFIERS {modifiers:?} status={status}"))
        }
    }
}

fn input_method_event(event: InputMethodEvent, status: Status) -> ProbeEvent {
    let status = status_name(status).to_owned();

    match event {
        InputMethodEvent::Opened => ProbeEvent::Log(format!("IME OPENED status={status}")),
        InputMethodEvent::Preedit(value, selection) => ProbeEvent::ImePreedit {
            value,
            selection: format!("{selection:?} status={status}"),
        },
        InputMethodEvent::Commit(value) => ProbeEvent::ImeCommit { value, status },
        InputMethodEvent::Closed => ProbeEvent::ImeClosed(status),
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

fn tracked_state_mut(state: &mut Probe, key: TrackedKey) -> &mut KeyState {
    match key {
        TrackedKey::LeftControl => &mut state.left_control,
        TrackedKey::RightControl => &mut state.right_control,
        TrackedKey::LeftAlt => &mut state.left_alt,
        TrackedKey::RightAlt => &mut state.right_alt,
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
                produced_text,
            } => {
                if let Some(key) = tracked {
                    tracked_state_mut(state, key).observe(pressed);
                }
                if let Some(produced_text) = produced_text.filter(|text| !text.is_empty()) {
                    if state.right_alt.pressed {
                        state.altgr_text_seen = true;
                    }
                    if produced_text.chars().any(|character| !character.is_ascii()) {
                        state.unicode_seen = true;
                    }
                }
                push_log(state, line);
            }
            ProbeEvent::Log(line) => push_log(state, line),
            ProbeEvent::ImePreedit { value, selection } => {
                state.ime_preedit.clone_from(&value);
                if !value.is_empty() {
                    state.ime_preedit_seen = true;
                }
                push_log(state, format!("IME PREEDIT {value:?} {selection}"));
            }
            ProbeEvent::ImeCommit { value, status } => {
                state.ime_preedit.clear();
                state.last_ime_commit.clone_from(&value);
                state.ime_commit_seen = true;
                if value.chars().any(|character| !character.is_ascii()) {
                    state.unicode_seen = true;
                }
                push_log(state, format!("IME COMMIT {value:?} status={status}"));
            }
            ProbeEvent::ImeClosed(status) => {
                state.ime_preedit.clear();
                push_log(state, format!("IME CLOSED status={status}"));
            }
        },
        Message::TextChanged(value) => {
            if value != state.text_value {
                state.text_input_seen = true;
                if state.right_alt.pressed {
                    state.altgr_text_seen = true;
                }
                if value.chars().any(|character| !character.is_ascii()) {
                    state.unicode_seen = true;
                }
            }
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
        text(key_state("Left Ctrl / LEAP back", &state.left_control)),
        text(key_state("Right Ctrl", &state.right_control)),
        text(key_state("Left Alt / LEAP forward", &state.left_alt)),
        text(key_state("Right Alt / AltGr", &state.right_alt)),
    ]
    .spacing(24);

    let input = text_input(
        "Click here, then type normal text, accents, AltGr, and IME text…",
        &state.text_value,
    )
    .on_input(Message::TextChanged)
    .padding(10);

    let checks = column![
        text(check_line("Left Ctrl DOWN + UP", state.left_control.complete())),
        text(check_line("Right Ctrl DOWN + UP", state.right_control.complete())),
        text(check_line("Left Alt DOWN + UP", state.left_alt.complete())),
        text(check_line("Right Alt DOWN + UP", state.right_alt.complete())),
        text(check_line("Normal text input", state.text_input_seen)),
        text(check_line("Unicode / non-ASCII input", state.unicode_seen)),
        text(check_line("AltGr produced text", state.altgr_text_seen)),
        text(optional_check_line("IME preedit observed", state.ime_preedit_seen)),
        text(optional_check_line("IME commit observed", state.ime_commit_seen)),
    ]
    .spacing(3);

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
        text("Automatic observations").size(20),
        checks,
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

fn key_state(label: &str, state: &KeyState) -> String {
    format!(
        "{label}: {} · {}",
        if state.pressed { "DOWN" } else { "up" },
        if state.complete() { "PASS" } else { "pending" }
    )
}

fn check_line(label: &str, passed: bool) -> String {
    format!("{label}: {}", if passed { "PASS" } else { "pending" })
}

fn optional_check_line(label: &str, passed: bool) -> String {
    format!(
        "{label}: {}",
        if passed {
            "observed"
        } else {
            "not observed (optional if no IME is configured)"
        }
    )
}
