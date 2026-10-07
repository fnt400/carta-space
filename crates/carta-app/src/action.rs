use carta_core::LeapDirection;

/// Frontend-independent interaction intents.
///
/// Frontends translate native events into these semantic actions. The enum grows
/// incrementally as behavior is extracted from presentation-specific code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    InsertText(String),
    InsertLineBreak,
    Indent,
    Outdent,
    Backspace,
    Erase,
    MoveCharacterBackward,
    MoveCharacterForward,
    PreviousDocument,
    NextDocument,
    DocumentStart,
    DocumentEnd,
    BeginLeap(LeapDirection),
    EndLeap,
    LeapInput(String),
    LeapBackspace,
    LeapEnter,
    CancelLeap,
}
