use carta_app::{Action, App, AppMode, Command, ModeAction, View};
use carta_core::LeapDirection;
use iced::keyboard::key::{Code, Physical};
use iced::keyboard::Event as KeyboardEvent;
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
struct PendingLeap {
    code: Code,
    direction: LeapDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RightControlIntent {
    Kill,
    NewDocument,
    Bold,
    Italic,
    OpenWork,
    Link,
    MonthlyCreationDate,
    Undo,
    Redo,
    CopyOrPaste,
    PreviousDocument,
    NextDocument,
    DocumentStart,
    DocumentEnd,
}

fn right_control_intent(code: Code) -> Option<RightControlIntent> {
    match code {
        Code::KeyG => Some(RightControlIntent::Kill),
        Code::KeyN => Some(RightControlIntent::NewDocument),
        Code::KeyB => Some(RightControlIntent::Bold),
        Code::KeyI => Some(RightControlIntent::Italic),
        Code::KeyW => Some(RightControlIntent::OpenWork),
        Code::KeyL => Some(RightControlIntent::Link),
        Code::KeyM => Some(RightControlIntent::MonthlyCreationDate),
        Code::KeyZ => Some(RightControlIntent::Undo),
        Code::KeyR => Some(RightControlIntent::Redo),
        Code::KeyC => Some(RightControlIntent::CopyOrPaste),
        Code::PageUp => Some(RightControlIntent::PreviousDocument),
        Code::PageDown => Some(RightControlIntent::NextDocument),
        Code::Home => Some(RightControlIntent::DocumentStart),
        Code::End => Some(RightControlIntent::DocumentEnd),
        _ => None,
    }
}

#[derive(Debug, Default)]
pub struct GuiInputState {
    pending_leap: Option<PendingLeap>,
    active_leap: Option<PendingLeap>,
    suppressed_leap_releases: u8,
    right_control_held: bool,
}

impl GuiInputState {
    pub fn handle(&mut self, app: &mut App, event: KeyboardEvent) -> carta_app::AppResult {
        match event {
            KeyboardEvent::KeyPressed {
                physical_key,
                text,
                repeat,
                ..
            } => {
                let Physical::Code(code) = physical_key else {
                    return Ok(());
                };

                if !repeat && code == Code::ControlRight {
                    self.handle_right_control_press(app);
                    return Ok(());
                }

                if !repeat && self.handle_leap_press(app, code) {
                    return Ok(());
                }

                if is_neutral_modifier(code) {
                    return Ok(());
                }

                self.activate_pending(app);
                self.handle_key_press(app, code, text.as_deref())?;
            }
            KeyboardEvent::KeyReleased { physical_key, .. } => {
                let Physical::Code(code) = physical_key else {
                    return Ok(());
                };
                if code == Code::ControlRight {
                    self.right_control_held = false;
                    return Ok(());
                }
                self.handle_release(app, code);
            }
            KeyboardEvent::ModifiersChanged(_) => {}
        }
        Ok(())
    }

    fn handle_right_control_press(&mut self, app: &mut App) {
        self.right_control_held = true;

        if let Some(active) = self.active_leap {
            if matches!(app.mode, AppMode::Leap { palette: false, .. }) {
                app.leap_again_active();
            } else {
                app.leap_again_preserving_anchor(active.direction);
            }
            return;
        }

        if let Some(pending) = self.pending_leap.take() {
            self.active_leap = Some(pending);
            app.leap_again(pending.direction);
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

        if self.right_control_held && self.active_leap.is_none() {
            self.pending_leap = None;
            self.suppressed_leap_releases = self.suppressed_leap_releases.saturating_add(1);
            app.leap_again(direction);
            return true;
        }

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

    fn handle_key_press(
        &mut self,
        app: &mut App,
        code: Code,
        text: Option<&str>,
    ) -> carta_app::AppResult {
        if self.right_control_held && self.handle_right_control_chord(app, code)? {
            return Ok(());
        }

        if self.handle_mode_key(app, code, text)? {
            return Ok(());
        }

        if matches!(app.mode, AppMode::Editing) && code == Code::Escape {
            if matches!(app.view, View::Help { .. }) {
                app.execute(carta_app::Command::ReturnToPreviousView)?;
            } else {
                app.open_palette();
            }
            return Ok(());
        }

        if matches!(
            app.view,
            View::Search { .. }
                | View::History { .. }
                | View::WorkHistory { .. }
                | View::Trash { .. }
                | View::Conflicts { .. }
                | View::Help { .. }
        ) && matches!(app.mode, AppMode::Editing)
        {
            match code {
                Code::ArrowUp => app.move_list_selection(false),
                Code::ArrowDown => app.move_list_selection(true),
                Code::Enter | Code::NumpadEnter => app.open_selected()?,
                _ => {}
            }
            return Ok(());
        }

        if app.collapsed && matches!(app.mode, AppMode::Editing) {
            match code {
                Code::ArrowUp | Code::PageUp => {
                    app.dispatch_action(Action::PreviousDocument, Instant::now());
                }
                Code::ArrowDown | Code::PageDown => {
                    app.dispatch_action(Action::NextDocument, Instant::now());
                }
                Code::Enter | Code::NumpadEnter => {
                    app.execute(carta_app::Command::ExpandView)?;
                }
                _ => {}
            }
            return Ok(());
        }

        let in_leap = matches!(app.mode, AppMode::Leap { .. });
        match code {
            Code::Backspace => {
                let action = if in_leap {
                    Action::LeapBackspace
                } else {
                    Action::Backspace
                };
                app.dispatch_action(action, Instant::now());
                return Ok(());
            }
            Code::Enter | Code::NumpadEnter => {
                let action = if in_leap {
                    Action::LeapEnter
                } else {
                    Action::InsertLineBreak
                };
                app.dispatch_action(action, Instant::now());
                return Ok(());
            }
            Code::Escape if in_leap => {
                app.dispatch_action(Action::CancelLeap, Instant::now());
                return Ok(());
            }
            Code::Delete if !in_leap => {
                app.dispatch_action(Action::Erase, Instant::now());
                return Ok(());
            }
            Code::ArrowLeft if !in_leap => {
                app.dispatch_action(Action::MoveCharacterBackward, Instant::now());
                return Ok(());
            }
            Code::ArrowRight if !in_leap => {
                app.dispatch_action(Action::MoveCharacterForward, Instant::now());
                return Ok(());
            }
            Code::Home if !in_leap => {
                app.dispatch_action(Action::DocumentStart, Instant::now());
                return Ok(());
            }
            Code::End if !in_leap => {
                app.dispatch_action(Action::DocumentEnd, Instant::now());
                return Ok(());
            }
            Code::Tab if !in_leap => {
                app.dispatch_action(Action::Indent, Instant::now());
                return Ok(());
            }
            _ => {}
        }

        let Some(text) = text.filter(|text| !text.is_empty()) else {
            return Ok(());
        };
        let action = if in_leap {
            Action::LeapInput(text.to_owned())
        } else {
            Action::InsertText(text.to_owned())
        };
        app.dispatch_action(action, Instant::now());
        Ok(())
    }

    fn handle_right_control_chord(
        &mut self,
        app: &mut App,
        code: Code,
    ) -> carta_app::AppResult<bool> {
        let Some(intent) = right_control_intent(code) else {
            return Ok(false);
        };

        self.pending_leap = None;

        let editing = matches!(app.mode, AppMode::Editing);
        let editable_view = matches!(
            app.view,
            View::CreationDate(_) | View::ModificationDate | View::Work(_)
        );
        let editable = editing && editable_view && !app.collapsed;

        match intent {
            RightControlIntent::Kill => {
                self.active_leap = None;
                app.trigger_kill_switch();
            }
            RightControlIntent::NewDocument if editing && editable_view => {
                app.execute(Command::NewDocument)?;
            }
            RightControlIntent::Bold if editable => {
                if app.insert_markdown_pair("**") {
                    app.edited(Instant::now());
                    app.status.clear();
                }
            }
            RightControlIntent::Italic if editable => {
                if app.insert_markdown_pair("*") {
                    app.edited(Instant::now());
                    app.status.clear();
                }
            }
            RightControlIntent::OpenWork if editing => app.execute(Command::OpenWork)?,
            RightControlIntent::Link if editing => app.activate_link_shortcut()?,
            RightControlIntent::MonthlyCreationDate if editing => {
                app.execute(Command::OpenCreationDateView)?;
            }
            RightControlIntent::Undo if editable => app.execute(Command::Undo)?,
            RightControlIntent::Redo if editable => app.execute(Command::Redo)?,
            RightControlIntent::CopyOrPaste if editable => {
                if app.editor.cat_highlight().is_some() {
                    app.copy_cat_highlight();
                } else {
                    app.status = "System clipboard paste is not yet available in carta-gui".into();
                }
            }
            RightControlIntent::PreviousDocument if editing && editable_view => {
                app.dispatch_action(Action::PreviousDocument, Instant::now());
            }
            RightControlIntent::NextDocument if editing && editable_view => {
                app.dispatch_action(Action::NextDocument, Instant::now());
            }
            RightControlIntent::DocumentStart if editing && editable_view => {
                app.dispatch_action(Action::DocumentStart, Instant::now());
            }
            RightControlIntent::DocumentEnd if editing && editable_view => {
                app.dispatch_action(Action::DocumentEnd, Instant::now());
            }
            _ => {}
        }

        Ok(true)
    }

    fn handle_mode_key(
        &mut self,
        app: &mut App,
        code: Code,
        text: Option<&str>,
    ) -> carta_app::AppResult<bool> {
        if let AppMode::Palette { query, selected } = &app.mode {
            let commands = app.palette_commands(query);
            let chosen = commands.get(*selected).copied();
            match code {
                Code::Escape => app.cancel_mode(),
                Code::Backspace => {
                    if let AppMode::Palette { query, selected } = &mut app.mode {
                        query.pop();
                        *selected = 0;
                    }
                }
                Code::ArrowUp => {
                    if let AppMode::Palette { selected, .. } = &mut app.mode {
                        *selected = selected.saturating_sub(1);
                    }
                }
                Code::ArrowDown => {
                    if let AppMode::Palette { selected, .. } = &mut app.mode {
                        *selected = (*selected + 1).min(commands.len().saturating_sub(1));
                    }
                }
                Code::Enter | Code::NumpadEnter => {
                    if let Some(command) = chosen {
                        app.mode = AppMode::Editing;
                        app.execute(command)?;
                    }
                }
                _ => {
                    if let Some(value) = text.filter(|value| !value.is_empty()) {
                        if let AppMode::Palette { query, selected } = &mut app.mode {
                            query.push_str(value);
                            *selected = 0;
                        }
                    }
                }
            }
            return Ok(true);
        }

        let action = match &app.mode {
            AppMode::Prompt { .. } => match code {
                Code::Escape => Some(ModeAction::Cancel),
                Code::Enter | Code::NumpadEnter => Some(ModeAction::Submit),
                Code::ArrowLeft => Some(ModeAction::CursorBackward),
                Code::ArrowRight => Some(ModeAction::CursorForward),
                Code::Home => Some(ModeAction::CursorStart),
                Code::End => Some(ModeAction::CursorEnd),
                Code::Backspace => Some(ModeAction::Backspace),
                Code::Delete => Some(ModeAction::Delete),
                _ => text
                    .filter(|value| !value.is_empty())
                    .map(|value| ModeAction::InsertText(value.to_owned())),
            },
            AppMode::Confirm { .. } => match code {
                Code::KeyY => Some(ModeAction::Confirm(true)),
                Code::KeyN | Code::Escape | Code::Enter | Code::NumpadEnter => {
                    Some(ModeAction::Confirm(false))
                }
                _ => None,
            },
            AppMode::Selector { .. } => match code {
                Code::Escape => Some(ModeAction::Cancel),
                Code::Enter | Code::NumpadEnter => Some(ModeAction::Submit),
                Code::Backspace => Some(ModeAction::Backspace),
                Code::ArrowUp => Some(ModeAction::SelectionPrevious),
                Code::ArrowDown => Some(ModeAction::SelectionNext),
                _ => text
                    .filter(|value| !value.is_empty())
                    .map(|value| ModeAction::InsertText(value.to_owned())),
            },
            _ => return Ok(false),
        };
        if let Some(action) = action {
            app.dispatch_mode_action(action)?;
        }
        Ok(true)
    }
}

fn is_neutral_modifier(code: Code) -> bool {
    matches!(code, Code::ShiftLeft | Code::ShiftRight | Code::AltRight)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_control_chords_match_the_interaction_contract() {
        let expected = [
            (Code::KeyG, RightControlIntent::Kill),
            (Code::KeyN, RightControlIntent::NewDocument),
            (Code::KeyB, RightControlIntent::Bold),
            (Code::KeyI, RightControlIntent::Italic),
            (Code::KeyW, RightControlIntent::OpenWork),
            (Code::KeyL, RightControlIntent::Link),
            (Code::KeyM, RightControlIntent::MonthlyCreationDate),
            (Code::KeyZ, RightControlIntent::Undo),
            (Code::KeyR, RightControlIntent::Redo),
            (Code::KeyC, RightControlIntent::CopyOrPaste),
            (Code::PageUp, RightControlIntent::PreviousDocument),
            (Code::PageDown, RightControlIntent::NextDocument),
            (Code::Home, RightControlIntent::DocumentStart),
            (Code::End, RightControlIntent::DocumentEnd),
        ];

        for (code, intent) in expected {
            assert_eq!(right_control_intent(code), Some(intent));
        }
        assert_eq!(right_control_intent(Code::KeyA), None);
        assert_eq!(right_control_intent(Code::AltRight), None);
    }
}
