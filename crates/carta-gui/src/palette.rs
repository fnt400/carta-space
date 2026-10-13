//! GUI-only command-palette presentation. The TUI keeps its original ordering.
use carta_app::{App, Command};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Section {
    Navigation,
    Documents,
    Works,
    Links,
    History,
    Export,
    Sync,
    Help,
}

impl Section {
    fn title(self) -> &'static str {
        match self {
            Self::Navigation => "NAVIGATION",
            Self::Documents => "WRITING & DOCUMENTS",
            Self::Works => "WORKS",
            Self::Links => "SEARCH & LINKS",
            Self::History => "HISTORY & RECOVERY",
            Self::Export => "IMPORT, EXPORT & PRINT",
            Self::Sync => "SYNC & CONFLICTS",
            Self::Help => "HELP & APPLICATION",
        }
    }
}

fn section(command: Command) -> Section {
    use Command::*;
    match command {
        PreviousMonth | NextMonth | GoToMonth | OpenCreationDateView
        | OpenModificationDateView | GoToStartOfView | GoToEndOfView
        | CollapseView | ExpandView | Back | Forward | LeapForward
        | LeapBackward | LeapAgainForward | LeapAgainBackward => Section::Navigation,

        NewDocument | DuplicateAsNew | NewLinkedDocument | SplitDocument
        | LockDocument | UnlockDocument | CheckDocumentSpelling
        | SetDocumentLanguage | RemoveDictionaryWord | InsertDateTime
        | Undo | Redo => Section::Documents,

        CreateWork | RenameWork | OpenWork | LockWork | UnlockWork
        | ShowMemberships | AddToWork | RemoveFromWork | MoveEarlier
        | MoveLater | MoveAfter | CheckWorkSpelling => Section::Works,

        SearchArchive | InsertLink | InsertBacklink | OpenLink
        | ShowBacklinks => Section::Links,

        History | WorkHistory | ReturnToPreviousView | RestoreThisVersion
        | RestoreAsNew | RestoreWorkVersion | Trash | TrashWork
        | ShowTrash | RestoreTrash | Wipe => Section::History,

        Import | ExportDocumentMarkdown | ExportWorkMarkdown
        | ExportDocumentPdf | ExportWorkPdf | PrintDocument | Package
        | CreateCheckpoint => Section::Export,

        SyncNow | SyncSettings | ShowConflicts | UseLocal | UseExternal
        | UseLocalPreserveOther | UseExternalPreserveOther => Section::Sync,

        Cheatsheet | Manual | Quit => Section::Help,
    }
}

/// The selection index always addresses this ordered (and filtered) list.
/// Never use the underlying TUI/app order for GUI Up, Down, or Enter.
pub(crate) fn commands(app: &App, query: &str) -> Vec<Command> {
    order(app.palette_commands(query))
}

fn order(mut commands: Vec<Command>) -> Vec<Command> {
    // Stable sort preserves the pre-existing order inside each category.
    commands.sort_by_key(|command| section(*command));
    commands
}

/// Show only shortcuts actually handled by the graphical keyboard adapter.
pub(crate) fn shortcut(command: Command) -> Option<&'static str> {
    use Command::*;
    match command {
        NewDocument => Some("Right Ctrl+N"),
        OpenWork => Some("Right Ctrl+W"),
        OpenCreationDateView => Some("Right Ctrl+M"),
        InsertLink | OpenLink => Some("Right Ctrl+L"),
        Undo => Some("Right Ctrl+Z"),
        Redo => Some("Right Ctrl+R"),
        LeapBackward => Some("Left Ctrl"),
        LeapForward => Some("Left Alt"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    Heading(&'static str),
    Command { command: Command, index: usize },
}

fn rows(commands: &[Command]) -> Vec<Row> {
    let mut result = Vec::with_capacity(commands.len() + 8);
    let mut previous = None;
    for (index, &command) in commands.iter().enumerate() {
        let current = section(command);
        if previous != Some(current) {
            result.push(Row::Heading(current.title()));
            previous = Some(current);
        }
        result.push(Row::Command { command, index });
    }
    result
}

/// Window both command rows and non-selectable headings. The selected
/// command must remain visible even when it lies beyond the initial viewport.
pub(crate) fn visible_rows(commands: &[Command], selected: usize, limit: usize) -> Vec<Row> {
    let rows = rows(commands);
    if rows.is_empty() || limit == 0 {
        return Vec::new();
    }
    let active_row = rows
        .iter()
        .position(|row| matches!(row, Row::Command { index, .. } if *index == selected))
        .unwrap_or(0);
    let first = active_row
        .saturating_sub(limit.saturating_sub(3))
        .min(rows.len().saturating_sub(limit));
    rows.into_iter().skip(first).take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_commands_and_keeps_original_order_within_each_section() {
        let input = vec![
            Command::Quit,
            Command::InsertLink,
            Command::NewDocument,
            Command::Manual,
            Command::OpenLink,
            Command::Undo,
        ];
        let sorted = order(input);
        assert_eq!(
            sorted,
            vec![
                Command::NewDocument,
                Command::Undo,
                Command::InsertLink,
                Command::OpenLink,
                Command::Quit,
                Command::Manual,
            ]
        );
        let entries = rows(&sorted);
        assert!(matches!(entries.first(), Some(Row::Heading("WRITING & DOCUMENTS"))));
        assert_eq!(
            entries.iter().filter(|row| matches!(row, Row::Heading(_))).count(),
            3
        );
    }

    #[test]
    fn selected_command_remains_visible_at_top_middle_and_bottom() {
        let commands = order(vec![
            Command::Manual,
            Command::Quit,
            Command::NewDocument,
            Command::Undo,
            Command::History,
            Command::ExportDocumentPdf,
            Command::CreateWork,
            Command::SearchArchive,
            Command::OpenWork,
            Command::NextMonth,
            Command::Back,
            Command::InsertLink,
            Command::SyncNow,
            Command::Cheatsheet,
            Command::Redo,
        ]);
        for (selected, _) in commands.iter().enumerate() {
            let visible = visible_rows(&commands, selected, 5);
            assert!(visible.len() <= 5);
            assert!(
                visible.iter().any(|row| matches!(row, Row::Command { index, .. } if *index == selected)),
                "selection {selected} scrolled outside the palette"
            );
        }
        assert!(visible_rows(&commands, 14, 0).is_empty());
        assert!(visible_rows(&[], 0, 8).is_empty());
    }

    #[test]
    fn advertised_shortcuts_correspond_to_graphical_bindings() {
        assert_eq!(shortcut(Command::OpenLink), Some("Right Ctrl+L"));
        assert_eq!(shortcut(Command::InsertLink), Some("Right Ctrl+L"));
        assert_eq!(shortcut(Command::Manual), None);
        assert_eq!(shortcut(Command::PrintDocument), None);
    }
}
