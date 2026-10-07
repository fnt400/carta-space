use carta_app::Action;
use carta_core::LeapDirection;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, ModifierKeyCode};

#[derive(Clone, Copy)]
pub(super) struct PendingLeap {
    direction: LeapDirection,
    key: ModifierKeyCode,
}

impl PendingLeap {
    fn new(direction: LeapDirection, key: ModifierKeyCode) -> Self {
        Self { direction, key }
    }

    pub(super) fn direction(self) -> LeapDirection {
        self.direction
    }

    pub(super) fn matches_key(self, key: ModifierKeyCode) -> bool {
        self.key == key
    }

    pub(super) fn key(self) -> ModifierKeyCode {
        self.key
    }
}

/// Crossterm-specific state used while translating physical terminal input.
///
/// This state is intentionally kept out of `carta-app`: physical modifier
/// identity, enhanced-keyboard quirks, and suppressed release events belong to
/// the terminal adapter rather than to Carta's shared application semantics.
#[derive(Default)]
pub(super) struct TuiInputState {
    pending_leap: Option<PendingLeap>,
    active_leap: Option<PendingLeap>,
    suppressed_leap_releases: u8,
    right_control_held: bool,
}

impl TuiInputState {
    pub(super) fn right_control_held(&self) -> bool {
        self.right_control_held
    }

    pub(super) fn set_right_control_held(&mut self, held: bool) {
        self.right_control_held = held;
    }

    pub(super) fn pending(&self) -> Option<PendingLeap> {
        self.pending_leap
    }

    pub(super) fn active(&self) -> Option<PendingLeap> {
        self.active_leap
    }

    pub(super) fn has_pending(&self) -> bool {
        self.pending_leap.is_some()
    }

    pub(super) fn has_active(&self) -> bool {
        self.active_leap.is_some()
    }

    pub(super) fn no_leap_key_active(&self) -> bool {
        self.pending_leap.is_none() && self.active_leap.is_none()
    }

    pub(super) fn clear_pending(&mut self) {
        self.pending_leap = None;
    }

    pub(super) fn clear_active(&mut self) {
        self.active_leap = None;
    }

    pub(super) fn clear_leaps(&mut self) {
        self.pending_leap = None;
        self.active_leap = None;
    }

    pub(super) fn begin_pending(&mut self, direction: LeapDirection, key: ModifierKeyCode) {
        self.pending_leap = Some(PendingLeap::new(direction, key));
    }

    pub(super) fn take_pending(&mut self) -> Option<PendingLeap> {
        self.pending_leap.take()
    }

    pub(super) fn promote_pending(&mut self) -> Option<PendingLeap> {
        let pending = self.pending_leap.take()?;
        self.active_leap = Some(pending);
        Some(pending)
    }

    pub(super) fn set_active(&mut self, leap: PendingLeap) {
        self.active_leap = Some(leap);
    }

    pub(super) fn suppress_one_leap_release(&mut self) {
        self.suppressed_leap_releases = self.suppressed_leap_releases.saturating_add(1);
    }

    pub(super) fn suppress_leap_releases(&mut self, count: u8) {
        self.suppressed_leap_releases = count;
    }

    pub(super) fn consume_suppressed_leap_release(&mut self) -> bool {
        if self.suppressed_leap_releases == 0 {
            return false;
        }
        self.suppressed_leap_releases -= 1;
        true
    }
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
