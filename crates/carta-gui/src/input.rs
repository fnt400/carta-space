use carta_app::{Action, App, AppMode};
use carta_core::LeapDirection;
use iced::keyboard::key::{Code, Physical};
use iced::keyboard::Event as KeyboardEvent;
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
struct PendingLeap {
    code: Code,
    direction: LeapDirection,
}

#[derive(Debug, Default)]
pub struct GuiInputState {
    pending_leap: Option<PendingLeap>,
    active_leap: Option<PendingLeap>,
    suppressed_leap_releases: u8,
}

impl GuiInputState {
    pub fn handle(&mut self, app: &mut App, event: KeyboardEvent) {
        match event {
            KeyboardEvent::KeyPressed {
                physical_key,
                text,
                repeat,
                ..
            } => {
                let Physical::Code(code) = physical_key else {
                    return;
                };

                if !repeat && self.handle_leap_press(app, code) {
                    return;
                }

                if is_neutral_modifier(code) {
                    return;
                }

                self.activate_pending(app);
                self.handle_key_press(app, code, text.as_deref());
            }
            KeyboardEvent::KeyReleased { physical_key, .. } => {
                let Physical::Code(code) = physical_key else {
                    return;
                };
                self.handle_release(app, code);
            }
            KeyboardEvent::ModifiersChanged(_) => {}
        }
    }

    fn handle_leap_press(&mut self, app: &mut App, code: Code) -> bool {
        let direction = match code {
            Code::ControlLeft => Some(LeapDirection::Backward),
            Code::AltLeft => Some(LeapDirection::Forward),
            _ => None,
        };
        let Some(direction) = direction else {
            return false;
        };

        if self.active_leap.is_some() {
            return true;
        }

        if let Some(pending) = self.pending_leap {
            if pending.code != code {
                self.pending_leap = None;
                self.suppressed_leap_releases = 2;
                app.extend_last_leap_highlight();
            }
            return true;
        }

        self.pending_leap = Some(PendingLeap { code, direction });
        true
    }

    fn activate_pending(&mut self, app: &mut App) {
        let Some(pending) = self.pending_leap.take() else {
            return;
        };
        self.active_leap = Some(pending);
        app.dispatch_action(Action::BeginLeap(pending.direction), Instant::now());
    }

    fn handle_release(&mut self, app: &mut App, code: Code) {
        if matches!(code, Code::ControlLeft | Code::AltLeft) && self.suppressed_leap_releases > 0 {
            self.suppressed_leap_releases -= 1;
            return;
        }

        if self
            .pending_leap
            .is_some_and(|pending| pending.code == code)
        {
            let pending = self.pending_leap.take().expect("pending LEAP exists");
            app.cat_tap_leap(pending.direction);
            return;
        }

        if self.active_leap.is_some_and(|active| active.code == code) {
            self.active_leap = None;
            if matches!(app.mode, AppMode::Leap { .. }) {
                app.dispatch_action(Action::EndLeap, Instant::now());
            }
        }
    }

    fn handle_key_press(&mut self, app: &mut App, code: Code, text: Option<&str>) {
        let in_leap = matches!(app.mode, AppMode::Leap { .. });
        match code {
            Code::Backspace => {
                let action = if in_leap {
                    Action::LeapBackspace
                } else {
                    Action::Backspace
                };
                app.dispatch_action(action, Instant::now());
                return;
            }
            Code::Enter | Code::NumpadEnter => {
                let action = if in_leap {
                    Action::LeapEnter
                } else {
                    Action::InsertLineBreak
                };
                app.dispatch_action(action, Instant::now());
                return;
            }
            Code::Escape if in_leap => {
                app.dispatch_action(Action::CancelLeap, Instant::now());
                return;
            }
            Code::Delete if !in_leap => {
                app.dispatch_action(Action::Erase, Instant::now());
                return;
            }
            Code::ArrowLeft if !in_leap => {
                app.dispatch_action(Action::MoveCharacterBackward, Instant::now());
                return;
            }
            Code::ArrowRight if !in_leap => {
                app.dispatch_action(Action::MoveCharacterForward, Instant::now());
                return;
            }
            Code::Home if !in_leap => {
                app.dispatch_action(Action::DocumentStart, Instant::now());
                return;
            }
            Code::End if !in_leap => {
                app.dispatch_action(Action::DocumentEnd, Instant::now());
                return;
            }
            Code::Tab if !in_leap => {
                app.dispatch_action(Action::Indent, Instant::now());
                return;
            }
            _ => {}
        }

        let Some(text) = text.filter(|text| !text.is_empty()) else {
            return;
        };
        let action = if in_leap {
            Action::LeapInput(text.to_owned())
        } else {
            Action::InsertText(text.to_owned())
        };
        app.dispatch_action(action, Instant::now());
    }
}

fn is_neutral_modifier(code: Code) -> bool {
    matches!(
        code,
        Code::ShiftLeft | Code::ShiftRight | Code::ControlRight | Code::AltRight
    )
}
