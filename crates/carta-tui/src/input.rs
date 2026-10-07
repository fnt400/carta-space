use carta_app::Action;
use carta_core::LeapDirection;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, ModifierKeyCode,
};

#[derive(Clone, Copy)]
pub(super) struct PendingLeap {
    pub(super) direction: LeapDirection,
    pub(super) key: ModifierKeyCode,
}

/// Crossterm-specific state used while translating physical terminal input.
///
/// This state is intentionally kept out of `carta-app`: physical modifier
/// identity, enhanced-keyboard quirks, and suppressed release events belong to
/// the terminal adapter rather than to Carta's shared application semantics.
#[derive(Default)]
pub(super) struct TuiInputState {
    pub(super) pending_leap: Option<PendingLeap>,
    pub(super) active_leap: Option<PendingLeap>,
    pub(super) suppressed_leap_releases: u8,
    pub(super) right_control_held: bool,
}

pub(super) fn editing_action_from_key(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::InsertText(c.to_string()))
        }
        KeyCode::Enter => Some(Action::InsertLineBreak),
        KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => Some(Action::Outdent),
        KeyCode::Tab => Some(Action::Indent),
        KeyCode::Backspace => Some(Action::Backspace),
        KeyCode::Delete => Some(Action::Erase),
        _ => None,
    }
}

pub(super) fn leap_direction_from_modifier(key: ModifierKeyCode) -> Option<LeapDirection> {
    match key {
        ModifierKeyCode::LeftControl => Some(LeapDirection::Backward),
        ModifierKeyCode::LeftAlt => Some(LeapDirection::Forward),
        _ => None,
    }
}

pub(super) fn emergency_kill_event(event: &Event, right_control_held: &mut bool) -> bool {
    let Event::Key(key) = event else {
        return false;
    };
    if key.code == KeyCode::Modifier(ModifierKeyCode::RightControl) {
        if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            *right_control_held = true;
        } else if key.kind == KeyEventKind::Release {
            *right_control_held = false;
        }
        return false;
    }
    *right_control_held
        && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('g' | 'G'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_leap_modifiers_map_only_left_control_and_left_alt() {
        assert_eq!(
            leap_direction_from_modifier(ModifierKeyCode::LeftControl),
            Some(LeapDirection::Backward)
        );
        assert_eq!(
            leap_direction_from_modifier(ModifierKeyCode::LeftAlt),
            Some(LeapDirection::Forward)
        );
        assert_eq!(
            leap_direction_from_modifier(ModifierKeyCode::RightAlt),
            None
        );
        assert_eq!(
            leap_direction_from_modifier(ModifierKeyCode::RightControl),
            None
        );
    }
}
