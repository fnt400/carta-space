use carta_core::{DocumentId, LeapSession, WorkId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptAction {
    Search,
    GoToMonth,
    CreateWork,
    RenameWork,
    Import,
    ExportDocumentMarkdown,
    ExportWorkMarkdown,
    ExportDocumentPdf,
    ExportWorkPdf,
    Package,
    Checkpoint,
    SyncRemote,
    ConfirmRestoreDocument,
    ConfirmRestoreDocumentVersion,
    ConfirmRestoreWork,
    ConfirmRestoreWorkWithTitle,
    ConfirmRestoreWorkVersion,
    ConfirmWipe,
    ResolveLocal,
    ResolveExternal,
    ResolveLocalPreserve,
    ResolveExternalPreserve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAction {
    TrashDocument(DocumentId),
    TrashWork(WorkId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectAction {
    OpenWork,
    AddToWork,
    MoveAfter,
    InsertLink,
    OpenBacklink,
    SpellingSuggestion,
    SetDocumentLanguage,
    SetSpellingDocumentLanguage,
    RemoveDictionaryWord,
    OpenSearchResult,
    RestoreHistory,
    RestoreHistoryAsNew,
    TrashItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRow {
    pub document: DocumentId,
    pub created: carta_core::Timestamp,
    pub label: String,
    pub context: String,
    pub byte: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeAction {
    Cancel,
    Submit,
    InsertText(String),
    Backspace,
    Delete,
    CursorBackward,
    CursorForward,
    CursorStart,
    CursorEnd,
    SelectionPrevious,
    SelectionNext,
    Confirm(bool),
}

#[derive(Debug, Clone)]
pub enum AppMode {
    Editing,
    Palette {
        query: String,
        selected: usize,
    },
    Prompt {
        title: String,
        input: String,
        cursor: usize,
        details: Vec<String>,
        action: PromptAction,
    },
    Confirm {
        title: String,
        details: Vec<String>,
        action: ConfirmAction,
    },
    Selector {
        title: String,
        query: String,
        selected: usize,
        choices: Vec<Choice>,
        action: SelectAction,
    },
    Leap {
        session: LeapSession,
        palette: bool,
    },
}
