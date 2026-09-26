use crate::editor::{CompositeEditor, Cursor, Region};
use crate::palette;
use crate::session::{Position, SavedView, Session};
use carta_core::{
    Archive, CartaLinkTarget, CheckpointKind, Conflict, ConflictChoice, DocumentId,
    DocumentTextRegion, LeapDirection, LeapPosition, LeapRuntime, LeapSession, PdfExportOptions,
    Volume, WorkId, WorkRestoreOptions,
};
use chrono::{Datelike, Local};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::time::{Duration, Instant};

pub type AppResult<T = ()> = Result<T, Box<dyn Error>>;

const WORK_COLOR_PALETTE: [&str; 8] = [
    "#667A75", "#6D7487", "#806F6A", "#756A80", "#A9B39B", "#B2A596", "#9FAAB5", "#A99EAE",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Chronological(Volume),
    Work(WorkId),
    Search {
        query: String,
        selected: usize,
    },
    History {
        document: DocumentId,
        selected: usize,
    },
    WorkHistory {
        work: WorkId,
        selected: usize,
    },
    Trash {
        selected: usize,
    },
    Conflicts {
        selected: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Location {
    view: View,
    cursor: Cursor,
    scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptAction {
    Search,
    GoToMonth,
    CreateWork,
    RenameWork,
    Import,
    ExportDocumentMarkdown,
    ExportDocumentPdf,
    ExportWorkMarkdown,
    ExportWorkPdf,
    Package,
    Checkpoint,
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
    OpenSearchResult,
    RestoreHistory,
    RestoreHistoryAsNew,
    TrashItem,
}

#[derive(Debug, Clone)]
pub struct Choice {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct ResultRow {
    pub document: DocumentId,
    pub created: carta_core::Timestamp,
    pub label: String,
    pub context: String,
    pub byte: usize,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    NewDocument,
    DuplicateAsNew,
    NewLinkedDocument,
    PreviousMonth,
    NextMonth,
    GoToMonth,
    CreateWork,
    RenameWork,
    OpenWork,
    OpenChronologicalView,
    ShowMemberships,
    AddToWork,
    RemoveFromWork,
    MoveEarlier,
    MoveLater,
    MoveAfter,
    SearchArchive,
    InsertLink,
    OpenLink,
    ShowBacklinks,
    Back,
    Forward,
    History,
    WorkHistory,
    ReturnToPreviousView,
    RestoreThisVersion,
    RestoreAsNew,
    RestoreWorkVersion,
    Trash,
    ShowTrash,
    TrashWork,
    RestoreTrash,
    Wipe,
    Import,
    ExportDocumentMarkdown,
    ExportDocumentPdf,
    ExportWorkMarkdown,
    ExportWorkPdf,
    Package,
    CreateCheckpoint,
    InsertDateTime,
    LeapForward,
    LeapBackward,
    LeapAgainForward,
    LeapAgainBackward,
    Undo,
    Redo,
    Quit,
    ShowConflicts,
    UseLocal,
    UseExternal,
    UseLocalPreserveOther,
    UseExternalPreserveOther,
}

impl Command {
    pub fn label(self) -> &'static str {
        match self {
            Self::NewDocument => "New Document",
            Self::DuplicateAsNew => "Duplicate as New",
            Self::NewLinkedDocument => "New Linked Document",
            Self::PreviousMonth => "Previous Month",
            Self::NextMonth => "Next Month",
            Self::GoToMonth => "Go to Month…",
            Self::CreateWork => "Create Work…",
            Self::RenameWork => "Rename Work…",
            Self::OpenWork => "Open Work…",
            Self::OpenChronologicalView => "Open Chronological View",
            Self::ShowMemberships => "Show Memberships",
            Self::AddToWork => "Add to Work…",
            Self::RemoveFromWork => "Remove from Work",
            Self::MoveEarlier => "Move Document Earlier",
            Self::MoveLater => "Move Document Later",
            Self::MoveAfter => "Move This Document After…",
            Self::SearchArchive => "Search Archive…",
            Self::InsertLink => "Insert Link…",
            Self::OpenLink => "Open Link",
            Self::ShowBacklinks => "Show Backlinks",
            Self::Back => "Back",
            Self::Forward => "Forward",
            Self::History => "Document History",
            Self::WorkHistory => "Work History",
            Self::ReturnToPreviousView => "Return to Previous View",
            Self::RestoreThisVersion => "Restore This Version",
            Self::RestoreAsNew => "Restore as New Document",
            Self::RestoreWorkVersion => "Restore Work Version",
            Self::Trash => "Trash",
            Self::TrashWork => "Trash Work",
            Self::ShowTrash => "Show Trash",
            Self::RestoreTrash => "Restore from Trash",
            Self::Wipe => "Wipe permanently",
            Self::Import => "Import…",
            Self::ExportDocumentMarkdown => "Export Document as Markdown…",
            Self::ExportDocumentPdf => "Export Document as PDF…",
            Self::ExportWorkMarkdown => "Export Work as Markdown…",
            Self::ExportWorkPdf => "Export Work as PDF…",
            Self::Package => "Package Archive…",
            Self::CreateCheckpoint => "Create Checkpoint…",
            Self::InsertDateTime => "Insert Current Date and Time",
            Self::LeapForward => "LEAP Forward…",
            Self::LeapBackward => "LEAP Backward…",
            Self::LeapAgainForward => "Leap Again Forward",
            Self::LeapAgainBackward => "Leap Again Backward",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Quit => "Quit",
            Self::ShowConflicts => "Show Conflicts",
            Self::UseLocal => "Use Local Variant",
            Self::UseExternal => "Use External Variant",
            Self::UseLocalPreserveOther => "Use Local; Preserve External as New Document",
            Self::UseExternalPreserveOther => "Use External; Preserve Local as New Document",
        }
    }
}

pub struct Scheduler {
    last_edit: Option<Instant>,
    last_checkpoint: Instant,
    autosave_after: Duration,
    checkpoint_after: Duration,
}

impl Scheduler {
    pub fn new(now: Instant) -> Self {
        Self {
            last_edit: None,
            last_checkpoint: now,
            autosave_after: Duration::from_secs(1),
            checkpoint_after: Duration::from_secs(600),
        }
    }
    pub fn edited(&mut self, now: Instant) {
        self.last_edit = Some(now);
    }
    pub fn autosave_due(&self, now: Instant, dirty: bool) -> bool {
        dirty
            && self
                .last_edit
                .is_some_and(|last| now.duration_since(last) >= self.autosave_after)
    }
    pub fn checkpoint_due(&self, now: Instant) -> bool {
        now.duration_since(self.last_checkpoint) >= self.checkpoint_after
    }
    pub fn saved(&mut self) {
        self.last_edit = None;
    }
    pub fn checkpointed(&mut self, now: Instant) {
        self.last_checkpoint = now;
    }
}

pub struct App {
    pub archive: Archive,
    pub view: View,
    pub editor: CompositeEditor,
    pub mode: AppMode,
    pub status: String,
    pub scroll: usize,
    pub search_results: Vec<ResultRow>,
    pub history: Vec<carta_core::DocumentRevision>,
    pub work_history: Vec<carta_core::WorkSnapshot>,
    pub trash: Option<carta_core::TrashInventory>,
    pub conflicts: Vec<Conflict>,
    pub scheduler: Scheduler,
    pub leap: LeapRuntime,
    pub quit: bool,
    last_leap_span: Option<(Cursor, Cursor)>,
    cat_span_fixed: Option<Cursor>,
    rehighlight_span: Option<(Cursor, Cursor)>,
    typed_span_start: Option<Cursor>,
    cat_erase_forward: bool,
    back: Vec<Location>,
    forward: Vec<Location>,
    provisional: Option<DocumentId>,
    pending_wipe: Option<carta_core::WipePlan>,
    work_positions: BTreeMap<WorkId, Position>,
    work_mru: Vec<WorkId>,
}

impl App {
    pub fn open(mut archive: Archive, session: Option<&Session>, now: Instant) -> AppResult<Self> {
        if ensure_work_colors(&mut archive)? {
            archive.checkpoint(
                CheckpointKind::Structural,
                Some("Assigned persistent Work colors"),
            )?;
        }
        let current = current_volume();
        let mut provisional = None;
        let mut position = session.and_then(|s| s.position.clone());
        let mut view = match session.map(|s| &s.view) {
            Some(SavedView::Work { id }) if archive.work(*id).is_some() => View::Work(*id),
            Some(SavedView::Search { query, selected }) => View::Search {
                query: query.clone(),
                selected: *selected,
            },
            Some(SavedView::Chronological { year, month }) => {
                View::Chronological(Volume::new(*year, *month).unwrap_or(current))
            }
            _ => {
                let id = archive.create_document("")?;
                provisional = Some(id);
                position = Some(Position {
                    document: id,
                    byte: 0,
                    scroll: 0,
                });
                View::Chronological(current)
            }
        };
        if matches!(&view, View::Chronological(volume) if archive.chronological_month(*volume).is_empty())
        {
            let id = archive.create_document("")?;
            provisional = Some(id);
            view = View::Chronological(current);
            position = Some(Position {
                document: id,
                byte: 0,
                scroll: 0,
            });
        }
        let (mut editor, scroll) = load_editor(&archive, &view, position.as_ref())?;
        if let View::Search { selected, .. } = &view {
            editor.set_cursor(
                Cursor {
                    region: *selected,
                    byte: 0,
                },
                false,
            );
        }
        let search_results = if let View::Search { query, .. } = &view {
            archive
                .search(query)?
                .into_iter()
                .map(|result| ResultRow {
                    document: result.document(),
                    created: result.created(),
                    label: result.label().to_owned(),
                    context: result.context().to_owned(),
                    byte: result.occurrence().start,
                })
                .collect()
        } else {
            Vec::new()
        };
        let conflicts = archive.conflicts()?;
        let view = if conflicts.is_empty() {
            view
        } else {
            View::Conflicts { selected: 0 }
        };
        Ok(Self {
            archive,
            view,
            editor,
            mode: AppMode::Editing,
            status: String::new(),
            scroll,
            search_results,
            history: Vec::new(),
            work_history: Vec::new(),
            trash: None,
            conflicts,
            scheduler: Scheduler::new(now),
            leap: LeapRuntime::default(),
            quit: false,
            last_leap_span: None,
            cat_span_fixed: None,
            rehighlight_span: None,
            typed_span_start: None,
            cat_erase_forward: true,
            back: Vec::new(),
            forward: Vec::new(),
            provisional,
            pending_wipe: None,
            work_positions: session.map_or_else(BTreeMap::new, |s| s.work_positions.clone()),
            work_mru: session.map_or_else(Vec::new, |s| s.work_mru.clone()),
        })
    }

    pub fn session(&self) -> Session {
        let position = self.editor.current_document().map(|document| Position {
            document,
            byte: self.editor.cursor().byte,
            scroll: self.scroll,
        });
        let view = match self.view {
            View::Work(id) => SavedView::Work { id },
            View::Chronological(v) => SavedView::Chronological {
                year: v.year(),
                month: v.month(),
            },
            View::Search {
                ref query,
                selected,
            } => SavedView::Search {
                query: query.clone(),
                selected,
            },
            _ => SavedView::Chronological {
                year: current_volume().year(),
                month: current_volume().month(),
            },
        };
        Session {
            view,
            position,
            work_positions: self.work_positions.clone(),
            work_mru: self.work_mru.clone(),
        }
    }

    pub fn commands(&self) -> Vec<Command> {
        use Command::*;
        let editable = matches!(self.view, View::Chronological(_) | View::Work(_));
        let has_doc = self.editor.current_document().is_some();
        let mut commands = vec![
            NewDocument,
            CreateWork,
            OpenWork,
            SearchArchive,
            ShowTrash,
            Import,
            Package,
            CreateCheckpoint,
            Back,
            Forward,
            Quit,
            ShowConflicts,
        ];
        if matches!(
            self.view,
            View::Chronological(_) | View::Work(_) | View::Search { .. }
        ) && !self.editor.regions().is_empty()
        {
            commands.extend([
                LeapForward,
                LeapBackward,
                LeapAgainForward,
                LeapAgainBackward,
            ]);
        }
        if editable && has_doc {
            commands.extend([
                DuplicateAsNew,
                NewLinkedDocument,
                ShowMemberships,
                AddToWork,
                InsertLink,
                ShowBacklinks,
                History,
                InsertDateTime,
                Trash,
                ExportDocumentMarkdown,
                ExportDocumentPdf,
            ]);
        }
        if editable && self.editor.can_undo() {
            commands.push(Undo);
        }
        if editable && self.editor.can_redo() {
            commands.push(Redo);
        }
        if matches!(self.view, View::Chronological(_)) {
            commands.extend([PreviousMonth, NextMonth, GoToMonth]);
        }
        if matches!(self.view, View::Work(_)) {
            commands.extend([
                OpenChronologicalView,
                RenameWork,
                TrashWork,
                WorkHistory,
                RemoveFromWork,
                MoveEarlier,
                MoveLater,
                MoveAfter,
                ExportWorkMarkdown,
                ExportWorkPdf,
            ]);
        }
        if has_doc && self.link_under_cursor().is_some() {
            commands.push(OpenLink);
        }
        if matches!(self.view, View::History { .. }) {
            commands.extend([ReturnToPreviousView, RestoreThisVersion, RestoreAsNew]);
        }
        if matches!(self.view, View::WorkHistory { .. }) {
            commands.extend([ReturnToPreviousView, RestoreWorkVersion]);
        }
        if matches!(self.view, View::Trash { .. }) {
            commands.extend([RestoreTrash, Wipe]);
        }
        if let View::Conflicts { selected } = self.view {
            if let Some(conflict) = self.conflicts.get(selected) {
                commands.extend([UseLocal, UseExternal]);
                if matches!(conflict, Conflict::Document(_)) {
                    commands.extend([UseLocalPreserveOther, UseExternalPreserveOther]);
                }
            }
        }
        commands
    }

    pub fn palette_commands(&self, query: &str) -> Vec<Command> {
        self.commands()
            .into_iter()
            .filter(|c| palette::matches(query, c.label()))
            .collect()
    }
    pub fn open_palette(&mut self) {
        self.mode = AppMode::Palette {
            query: String::new(),
            selected: 0,
        };
    }
    pub fn cancel_mode(&mut self) {
        self.mode = AppMode::Editing;
        self.status.clear();
    }

    pub fn execute(&mut self, command: Command) -> AppResult {
        use Command::*;
        match command {
            NewDocument => self.new_document()?,
            DuplicateAsNew => {
                self.autosave_for_destructive()?;
                let id = self.archive.duplicate_document(self.current_document()?)?;
                self.structural("Duplicated Document")?;
                self.open_document(id, true)?;
            }
            NewLinkedDocument => {
                self.autosave_for_destructive()?;
                let source = self.current_document()?;
                let id = self.archive.new_linked_document(
                    source,
                    self.editor.cursor().byte,
                    "Continue in…",
                )?;
                self.structural("Created linked Document")?;
                self.open_document(id, true)?;
            }
            PreviousMonth => self.change_month(-1)?,
            NextMonth => self.change_month(1)?,
            GoToMonth => self.prompt("Go to month (YYYY-MM)", PromptAction::GoToMonth),
            CreateWork => self.prompt("Work name", PromptAction::CreateWork),
            RenameWork => self.prompt("New Work name", PromptAction::RenameWork),
            OpenWork => self.select_works(SelectAction::OpenWork, false),
            OpenChronologicalView => self.open_chronological_view()?,
            ShowMemberships => self.show_memberships()?,
            AddToWork => self.select_works(SelectAction::AddToWork, true),
            RemoveFromWork => {
                let work = self.current_work()?;
                let doc = self.current_document()?;
                self.archive.remove_document_from_work(work, doc)?;
                self.structural("Removed Document from Work")?;
                self.reload_view(None)?;
            }
            MoveEarlier => self.move_work(true)?,
            MoveLater => self.move_work(false)?,
            MoveAfter => self.select_move_after()?,
            SearchArchive => {
                self.autosave_for_destructive()?;
                self.prompt("Search Archive", PromptAction::Search)
            }
            InsertLink => self.select_links()?,
            OpenLink => self.open_link()?,
            ShowBacklinks => {
                self.autosave_for_destructive()?;
                self.show_backlinks()?
            }
            Back => self.navigate(false)?,
            Forward => self.navigate(true)?,
            History => self.show_history()?,
            WorkHistory => self.show_work_history()?,
            ReturnToPreviousView => self.navigate(false)?,
            RestoreThisVersion => self.prepare_history_restore()?,
            RestoreAsNew => self.restore_history(true)?,
            RestoreWorkVersion => self.prepare_work_history_restore()?,
            Trash => self.prepare_trash()?,
            TrashWork => self.prepare_trash_work()?,
            ShowTrash => self.show_trash()?,
            RestoreTrash => self.restore_trash()?,
            Wipe => self.prepare_wipe()?,
            Import => self.prompt("Import .md or .txt path", PromptAction::Import),
            ExportDocumentMarkdown => {
                self.autosave_for_destructive()?;
                let label = self
                    .editor
                    .current_text()
                    .map(derived_label_from_text)
                    .unwrap_or_else(|| "document".to_owned());
                self.prompt_prefilled(
                    "Export Markdown path",
                    format!("{}.md", sanitize_filename(&label)),
                    PromptAction::ExportDocumentMarkdown,
                )
            }
            ExportDocumentPdf => {
                self.autosave_for_destructive()?;
                let label = self
                    .editor
                    .current_text()
                    .map(derived_label_from_text)
                    .unwrap_or_else(|| "document".to_owned());
                self.prompt_prefilled(
                    "Export PDF path",
                    format!("{}.pdf", sanitize_filename(&label)),
                    PromptAction::ExportDocumentPdf,
                )
            }
            ExportWorkMarkdown => {
                self.autosave_for_destructive()?;
                let title = self.archive.work(self.current_work()?).unwrap().title();
                self.prompt_prefilled(
                    "Export Work Markdown path",
                    format!("{}.md", sanitize_filename(title)),
                    PromptAction::ExportWorkMarkdown,
                )
            }
            ExportWorkPdf => {
                self.autosave_for_destructive()?;
                let title = self.archive.work(self.current_work()?).unwrap().title();
                self.prompt_prefilled(
                    "Export Work PDF path",
                    format!("{}.pdf", sanitize_filename(title)),
                    PromptAction::ExportWorkPdf,
                )
            }
            Package => self.prompt("Package .cat path", PromptAction::Package),
            CreateCheckpoint => self.prompt("Checkpoint note (optional)", PromptAction::Checkpoint),
            InsertDateTime => {
                let timestamp = Local::now().format("%Y-%m-%d %H:%M").to_string();
                if self.cat_insert(&timestamp) {
                    self.edited(Instant::now());
                }
            }
            LeapForward => self.start_leap(LeapDirection::Forward, true),
            LeapBackward => self.start_leap(LeapDirection::Backward, true),
            LeapAgainForward => self.leap_again(LeapDirection::Forward),
            LeapAgainBackward => self.leap_again(LeapDirection::Backward),
            Undo => {
                if self.editor.undo() {
                    self.cat_navigation();
                    self.edited(Instant::now());
                }
            }
            Redo => {
                if self.editor.redo() {
                    self.cat_navigation();
                    self.edited(Instant::now());
                }
            }
            Quit => self.finish_quit()?,
            ShowConflicts => self.show_conflicts()?,
            UseLocal => self.prompt("Type USE LOCAL to resolve", PromptAction::ResolveLocal),
            UseExternal => self.prompt(
                "Type USE EXTERNAL to resolve",
                PromptAction::ResolveExternal,
            ),
            UseLocalPreserveOther => self.prompt(
                "Type USE LOCAL AND PRESERVE EXTERNAL",
                PromptAction::ResolveLocalPreserve,
            ),
            UseExternalPreserveOther => self.prompt(
                "Type USE EXTERNAL AND PRESERVE LOCAL",
                PromptAction::ResolveExternalPreserve,
            ),
        }
        Ok(())
    }

    pub fn submit_prompt(&mut self) -> AppResult {
        let AppMode::Prompt { input, action, .. } = &self.mode else {
            return Ok(());
        };
        let input = input.clone();
        let action = *action;
        self.mode = AppMode::Editing;
        match action {
            PromptAction::Search => self.search(&input)?,
            PromptAction::GoToMonth => {
                let volume = parse_month(&input).ok_or("month must be YYYY-MM")?;
                self.switch_view(View::Chronological(volume), None, false)?;
            }
            PromptAction::CreateWork => {
                let id = self.archive.create_empty_work(input)?;
                ensure_work_color(&mut self.archive, id)?;
                self.structural("Created Work")?;
                self.switch_view(View::Work(id), None, true)?;
            }
            PromptAction::RenameWork => {
                self.archive.rename_work(self.current_work()?, input)?;
                self.structural("Renamed Work")?;
            }
            PromptAction::Import => {
                let extension = std::path::Path::new(&input)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(str::to_ascii_lowercase);
                if !matches!(extension.as_deref(), Some("md" | "txt")) {
                    return Err("Import supports only .md and .txt files".into());
                }
                let bytes = fs::read(&input)?;
                let id = self.archive.import_document_bytes(&bytes)?;
                self.structural("Imported Document")?;
                self.open_document(id, true)?;
            }
            PromptAction::ExportDocumentMarkdown => self
                .archive
                .export_document_markdown_file(self.current_document()?, input)?,
            PromptAction::ExportDocumentPdf => self.archive.export_document_pdf_with(
                self.current_document()?,
                input,
                &PdfExportOptions::default(),
            )?,
            PromptAction::ExportWorkMarkdown => self
                .archive
                .export_work_markdown_file(self.current_work()?, input)?,
            PromptAction::ExportWorkPdf => self.archive.export_work_pdf_with(
                self.current_work()?,
                input,
                &PdfExportOptions::default(),
            )?,
            PromptAction::Package => {
                self.autosave()?;
                self.archive.package(input)?;
            }
            PromptAction::Checkpoint => {
                self.autosave()?;
                self.archive
                    .checkpoint(CheckpointKind::Manual, Some(&input))?;
            }
            PromptAction::ConfirmRestoreDocument => {
                if input == "RESTORE" {
                    if let Some(id) = self.selected_trashed_document() {
                        self.archive.restore_trashed_document(id)?;
                        self.open_document(id, false)?;
                    }
                }
            }
            PromptAction::ConfirmRestoreDocumentVersion => {
                if input == "RESTORE" {
                    self.restore_history(false)?;
                }
            }
            PromptAction::ConfirmRestoreWork => {
                if input == "RESTORE" || input == "RESTORE WITH DOCUMENTS" {
                    if let Some(id) = self.selected_trashed_work() {
                        self.archive.restore_trashed_work(
                            id,
                            input == "RESTORE WITH DOCUMENTS",
                            None,
                        )?;
                        self.switch_view(View::Work(id), None, false)?;
                    }
                }
            }
            PromptAction::ConfirmRestoreWorkWithTitle => {
                if let Some(id) = self.selected_trashed_work() {
                    let (title, restore_documents) = input
                        .strip_suffix(" | WITH DOCUMENTS")
                        .map_or((input.as_str(), false), |title| (title, true));
                    if title.trim().is_empty() {
                        return Err("a unique replacement Work title is required".into());
                    }
                    self.archive.restore_trashed_work(
                        id,
                        restore_documents,
                        Some(title.to_owned()),
                    )?;
                    self.switch_view(View::Work(id), None, false)?;
                }
            }
            PromptAction::ConfirmRestoreWorkVersion => {
                if input == "RESTORE WORK" || input == "RESTORE WORK WITH DOCUMENTS" {
                    let View::WorkHistory { work, selected } = self.view else {
                        return Ok(());
                    };
                    let checkpoint = self
                        .work_history
                        .get(selected)
                        .ok_or("no historical Work revision")?
                        .checkpoint()
                        .id()
                        .clone();
                    self.archive.checkpoint(
                        CheckpointKind::Automatic,
                        Some("Saved current state before Work History restore"),
                    )?;
                    self.archive.restore_work_version(
                        work,
                        &checkpoint,
                        WorkRestoreOptions {
                            restore_required_trashed_documents: input
                                == "RESTORE WORK WITH DOCUMENTS",
                        },
                    )?;
                    self.switch_view(View::Work(work), None, false)?;
                }
            }
            PromptAction::ConfirmWipe => {
                if let Some(plan) = self.pending_wipe.take() {
                    if input == plan.confirmation_token() {
                        let document = plan.document();
                        self.archive.execute_wipe_document(&plan, &input)?;
                        self.finish_wipe(document)?;
                    } else {
                        self.status = "Wipe confirmation did not match".into();
                    }
                }
            }
            PromptAction::ResolveLocal => {
                if input == "USE LOCAL" {
                    self.resolve_selected_conflict(ConflictChoice::Local, false)?;
                }
            }
            PromptAction::ResolveExternal => {
                if input == "USE EXTERNAL" {
                    self.resolve_selected_conflict(ConflictChoice::External, false)?;
                }
            }
            PromptAction::ResolveLocalPreserve => {
                if input == "USE LOCAL AND PRESERVE EXTERNAL" {
                    self.resolve_selected_conflict(ConflictChoice::Local, true)?;
                }
            }
            PromptAction::ResolveExternalPreserve => {
                if input == "USE EXTERNAL AND PRESERVE LOCAL" {
                    self.resolve_selected_conflict(ConflictChoice::External, true)?;
                }
            }
        }
        Ok(())
    }

    pub fn submit_confirmation(&mut self, confirmed: bool) -> AppResult {
        let AppMode::Confirm { action, .. } = self.mode.clone() else {
            return Ok(());
        };
        self.mode = AppMode::Editing;
        if !confirmed {
            self.status = "Trash cancelled".into();
            return Ok(());
        }
        match action {
            ConfirmAction::TrashDocument(document) => {
                self.autosave_for_destructive()?;
                self.archive.trash_document(document)?;
                self.reload_after_removal()?;
            }
            ConfirmAction::TrashWork(work) => {
                self.autosave_for_destructive()?;
                self.archive.trash_work(work)?;
                self.switch_view(View::Chronological(current_volume()), None, false)?;
            }
        }
        Ok(())
    }

    pub fn submit_selector(&mut self) -> AppResult {
        let AppMode::Selector {
            query,
            selected,
            choices,
            action,
            ..
        } = &self.mode
        else {
            return Ok(());
        };
        let query = query.clone();
        let selected = *selected;
        let choices = choices.clone();
        let action = action.clone();
        let filtered: Vec<_> = choices
            .iter()
            .filter(|c| palette::matches(&query, &c.label))
            .collect();

        if action == SelectAction::AddToWork && filtered.is_empty() && !query.trim().is_empty() {
            let document = self.current_document()?;
            self.mode = AppMode::Editing;
            if let Some(existing) = self.archive.work_title_conflict(&query, None) {
                if self
                    .archive
                    .work(existing)
                    .is_some_and(|work| work.documents().contains(&document))
                {
                    self.status = "Document already belongs to that Work".into();
                    return Ok(());
                }
            }
            let work = self.archive.create_work(query, vec![document])?;
            ensure_work_color(&mut self.archive, work)?;
            self.structural("Created Work and added Document")?;
            self.switch_view(View::Work(work), Some((document, 0)), true)?;
            return Ok(());
        }

        let Some(choice) = filtered.get(selected.min(filtered.len().saturating_sub(1))) else {
            return Ok(());
        };
        let value = choice.value.clone();
        let label = choice.label.clone();
        self.mode = AppMode::Editing;
        match action {
            SelectAction::OpenWork => self.switch_view(View::Work(value.parse()?), None, true)?,
            SelectAction::AddToWork => {
                let work = value.parse()?;
                let document = self.current_document()?;
                self.archive.add_document_to_work(work, document)?;
                ensure_work_color(&mut self.archive, work)?;
                self.structural("Added Document to Work")?;
                self.switch_view(View::Work(work), Some((document, 0)), true)?;
            }
            SelectAction::MoveAfter => {
                let after = if value.is_empty() {
                    None
                } else {
                    Some(value.parse()?)
                };
                self.archive.move_document_after(
                    self.current_work()?,
                    self.current_document()?,
                    after,
                )?;
                self.structural("Reordered Work")?;
                self.reload_view(None)?;
            }
            SelectAction::InsertLink => self.insert_link_choice(&value, &label)?,
            _ => {}
        }
        Ok(())
    }

    pub fn move_list_selection(&mut self, down: bool) {
        match &mut self.view {
            View::Search { selected, .. } => move_index(selected, self.search_results.len(), down),
            View::History { selected, .. } => move_index(selected, self.history.len(), down),
            View::WorkHistory { selected, .. } => {
                move_index(selected, self.work_history.len(), down)
            }
            View::Trash { selected } => {
                let len = self
                    .trash
                    .as_ref()
                    .map_or(0, |t| t.documents().len() + t.works().len());
                move_index(selected, len, down);
            }
            View::Conflicts { selected } => move_index(selected, self.conflicts.len(), down),
            _ => {}
        }
        if let View::Search { selected, .. } = self.view {
            self.editor.set_cursor(
                Cursor {
                    region: selected,
                    byte: 0,
                },
                false,
            );
        }
    }

    pub fn open_selected(&mut self) -> AppResult {
        if let View::Search { selected, .. } = self.view {
            if let Some(result) = self.search_results.get(selected) {
                let target = (result.document, result.byte);
                self.open_document(target.0, true)?;
                if let Some(region) = self
                    .editor
                    .regions()
                    .iter()
                    .position(|r| r.document == target.0)
                {
                    self.editor.set_cursor(
                        Cursor {
                            region,
                            byte: target.1,
                        },
                        false,
                    );
                }
            }
        }
        Ok(())
    }

    pub fn tick(&mut self, now: Instant) -> AppResult {
        if self.scheduler.autosave_due(now, self.editor.is_dirty()) {
            self.autosave()?;
        }
        if self.scheduler.checkpoint_due(now) {
            self.autosave()?;
            self.archive.checkpoint(CheckpointKind::Automatic, None)?;
            self.scheduler.checkpointed(now);
        }
        Ok(())
    }

    pub fn edited(&mut self, now: Instant) {
        self.scheduler.edited(now);
    }

    pub fn autosave(&mut self) -> AppResult {
        self.autosave_with(|archive, document, text| archive.edit_document(document, text))
    }

    fn autosave_with(
        &mut self,
        mut save: impl FnMut(&mut Archive, DocumentId, &str) -> Result<(), carta_core::Error>,
    ) -> AppResult {
        if !self.editor.is_dirty() {
            return Ok(());
        }
        let regions = self.editor.regions().to_vec();
        let mut preserved = Vec::new();
        let mut first_error = None;
        for region in regions {
            match save(&mut self.archive, region.document, &region.text) {
                Ok(()) => {}
                Err(carta_core::Error::ConflictPreserved(id)) => {
                    preserved.push(id);
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        if first_error.is_none() {
            self.editor.mark_saved();
            self.scheduler.saved();
            if self.provisional.is_some_and(|id| {
                self.editor
                    .regions()
                    .iter()
                    .find(|region| region.document == id)
                    .is_some_and(|region| !region.text.trim().is_empty())
            }) {
                self.archive.checkpoint(
                    CheckpointKind::Structural,
                    Some("Document became persistent"),
                )?;
                self.provisional = None;
            }
        }
        if let Some(id) = preserved.first() {
            self.conflicts = self.archive.conflicts()?;
            let selected = self
                .conflicts
                .iter()
                .position(|conflict| conflict.id() == id)
                .unwrap_or(0);
            self.view = View::Conflicts { selected };
            self.status = format!(
                "External divergence preserved in {} conflict(s)",
                preserved.len()
            );
        }
        match first_error {
            Some(error) => Err(error.into()),
            None => Ok(()),
        }
    }

    fn autosave_for_destructive(&mut self) -> AppResult {
        let before = self.archive.conflicts()?.len();
        self.autosave()?;
        let after = self.archive.conflicts()?.len();
        if after > before || matches!(self.view, View::Conflicts { .. }) {
            return Err("operation aborted because autosave preserved a conflict".into());
        }
        Ok(())
    }

    fn finish_wipe(&mut self, document: DocumentId) -> AppResult {
        self.editor = CompositeEditor::new(Vec::new(), Cursor { region: 0, byte: 0 });
        self.search_results.clear();
        self.history.clear();
        self.work_history.clear();
        self.conflicts.clear();
        self.trash = None;
        self.back.clear();
        self.forward.clear();
        self.leap = LeapRuntime::default();
        self.last_leap_span = None;
        self.cat_span_fixed = None;
        self.rehighlight_span = None;
        self.typed_span_start = None;
        self.cat_erase_forward = true;
        self.pending_wipe = None;
        self.provisional = self.provisional.filter(|id| *id != document);
        self.trash = Some(self.archive.trash_inventory()?);
        self.view = View::Trash { selected: 0 };
        self.mode = AppMode::Editing;
        self.scroll = 0;
        Ok(())
    }

    fn cat_char_range_at(&self, cursor: Cursor) -> Option<(Cursor, Cursor)> {
        let region = self.editor.regions().get(cursor.region)?;
        if cursor.byte >= region.text.len() || !region.text.is_char_boundary(cursor.byte) {
            return None;
        }
        let len = region.text[cursor.byte..].chars().next()?.len_utf8();
        Some((
            cursor,
            Cursor {
                region: cursor.region,
                byte: cursor.byte + len,
            },
        ))
    }

    fn cat_previous_char_range(&self, cursor: Cursor) -> Option<(Cursor, Cursor)> {
        let region = self.editor.regions().get(cursor.region)?;
        if cursor.byte == 0
            || cursor.byte > region.text.len()
            || !region.text.is_char_boundary(cursor.byte)
        {
            return None;
        }
        let start = region.text[..cursor.byte].char_indices().next_back()?.0;
        Some((
            Cursor {
                region: cursor.region,
                byte: start,
            },
            cursor,
        ))
    }

    fn cat_origin_range(&self, cursor: Cursor) -> Option<(Cursor, Cursor)> {
        if self.cat_erase_forward {
            self.cat_char_range_at(cursor)
        } else {
            self.cat_previous_char_range(cursor)
        }
    }

    fn cat_fixed_boundary_for(&self, cursor: Cursor, direction: LeapDirection) -> Cursor {
        let range = self.cat_origin_range(cursor).unwrap_or((cursor, cursor));
        match direction {
            LeapDirection::Forward => range.0,
            LeapDirection::Backward => range.1,
        }
    }

    fn refresh_cat_span_from_fixed(&mut self) {
        let Some(fixed) = self.cat_span_fixed else {
            self.last_leap_span = None;
            return;
        };
        let cursor = self.editor.cursor();
        if fixed.region != cursor.region {
            self.last_leap_span = None;
            return;
        }
        let current = self.cat_origin_range(cursor).unwrap_or((cursor, cursor));
        let (start, end) = if fixed.byte <= current.0.byte {
            (fixed, current.1)
        } else {
            (current.0, fixed)
        };
        self.last_leap_span = (start != end).then_some((start, end));
    }

    pub fn cat_render_highlight(&self) -> Option<(Cursor, Cursor)> {
        if let Some(highlight) = self.editor.cat_highlight() {
            return Some(highlight);
        }
        let cursor = self.editor.cursor();
        if self.cat_erase_forward {
            self.cat_char_range_at(cursor)
                .or_else(|| self.cat_previous_char_range(cursor))
        } else {
            self.cat_previous_char_range(cursor)
                .or_else(|| self.cat_char_range_at(cursor))
        }
    }

    pub fn cat_insert(&mut self, value: &str) -> bool {
        let before = self.editor.cursor();
        let start = self.typed_span_start.unwrap_or(before);
        if !self.editor.insert(value) {
            return false;
        }
        let after = self.editor.cursor();
        self.typed_span_start = Some(start);
        self.cat_span_fixed = Some(start);
        self.last_leap_span =
            (start.region == after.region && start != after).then_some((start, after));
        self.rehighlight_span = None;
        self.cat_erase_forward = false;
        true
    }

    pub fn cat_insert_newline(&mut self) -> bool {
        let before = self.editor.cursor();
        let start = self.typed_span_start.unwrap_or(before);
        if !self.editor.insert_newline_with_list_continuation() {
            return false;
        }
        let after = self.editor.cursor();
        self.typed_span_start = Some(start);
        self.cat_span_fixed = Some(start);
        self.last_leap_span =
            (start.region == after.region && start != after).then_some((start, after));
        self.rehighlight_span = None;
        self.cat_erase_forward = false;
        true
    }

    pub fn cat_navigation(&mut self) {
        self.typed_span_start = None;
        self.last_leap_span = None;
        self.cat_span_fixed = None;
        self.rehighlight_span = None;
        self.cat_erase_forward = true;
    }

    pub fn cat_erase(&mut self) -> bool {
        let changed = if self.editor.cat_highlight().is_some() {
            self.editor.erase_cat_highlight()
        } else if self.cat_erase_forward {
            self.editor.delete()
        } else {
            self.editor.backspace()
        };
        if changed {
            self.typed_span_start = None;
            self.last_leap_span = None;
            self.cat_span_fixed = None;
            self.rehighlight_span = None;
            self.cat_erase_forward = true;
        }
        changed
    }

    pub fn cat_tap_leap(&mut self, direction: LeapDirection) {
        self.typed_span_start = None;
        if let Some((start, end)) = self.editor.cat_highlight() {
            self.rehighlight_span = Some((start, end));
            self.last_leap_span = Some((start, end));
            self.editor.clear_cat_highlight();
            match direction {
                LeapDirection::Forward => {
                    self.cat_span_fixed = Some(start);
                    self.editor.set_cursor_preserving_highlight(end);
                    self.cat_erase_forward = false;
                }
                LeapDirection::Backward => {
                    self.cat_span_fixed = Some(end);
                    self.editor.set_cursor_preserving_highlight(start);
                    self.cat_erase_forward = true;
                }
            }
            return;
        }

        if !self.cat_erase_forward {
            if let Some((start, _)) = self.cat_previous_char_range(self.editor.cursor()) {
                self.editor.set_cursor_preserving_highlight(start);
            }
            self.cat_erase_forward = true;
            self.refresh_cat_span_from_fixed();
            return;
        }

        self.rehighlight_span = None;
        let origin = self.editor.cursor();
        if self.cat_span_fixed.is_none() {
            self.cat_span_fixed = Some(self.cat_fixed_boundary_for(origin, direction));
        }
        self.editor
            .move_horizontal(matches!(direction, LeapDirection::Forward), false);
        self.cat_erase_forward = true;
        self.refresh_cat_span_from_fixed();
    }

    pub fn start_leap(&mut self, direction: LeapDirection, palette_mode: bool) {
        self.typed_span_start = None;
        self.rehighlight_span = None;
        if self.editor.cat_highlight().is_none() {
            self.cat_span_fixed =
                Some(self.cat_fixed_boundary_for(self.editor.cursor(), direction));
            self.last_leap_span = None;
        } else {
            self.cat_span_fixed = None;
        }
        self.cat_erase_forward = true;
        self.editor.cancel_selection();
        self.mode = AppMode::Leap {
            session: LeapSession::new(
                direction,
                LeapPosition::new(self.editor.cursor().region, self.editor.cursor().byte),
            ),
            palette: palette_mode,
        };
    }

    pub fn leap_input(&mut self, value: &str) {
        let regions: Vec<_> = self
            .editor
            .regions()
            .iter()
            .map(|r| DocumentTextRegion::new(r.document, &r.text))
            .collect();
        if let AppMode::Leap { session, .. } = &mut self.mode {
            session.push_str(value, &regions);
            let p = leap_cursor_position(session);
            self.editor.set_cursor_preserving_highlight(Cursor {
                region: p.region(),
                byte: p.byte_offset(),
            });
            if let View::Search { selected, .. } = &mut self.view {
                *selected = p.region();
            }
        }
    }

    pub fn leap_backspace(&mut self) {
        let regions: Vec<_> = self
            .editor
            .regions()
            .iter()
            .map(|r| DocumentTextRegion::new(r.document, &r.text))
            .collect();
        if let AppMode::Leap { session, .. } = &mut self.mode {
            session.backspace(&regions);
            let p = leap_cursor_position(session);
            self.editor.set_cursor_preserving_highlight(Cursor {
                region: p.region(),
                byte: p.byte_offset(),
            });
            if let View::Search { selected, .. } = &mut self.view {
                *selected = p.region();
            }
        }
    }

    pub fn remember_direct_leap_span(&mut self, origin: Cursor, direction: LeapDirection) {
        let destination = self.editor.cursor();
        self.cat_span_fixed = Some(self.cat_fixed_boundary_for(origin, direction));
        self.typed_span_start = None;
        self.rehighlight_span = None;
        self.cat_erase_forward = true;

        if origin.region == destination.region && origin != destination {
            let (start, end) = if origin.byte <= destination.byte {
                (origin, destination)
            } else {
                (destination, origin)
            };
            self.last_leap_span = Some((start, end));
            self.status.clear();
        } else {
            self.last_leap_span = None;
        }
    }

    pub fn leap_visual_boundary(&mut self, direction: LeapDirection, width: usize) {
        let origin = self.editor.cursor();
        let Some(destination) = self
            .editor
            .visual_boundary_cursor(width, matches!(direction, LeapDirection::Forward))
        else {
            self.last_leap_span = None;
            return;
        };
        let had_highlight = self.editor.cat_highlight().is_some();
        self.editor.set_cursor_preserving_highlight(destination);

        if had_highlight {
            let previous_highlight = self.editor.cat_highlight();
            if self.editor.move_cat_highlight_to(destination) {
                self.edited(Instant::now());
                self.status = "Moved highlighted text".into();
                self.cat_span_fixed = None;
                self.rehighlight_span = None;
            } else if let Some((start, end)) = previous_highlight {
                if destination.region == start.region
                    && destination.byte >= start.byte
                    && destination.byte < end.byte
                {
                    self.rehighlight_span = Some((destination, end));
                    self.cat_span_fixed = Some(end);
                }
            }
            self.last_leap_span = None;
            self.cat_erase_forward = true;
        } else if origin != destination {
            self.last_leap_span = Some((origin, destination));
            self.status.clear();
        } else {
            self.last_leap_span = None;
        }
    }

    pub fn extend_last_leap_highlight(&mut self) -> bool {
        let Some((start, end)) = self.last_leap_span.or(self.rehighlight_span) else {
            return false;
        };
        if self.editor.set_cat_highlight(start, end) {
            self.rehighlight_span = None;
            self.status.clear();
            true
        } else {
            self.status = "Cat highlight cannot cross a Document boundary".into();
            false
        }
    }

    pub fn copy_cat_highlight(&mut self) -> bool {
        if self.editor.copy_cat_highlight() {
            self.edited(Instant::now());
            self.status = "Copied highlighted text".into();
            true
        } else {
            false
        }
    }

    pub fn end_leap(&mut self) {
        let AppMode::Leap { session, palette } = &self.mode else {
            return;
        };
        let session = session.clone();
        let palette = *palette;
        let query_empty = session.query().is_empty();
        let matched = session.current_match().is_some();
        let destination = self.editor.cursor();
        let had_highlight = self.editor.cat_highlight().is_some();

        if query_empty {
            self.last_leap_span = None;
            self.cat_span_fixed = None;
            self.rehighlight_span = None;
        } else {
            self.leap.remember(&session);
            self.typed_span_start = None;
            self.rehighlight_span = None;
            if !matched {
                self.last_leap_span = None;
                self.cat_span_fixed = None;
                self.cat_erase_forward = true;
            } else if had_highlight && !palette {
                let previous_highlight = self.editor.cat_highlight();
                if self.editor.move_cat_highlight_to(destination) {
                    self.edited(Instant::now());
                    self.status = "Moved highlighted text".into();
                } else if let Some((start, end)) = previous_highlight {
                    if destination.region == start.region
                        && destination.byte >= start.byte
                        && destination.byte < end.byte
                    {
                        self.rehighlight_span = Some((destination, end));
                        self.cat_span_fixed = Some(end);
                    }
                }
                self.last_leap_span = None;
                self.cat_erase_forward = true;
            } else {
                self.cat_erase_forward = true;
                self.refresh_cat_span_from_fixed();
            }
        }
        self.mode = AppMode::Editing;
    }

    pub fn cancel_leap(&mut self) {
        if let AppMode::Leap { session, .. } = &self.mode {
            let origin = session.origin();
            self.editor.set_cursor_preserving_highlight(Cursor {
                region: origin.region(),
                byte: origin.byte_offset(),
            });
        }
        self.last_leap_span = None;
        self.cat_span_fixed = None;
        self.rehighlight_span = None;
        self.typed_span_start = None;
        self.cat_erase_forward = true;
        self.mode = AppMode::Editing;
    }

    fn leap_again(&mut self, direction: LeapDirection) {
        let origin = self.editor.cursor();
        self.cat_span_fixed = Some(self.cat_fixed_boundary_for(origin, direction));
        let regions: Vec<_> = self
            .editor
            .regions()
            .iter()
            .map(|r| DocumentTextRegion::new(r.document, &r.text))
            .collect();
        if let Some(found) = self.leap.leap_again(
            direction,
            LeapPosition::new(self.editor.cursor().region, self.editor.cursor().byte),
            &regions,
        ) {
            let p = found.position();
            let destination = Cursor {
                region: p.region(),
                byte: p.byte_offset(),
            };
            self.editor.set_cursor(destination, false);
            self.cat_erase_forward = true;
            self.refresh_cat_span_from_fixed();
            self.typed_span_start = None;
            self.rehighlight_span = None;
            if let View::Search { selected, .. } = &mut self.view {
                *selected = p.region();
            }
        }
    }

    fn new_document(&mut self) -> AppResult {
        self.autosave()?;
        if let View::Work(work) = self.view {
            let current = self.editor.current_document();
            let id = self.archive.create_document_in_work_after(work, current)?;
            self.provisional = Some(id);
            self.reload_view(Some((id, 0)))?;
        } else {
            let id = self.archive.create_document("")?;
            self.provisional = Some(id);
            self.switch_view(View::Chronological(current_volume()), Some((id, 0)), false)?;
        }
        Ok(())
    }
    fn open_chronological_view(&mut self) -> AppResult {
        let (volume, target) = match self.editor.current_document() {
            Some(document) => {
                let volume = self
                    .archive
                    .documents()
                    .find(|info| info.id() == document)
                    .ok_or("missing Document")?
                    .volume();
                (volume, Some((document, self.editor.cursor().byte)))
            }
            None => (current_volume(), None),
        };
        if self.archive.chronological_month(volume).is_empty() {
            let document = self.archive.create_document("")?;
            self.provisional = Some(document);
            self.switch_view(View::Chronological(volume), Some((document, 0)), true)
        } else {
            self.switch_view(View::Chronological(volume), target, true)
        }
    }
    fn move_work(&mut self, earlier: bool) -> AppResult {
        let work = self.current_work()?;
        let doc = self.current_document()?;
        if if earlier {
            self.archive.move_document_earlier(work, doc)?
        } else {
            self.archive.move_document_later(work, doc)?
        } {
            self.structural("Reordered Work")?;
            self.reload_view(Some((doc, self.editor.cursor().byte)))?;
        }
        Ok(())
    }
    fn structural(&mut self, note: &str) -> AppResult {
        self.autosave()?;
        self.archive
            .checkpoint(CheckpointKind::Structural, Some(note))?;
        Ok(())
    }
    fn change_month(&mut self, delta: i32) -> AppResult {
        let View::Chronological(v) = self.view else {
            return Ok(());
        };
        let total = i32::from(v.year()) * 12 + i32::from(v.month()) - 1 + delta;
        let volume = Volume::new((total / 12) as u16, (total % 12 + 1) as u8)
            .ok_or("month is outside supported range")?;
        self.switch_view(View::Chronological(volume), None, false)
    }
    fn prompt(&mut self, title: &str, action: PromptAction) {
        self.mode = AppMode::Prompt {
            title: title.into(),
            input: String::new(),
            details: Vec::new(),
            action,
        };
    }

    fn prompt_prefilled(&mut self, title: &str, input: String, action: PromptAction) {
        self.mode = AppMode::Prompt {
            title: title.into(),
            input,
            details: Vec::new(),
            action,
        };
    }

    fn prompt_with_details(&mut self, title: &str, details: Vec<String>, action: PromptAction) {
        self.mode = AppMode::Prompt {
            title: title.into(),
            input: String::new(),
            details,
            action,
        };
    }
    fn select_works(&mut self, action: SelectAction, omit_member: bool) {
        let current = self.editor.current_document();
        let mut choices: Vec<_> = self
            .archive
            .works()
            .filter(|w| !omit_member || current.is_none_or(|d| !w.documents().contains(&d)))
            .map(|w| Choice {
                label: w.title().into(),
                value: w.id().to_string(),
            })
            .collect();
        choices.sort_by_key(|choice| {
            choice
                .value
                .parse::<WorkId>()
                .ok()
                .and_then(|id| self.work_mru.iter().position(|candidate| *candidate == id))
                .unwrap_or(usize::MAX)
        });
        self.mode = AppMode::Selector {
            title: if action == SelectAction::OpenWork {
                "Open Work"
            } else {
                "Add to Work"
            }
            .into(),
            query: String::new(),
            selected: 0,
            choices,
            action,
        };
    }
    fn select_move_after(&mut self) -> AppResult {
        let work = self
            .archive
            .work(self.current_work()?)
            .ok_or("missing Work")?;
        let current = self.current_document()?;
        let mut choices = vec![Choice {
            label: "[Beginning of Work]".into(),
            value: String::new(),
        }];
        for id in work.documents().iter().filter(|id| **id != current) {
            choices.push(Choice {
                label: self.archive.read_document(*id)?.derived_label(),
                value: id.to_string(),
            });
        }
        self.mode = AppMode::Selector {
            title: "Move after".into(),
            query: String::new(),
            selected: 0,
            choices,
            action: SelectAction::MoveAfter,
        };
        Ok(())
    }
    fn select_links(&mut self) -> AppResult {
        let mut choices = Vec::new();
        for info in self.archive.documents() {
            choices.push(Choice {
                label: self.archive.read_document(info.id())?.derived_label(),
                value: format!("doc:{}", info.id()),
            });
        }
        for work in self.archive.works() {
            choices.push(Choice {
                label: work.title().into(),
                value: format!("work:{}", work.id()),
            });
        }
        self.mode = AppMode::Selector {
            title: "Insert Link".into(),
            query: String::new(),
            selected: 0,
            choices,
            action: SelectAction::InsertLink,
        };
        Ok(())
    }
    fn insert_link_choice(&mut self, value: &str, target_label: &str) -> AppResult {
        let (kind, id) = value.split_once(':').ok_or("invalid link choice")?;
        let selected = self.editor.selected_text();
        let label = selected.unwrap_or_else(|| target_label.to_owned());
        let link = format!(
            "[{}](carta:{kind}:{id})",
            escape_markdown_link_label(&label)
        );
        let replaced_highlight = self.editor.cat_highlight().is_some();
        let changed = if replaced_highlight {
            self.editor.replace_cat_highlight(&link)
        } else {
            self.cat_insert(&link)
        };
        if !changed {
            self.status = "Cannot replace a selection across Document boundaries".into();
        } else {
            if replaced_highlight {
                self.cat_navigation();
            }
            self.edited(Instant::now());
        }
        Ok(())
    }
    fn show_memberships(&mut self) -> AppResult {
        let names: Vec<_> = self
            .archive
            .memberships(self.current_document()?)?
            .into_iter()
            .filter_map(|id| self.archive.work(id).map(|w| w.title().to_owned()))
            .collect();
        self.status = if names.is_empty() {
            "No Work memberships".into()
        } else {
            names.join(", ")
        };
        Ok(())
    }
    fn search(&mut self, query: &str) -> AppResult {
        self.push_navigation();
        self.search_results = self
            .archive
            .search(query)?
            .into_iter()
            .map(|result| ResultRow {
                document: result.document(),
                created: result.created(),
                label: result.label().to_owned(),
                context: result.context().to_owned(),
                byte: result.occurrence().start,
            })
            .collect();
        self.view = View::Search {
            query: query.trim().into(),
            selected: 0,
        };
        self.load_search_editor()?;
        self.mode = AppMode::Editing;
        Ok(())
    }
    fn show_backlinks(&mut self) -> AppResult {
        let id = self.current_document()?;
        let backlinks = self.archive.backlinks(CartaLinkTarget::Document(id))?;
        let mut rows = Vec::new();
        for backlink in backlinks {
            let doc = self.archive.read_document(backlink.source())?;
            let range = backlink.link().source_range();
            let line_start = doc.content()[..range.start]
                .rfind('\n')
                .map_or(0, |i| i + 1);
            let line_end = doc.content()[range.end..]
                .find('\n')
                .map_or(doc.content().len(), |i| range.end + i);
            rows.push(ResultRow {
                document: backlink.source(),
                created: doc.metadata().created(),
                label: doc.derived_label(),
                context: doc.content()[line_start..line_end].to_owned(),
                byte: range.start,
            });
        }
        self.push_navigation();
        self.search_results = rows;
        self.view = View::Search {
            query: "Backlinks".into(),
            selected: 0,
        };
        self.load_search_editor()?;
        Ok(())
    }
    fn open_link(&mut self) -> AppResult {
        let Some(link) = self.link_under_cursor() else {
            return Ok(());
        };
        match self.archive.resolve_link(&link) {
            Some(carta_core::LinkResolution::Document(id)) => self.open_document(id, true)?,
            Some(carta_core::LinkResolution::Work(id)) => {
                self.switch_view(View::Work(id), None, true)?
            }
            Some(carta_core::LinkResolution::Unresolved(_)) => {
                self.status = "Unresolved Carta link".into()
            }
            None => self.status = "External links are not executed by this build".into(),
        }
        Ok(())
    }
    fn link_under_cursor(&self) -> Option<carta_core::MarkdownLink> {
        self.editor
            .current_text()
            .and_then(|text| carta_core::link_at_byte_offset(text, self.editor.cursor().byte))
    }
    fn show_history(&mut self) -> AppResult {
        let id = self.current_document()?;
        self.push_navigation();
        self.autosave()?;
        self.history = self.archive.document_revisions(id)?;
        self.view = View::History {
            document: id,
            selected: 0,
        };
        Ok(())
    }
    fn show_work_history(&mut self) -> AppResult {
        let work = self.current_work()?;
        self.push_navigation();
        self.autosave()?;
        let mut snapshots = Vec::new();
        for checkpoint in self.archive.history()? {
            match self.archive.work_snapshot(work, checkpoint.id()) {
                Ok(snapshot) => snapshots.push(snapshot),
                Err(carta_core::Error::MissingWorkRevision { .. }) => {}
                Err(error) => return Err(error.into()),
            }
        }
        self.work_history = snapshots;
        self.view = View::WorkHistory { work, selected: 0 };
        Ok(())
    }
    fn restore_history(&mut self, as_new: bool) -> AppResult {
        let View::History { document, selected } = self.view else {
            return Ok(());
        };
        let revision = self.history.get(selected).ok_or("no historical revision")?;
        let checkpoint = revision.checkpoint().id().clone();
        self.archive.checkpoint(
            CheckpointKind::Automatic,
            Some("Saved current state before History restore"),
        )?;
        if as_new {
            let (id, _) = self
                .archive
                .restore_document_version_as_new(document, &checkpoint)?;
            self.open_document(id, false)?;
        } else {
            self.archive
                .restore_document_version(document, &checkpoint)?;
            self.open_document(document, false)?;
        }
        Ok(())
    }
    fn prepare_history_restore(&mut self) -> AppResult {
        let View::History { document, .. } = self.view else {
            return Ok(());
        };
        let impact = self.archive.document_restore_impact(document)?;
        let mut details = vec!["This replaces the current Document content.".to_owned()];
        if !impact.affected_other_works().is_empty() {
            details.push("Other Works affected through this shared Document:".to_owned());
            details.extend(
                impact
                    .affected_other_works()
                    .iter()
                    .map(|work| format!("  {}", work.title())),
            );
        }
        self.prompt_with_details(
            "Type RESTORE to restore this historical version",
            details,
            PromptAction::ConfirmRestoreDocumentVersion,
        );
        Ok(())
    }
    fn prepare_work_history_restore(&mut self) -> AppResult {
        let View::WorkHistory { work, selected } = self.view else {
            return Ok(());
        };
        let checkpoint = self
            .work_history
            .get(selected)
            .ok_or("no historical Work revision")?
            .checkpoint()
            .id();
        let impact = self.archive.work_restore_impact(work, checkpoint)?;
        let mut details = vec![
            "This replaces the Work title, order, membership, and historical member content."
                .to_owned(),
        ];
        if !impact.affected_other_works().is_empty() {
            details.push("Other Works affected by shared Document content:".to_owned());
            details.extend(
                impact
                    .affected_other_works()
                    .iter()
                    .map(|work| format!("  {}", work.title())),
            );
        }
        self.prompt_with_details(
            "Type RESTORE WORK, or RESTORE WORK WITH DOCUMENTS if required",
            details,
            PromptAction::ConfirmRestoreWorkVersion,
        );
        Ok(())
    }
    fn prepare_trash(&mut self) -> AppResult {
        self.autosave_for_destructive()?;
        if matches!(self.view, View::Work(_)) && self.editor.current_document().is_none() {
            return self.prepare_trash_work();
        }
        let impact = self
            .archive
            .document_trash_impact(self.current_document()?)?;
        let mut details = vec!["Work references to remove:".to_owned()];
        if impact.memberships().is_empty() {
            details.push("  (none)".to_owned());
        } else {
            details.extend(
                impact
                    .memberships()
                    .iter()
                    .map(|membership| format!("  {}", membership.title())),
            );
        }
        details.push("Inbound links that will become unresolved:".to_owned());
        if impact.inbound_links().is_empty() {
            details.push("  (none)".to_owned());
        } else {
            for backlink in impact.inbound_links() {
                let source = self.archive.read_document(backlink.source())?;
                let range = backlink.link().source_range();
                let line_start = source.content()[..range.start]
                    .rfind('\n')
                    .map_or(0, |index| index + 1);
                let line_end = source.content()[range.end..]
                    .find('\n')
                    .map_or(source.content().len(), |index| range.end + index);
                details.push(format!(
                    "  {}: {}",
                    source.derived_label(),
                    source.content()[line_start..line_end].trim()
                ));
            }
        }
        let document = self.current_document()?;
        self.mode = AppMode::Confirm {
            title: "Trash Document? [y/N]".into(),
            details,
            action: ConfirmAction::TrashDocument(document),
        };
        Ok(())
    }

    fn prepare_trash_work(&mut self) -> AppResult {
        self.autosave_for_destructive()?;
        let work = self.current_work()?;
        let title = self
            .archive
            .work(work)
            .map_or_else(|| "Work".to_owned(), |work| work.title().to_owned());
        self.mode = AppMode::Confirm {
            title: "Trash Work? [y/N]".into(),
            details: vec![
                format!("Work: {title}"),
                "The Work will move to Trash. Its Documents remain in the Archive.".into(),
            ],
            action: ConfirmAction::TrashWork(work),
        };
        Ok(())
    }

    fn show_trash(&mut self) -> AppResult {
        self.push_navigation();
        self.trash = Some(self.archive.trash_inventory()?);
        self.view = View::Trash { selected: 0 };
        self.mode = AppMode::Editing;
        Ok(())
    }
    pub fn show_conflicts(&mut self) -> AppResult {
        self.conflicts = self.archive.conflicts()?;
        self.push_navigation();
        self.view = View::Conflicts { selected: 0 };
        self.mode = AppMode::Editing;
        Ok(())
    }

    pub fn reveal_conflicts(&mut self) -> AppResult<bool> {
        let conflicts = self.archive.conflicts()?;
        if conflicts.is_empty() {
            return Ok(false);
        }
        self.conflicts = conflicts;
        self.view = View::Conflicts { selected: 0 };
        self.mode = AppMode::Editing;
        Ok(true)
    }

    fn resolve_selected_conflict(
        &mut self,
        choice: ConflictChoice,
        preserve_other: bool,
    ) -> AppResult {
        let View::Conflicts { selected } = self.view else {
            return Ok(());
        };
        let conflict = self
            .conflicts
            .get(selected)
            .cloned()
            .ok_or("no selected conflict")?;
        match conflict {
            Conflict::Document(conflict) => {
                self.archive
                    .resolve_document_conflict(conflict.id(), choice, preserve_other)?;
            }
            Conflict::Work(conflict) => {
                if preserve_other {
                    return Err("Work conflicts cannot create a second Work automatically".into());
                }
                self.archive.resolve_work_conflict(conflict.id(), choice)?;
            }
        }
        self.archive.checkpoint(
            CheckpointKind::Structural,
            Some("Resolved external divergence"),
        )?;
        self.conflicts = self.archive.conflicts()?;
        if self.conflicts.is_empty() {
            self.switch_view(View::Chronological(current_volume()), None, false)?;
        } else {
            self.view = View::Conflicts {
                selected: selected.min(self.conflicts.len() - 1),
            };
        }
        Ok(())
    }
    fn restore_trash(&mut self) -> AppResult {
        if self.selected_trashed_document().is_some() {
            self.prompt(
                "Type RESTORE to restore Document without memberships",
                PromptAction::ConfirmRestoreDocument,
            );
        } else if self.selected_trashed_work().is_some() {
            let work = self
                .trash
                .as_ref()
                .and_then(|trash| {
                    let View::Trash { selected } = self.view else {
                        return None;
                    };
                    selected
                        .checked_sub(trash.documents().len())
                        .and_then(|index| trash.works().get(index))
                })
                .ok_or("no selected trashed Work")?;
            if self
                .archive
                .work_title_conflict(work.title(), None)
                .is_some()
            {
                self.prompt_with_details(
                    "Replacement unique Work title",
                    vec![
                        "The historical title conflicts with an active Work.".to_owned(),
                        "Append ` | WITH DOCUMENTS` to consent to restoring missing members."
                            .to_owned(),
                    ],
                    PromptAction::ConfirmRestoreWorkWithTitle,
                );
            } else {
                self.prompt(
                    "Type RESTORE, or RESTORE WITH DOCUMENTS if required",
                    PromptAction::ConfirmRestoreWork,
                );
            }
        }
        Ok(())
    }
    fn prepare_wipe(&mut self) -> AppResult {
        let id = self
            .selected_trashed_document()
            .ok_or("select a trashed Document; Works cannot be wiped")?;
        self.autosave()?;
        self.archive
            .checkpoint(CheckpointKind::Structural, Some("Prepared Wipe"))?;
        let plan = self.archive.plan_wipe_document(id)?;
        let token = plan.confirmation_token().to_owned();
        let guarantee = plan.guarantee().to_owned();
        self.status = guarantee.clone();
        self.pending_wipe = Some(plan);
        self.mode = AppMode::Prompt {
            title: format!("Type exactly: {token}"),
            input: String::new(),
            details: vec![guarantee],
            action: PromptAction::ConfirmWipe,
        };
        Ok(())
    }

    fn load_search_editor(&mut self) -> AppResult {
        let regions = self
            .search_results
            .iter()
            .map(|result| Region {
                document: result.document,
                text: search_result_view_text(result),
            })
            .collect();
        self.editor = CompositeEditor::new(regions, Cursor { region: 0, byte: 0 });
        Ok(())
    }
    fn selected_trashed_document(&self) -> Option<DocumentId> {
        let View::Trash { selected } = self.view else {
            return None;
        };
        self.trash
            .as_ref()?
            .documents()
            .get(selected)
            .map(|d| d.id())
    }
    fn selected_trashed_work(&self) -> Option<WorkId> {
        let View::Trash { selected } = self.view else {
            return None;
        };
        let trash = self.trash.as_ref()?;
        selected
            .checked_sub(trash.documents().len())
            .and_then(|i| trash.works().get(i))
            .map(|w| w.id())
    }
    fn reload_after_removal(&mut self) -> AppResult {
        self.archive.refresh()?;
        self.switch_view(View::Chronological(current_volume()), None, false)
    }
    fn finish_quit(&mut self) -> AppResult {
        self.autosave()?;
        self.archive.checkpoint(CheckpointKind::Quit, None)?;
        self.quit = true;
        Ok(())
    }
    fn current_document(&self) -> AppResult<DocumentId> {
        self.editor
            .current_document()
            .ok_or_else(|| "no current Document".into())
    }
    fn current_work(&self) -> AppResult<WorkId> {
        match self.view {
            View::Work(id) => Ok(id),
            _ => Err("not in a Work".into()),
        }
    }
    fn open_document(&mut self, id: DocumentId, navigation: bool) -> AppResult {
        if matches!(self.view, View::Chronological(_) | View::Work(_)) {
            if let Some(index) = self.editor.regions().iter().position(|r| r.document == id) {
                if navigation {
                    self.push_navigation();
                }
                self.editor.set_cursor(
                    Cursor {
                        region: index,
                        byte: 0,
                    },
                    false,
                );
                return Ok(());
            }
        }
        let volume = self
            .archive
            .documents()
            .find(|d| d.id() == id)
            .ok_or("missing Document")?
            .volume();
        self.switch_view(View::Chronological(volume), Some((id, 0)), navigation)
    }
    fn switch_view(
        &mut self,
        view: View,
        target: Option<(DocumentId, usize)>,
        navigation: bool,
    ) -> AppResult {
        self.autosave()?;
        if let View::Work(work) = self.view {
            if let Some(document) = self.editor.current_document() {
                self.work_positions.insert(
                    work,
                    Position {
                        document,
                        byte: self.editor.cursor().byte,
                        scroll: self.scroll,
                    },
                );
            }
        }
        if navigation {
            self.push_navigation();
        }
        if let View::Work(work) = &view {
            if ensure_work_color(&mut self.archive, *work)? {
                self.archive.checkpoint(
                    CheckpointKind::Structural,
                    Some("Assigned persistent Work color"),
                )?;
            }
        }
        self.view = view;
        let remembered = match (&self.view, target) {
            (View::Work(work), None) => self.work_positions.get(work).cloned(),
            (_, Some((document, byte))) => Some(Position {
                document,
                byte,
                scroll: 0,
            }),
            _ => None,
        };
        let target = remembered
            .as_ref()
            .map(|position| (position.document, position.byte));
        if let View::Work(work) = self.view {
            self.work_mru.retain(|candidate| *candidate != work);
            self.work_mru.insert(0, work);
        }
        self.reload_view(target)?;
        if let Some(position) = remembered {
            self.scroll = position.scroll;
        }
        self.forward.clear();
        Ok(())
    }
    fn reload_view(&mut self, target: Option<(DocumentId, usize)>) -> AppResult {
        let position = target.map(|(document, byte)| Position {
            document,
            byte,
            scroll: 0,
        });
        let (editor, scroll) = load_editor(&self.archive, &self.view, position.as_ref())?;
        self.editor = editor;
        self.last_leap_span = None;
        self.cat_span_fixed = None;
        self.rehighlight_span = None;
        self.typed_span_start = None;
        self.cat_erase_forward = true;
        self.scroll = scroll;
        Ok(())
    }
    fn push_navigation(&mut self) {
        let location = Location {
            view: self.view.clone(),
            cursor: self.editor.cursor(),
            scroll: self.scroll,
        };
        if self.back.last() != Some(&location) {
            self.back.push(location);
        }
        self.forward.clear();
    }
    fn navigate(&mut self, forward: bool) -> AppResult {
        self.autosave()?;
        let source = if forward {
            &mut self.forward
        } else {
            &mut self.back
        };
        let Some(location) = source.pop() else {
            return Ok(());
        };
        let current = Location {
            view: self.view.clone(),
            cursor: self.editor.cursor(),
            scroll: self.scroll,
        };
        if forward {
            self.back.push(current);
        } else {
            self.forward.push(current);
        }
        self.view = location.view;
        self.reload_view(None)?;
        self.editor.set_cursor(location.cursor, false);
        self.scroll = location.scroll;
        Ok(())
    }
}

fn load_editor(
    archive: &Archive,
    view: &View,
    position: Option<&Position>,
) -> AppResult<(CompositeEditor, usize)> {
    let regions = match view {
        View::Search { query, .. } => archive
            .search(query)?
            .into_iter()
            .map(|result| {
                let row = ResultRow {
                    document: result.document(),
                    created: result.created(),
                    label: result.label().to_owned(),
                    context: result.context().to_owned(),
                    byte: result.occurrence().start,
                };
                Region {
                    document: row.document,
                    text: search_result_view_text(&row),
                }
            })
            .collect(),
        View::Chronological(v) => load_document_regions(archive, archive.chronological_month(*v))?,
        View::Work(id) => load_document_regions(
            archive,
            archive
                .work(*id)
                .map(|work| work.documents().to_vec())
                .unwrap_or_default(),
        )?,
        _ => Vec::new(),
    };
    let mut cursor = Cursor { region: 0, byte: 0 };
    let mut scroll = 0;
    if let Some(position) = position {
        if let Some(region) = regions.iter().position(|r| r.document == position.document) {
            cursor = Cursor {
                region,
                byte: position.byte,
            };
            scroll = position.scroll;
        }
    }
    Ok((CompositeEditor::new(regions, cursor), scroll))
}

fn load_document_regions(archive: &Archive, documents: Vec<DocumentId>) -> AppResult<Vec<Region>> {
    documents
        .into_iter()
        .map(|document| {
            Ok(Region {
                document,
                text: archive.read_document(document)?.content().to_owned(),
            })
        })
        .collect()
}

fn search_result_view_text(result: &ResultRow) -> String {
    format!(
        "{}\n{}\n{}",
        result.label,
        result
            .created
            .to_string()
            .chars()
            .take(10)
            .collect::<String>(),
        result.context
    )
}
fn ensure_work_colors(archive: &mut Archive) -> AppResult<bool> {
    let works: Vec<_> = archive.works().map(|work| work.id()).collect();
    let mut changed = false;
    for work in works {
        changed |= ensure_work_color(archive, work)?;
    }
    Ok(changed)
}

fn ensure_work_color(archive: &mut Archive, work: WorkId) -> AppResult<bool> {
    if archive.work(work).and_then(|work| work.color()).is_some() {
        return Ok(false);
    }

    let mut usage = [0_usize; WORK_COLOR_PALETTE.len()];
    for existing in archive.works() {
        if let Some(color) = existing.color() {
            if let Some(index) = WORK_COLOR_PALETTE
                .iter()
                .position(|candidate| candidate.eq_ignore_ascii_case(color))
            {
                usage[index] += 1;
            }
        }
    }
    let index = usage
        .iter()
        .enumerate()
        .min_by_key(|(index, count)| (**count, *index))
        .map_or(0, |(index, _)| index);
    archive.set_work_color(work, Some(WORK_COLOR_PALETTE[index].to_owned()))?;
    Ok(true)
}

fn leap_cursor_position(session: &LeapSession) -> LeapPosition {
    session
        .current_match()
        .map_or(session.origin(), |found| found.position())
}

fn current_volume() -> Volume {
    let now = Local::now();
    Volume::new(now.year() as u16, now.month() as u8).unwrap()
}
fn parse_month(value: &str) -> Option<Volume> {
    let (year, month) = value.trim().split_once('-')?;
    Volume::new(year.parse().ok()?, month.parse().ok()?)
}
fn move_index(index: &mut usize, len: usize, down: bool) {
    if len == 0 {
        *index = 0;
    } else if down {
        *index = (*index + 1).min(len - 1);
    } else {
        *index = index.saturating_sub(1);
    }
}

fn escape_markdown_link_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('\n', " ")
}

fn derived_label_from_text(content: &str) -> String {
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('#').trim())
        .filter(|line| !line.is_empty())
        .unwrap_or("document")
        .to_owned()
}

fn sanitize_filename(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '-',
            character if character.is_control() => '-',
            character => character,
        })
        .collect::<String>();
    let sanitized = sanitized.trim().trim_matches('.');
    if sanitized.is_empty() {
        "document".to_owned()
    } else {
        sanitized.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scheduler_waits_for_idle_and_checkpoint_interval() {
        let now = Instant::now();
        let mut s = Scheduler::new(now);
        s.edited(now);
        assert!(!s.autosave_due(now + Duration::from_millis(999), true));
        assert!(s.autosave_due(now + Duration::from_secs(1), true));
        assert!(s.checkpoint_due(now + Duration::from_secs(600)));
    }

    #[test]
    fn app_resumes_saved_view_cursor_without_creating_another_document() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("archive");
        let archive = Archive::create(&root).unwrap();
        let now = Instant::now();
        let mut app = App::open(archive, None, now).unwrap();
        assert!(app.editor.insert("résumé"));
        app.autosave().unwrap();
        let session = app.session();
        let count = app.archive.documents().count();
        drop(app);

        let reopened = Archive::open(&root).unwrap();
        let resumed = App::open(reopened, Some(&session), now).unwrap();
        assert_eq!(resumed.archive.documents().count(), count);
        assert_eq!(resumed.editor.cursor().byte, "résumé".len());
    }

    #[test]
    fn empty_chronological_restart_creates_a_selected_provisional_document() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let stale_document = DocumentId::new_v7();
        let session = Session {
            view: SavedView::Chronological {
                year: current_volume().year(),
                month: current_volume().month(),
            },
            position: Some(Position {
                document: stale_document,
                byte: 10,
                scroll: 20,
            }),
            work_positions: BTreeMap::new(),
            work_mru: Vec::new(),
        };

        let app = App::open(archive, Some(&session), Instant::now()).unwrap();

        assert_eq!(app.editor.regions().len(), 1);
        assert_ne!(app.editor.current_document(), Some(stale_document));
        assert_eq!(app.provisional, app.editor.current_document());
        assert_eq!(app.editor.cursor(), Cursor { region: 0, byte: 0 });
    }

    #[test]
    fn first_character_after_empty_chronological_restart_is_editable() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let session = Session::new(current_volume());
        let mut app = App::open(archive, Some(&session), Instant::now()).unwrap();

        assert!(app.editor.insert("x"));
        app.autosave().unwrap();

        let document = app.editor.current_document().unwrap();
        assert_eq!(app.archive.read_document(document).unwrap().content(), "x");
        assert!(app.provisional.is_none());
    }

    #[test]
    fn empty_old_month_restart_moves_provisional_to_current_month() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let old = Volume::new(1900, 1).unwrap();
        let session = Session::new(old);

        let app = App::open(archive, Some(&session), Instant::now()).unwrap();

        assert_eq!(app.view, View::Chronological(current_volume()));
        let document = app.editor.current_document().unwrap();
        assert_eq!(
            app.archive
                .documents()
                .find(|info| info.id() == document)
                .unwrap()
                .volume(),
            current_volume()
        );
    }

    #[test]
    fn empty_work_restart_remains_empty_and_creates_no_document() {
        let temporary = tempfile::tempdir().unwrap();
        let mut archive = Archive::create(temporary.path().join("archive")).unwrap();
        let work = archive.create_empty_work("Empty".into()).unwrap();
        let session = Session {
            view: SavedView::Work { id: work },
            position: None,
            work_positions: BTreeMap::new(),
            work_mru: Vec::new(),
        };

        let app = App::open(archive, Some(&session), Instant::now()).unwrap();

        assert_eq!(app.view, View::Work(work));
        assert!(app.editor.regions().is_empty());
        assert_eq!(app.archive.documents().count(), 0);
        assert!(app.provisional.is_none());
    }

    #[test]
    fn app_assigns_and_persists_stable_work_colors() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("archive");
        let mut archive = Archive::create(&root).unwrap();
        let first = archive.create_empty_work("First".into()).unwrap();
        let second = archive.create_empty_work("Second".into()).unwrap();
        assert!(archive.work(first).unwrap().color().is_none());
        assert!(archive.work(second).unwrap().color().is_none());

        let app = App::open(archive, None, Instant::now()).unwrap();
        let first_color = app.archive.work(first).unwrap().color().unwrap().to_owned();
        let second_color = app
            .archive
            .work(second)
            .unwrap()
            .color()
            .unwrap()
            .to_owned();
        assert_ne!(first_color, second_color);
        assert!(first_color.starts_with('#'));
        assert_eq!(first_color.len(), 7);
        drop(app);

        let reopened = Archive::open(&root).unwrap();
        assert_eq!(
            reopened.work(first).unwrap().color(),
            Some(first_color.as_str())
        );
        assert_eq!(
            reopened.work(second).unwrap().color(),
            Some(second_color.as_str())
        );
    }

    #[test]
    fn leap_dispatch_is_incremental_and_remembers_query() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        assert!(app.editor.insert("alpha beta alpha"));
        app.editor.end(false);
        app.start_leap(LeapDirection::Backward, false);
        app.leap_input("alpha");
        assert_eq!(app.editor.cursor().byte, 11);
        app.end_leap();
        assert_eq!(app.leap.remembered_query(), Some("alpha"));
    }

    #[test]
    fn palette_leap_updates_incrementally_and_escape_restores_origin() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        assert!(app.editor.insert("alpha beta alpha"));
        app.editor.end(false);
        let origin = app.editor.cursor();

        app.start_leap(LeapDirection::Backward, true);
        assert!(matches!(app.mode, AppMode::Leap { palette: true, .. }));
        app.leap_input("alpha");
        assert_eq!(app.editor.cursor().byte, 11);
        for _ in 0..5 {
            app.leap_backspace();
        }
        assert_eq!(app.editor.cursor(), origin);
        app.leap_input("beta");
        assert_eq!(app.editor.cursor().byte, 6);
        app.cancel_leap();
        assert_eq!(app.editor.cursor(), origin);
    }

    #[test]
    fn insert_link_uses_and_escapes_target_or_selected_label() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let target = DocumentId::new_v7();
        app.insert_link_choice(&format!("doc:{target}"), "A [label] \\ ok")
            .unwrap();
        assert_eq!(
            app.editor.current_text().unwrap(),
            format!("[A \\[label\\] \\\\ ok](carta:doc:{target})")
        );

        let length = app.editor.current_text().unwrap().len();
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.editor.set_cursor(
            Cursor {
                region: 0,
                byte: length,
            },
            true,
        );
        let work = WorkId::new_v7();
        app.insert_link_choice(&format!("work:{work}"), "ignored")
            .unwrap();
        assert!(app.editor.current_text().unwrap().starts_with("[\\[A"));
    }

    #[test]
    fn provisional_document_checkpoints_only_after_non_whitespace_autosave() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        assert_eq!(app.archive.history().unwrap().len(), 1);
        assert!(app.editor.insert("   "));
        app.autosave().unwrap();
        assert_eq!(app.archive.history().unwrap().len(), 1);
        assert!(app.editor.insert("x"));
        app.autosave().unwrap();
        let history = app.archive.history().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].kind(), Some(CheckpointKind::Structural));
    }

    #[test]
    fn new_document_in_work_stays_provisional_until_non_whitespace_autosave() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let first = app.current_document().unwrap();
        assert!(app.editor.insert("persistent"));
        app.autosave().unwrap();
        let work = app.archive.create_work("Work".into(), vec![first]).unwrap();
        app.structural("Created test Work").unwrap();
        app.switch_view(View::Work(work), None, false).unwrap();
        assert!(!app.archive.is_dirty().unwrap());
        assert!(app.archive.work(work).unwrap().color().is_some());
        let before = app.archive.history().unwrap().len();

        app.new_document().unwrap();
        assert_eq!(app.archive.history().unwrap().len(), before);
        assert_eq!(app.archive.work(work).unwrap().documents().len(), 2);
        assert!(app.archive.is_dirty().unwrap());
        assert!(app.editor.insert("   "));
        app.autosave().unwrap();
        assert_eq!(app.archive.history().unwrap().len(), before);
        assert!(app.editor.insert("x"));
        app.autosave().unwrap();
        assert_eq!(app.archive.history().unwrap().len(), before + 1);
        assert!(!app.archive.is_dirty().unwrap());
    }

    #[test]
    fn trash_confirmation_describes_each_work_and_inbound_link() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let target = app.current_document().unwrap();
        assert!(app.editor.insert("# Target"));
        app.autosave().unwrap();
        app.archive
            .create_work("My Work".into(), vec![target])
            .unwrap();
        app.archive
            .create_document(&format!("Source context [Target](carta:doc:{target}) end"))
            .unwrap();

        app.prepare_trash().unwrap();
        let AppMode::Confirm { details, .. } = &app.mode else {
            panic!("expected y/n confirmation")
        };
        let text = details.join("\n");
        assert!(text.contains("My Work"));
        assert!(text.contains("Source context"));
        assert!(text.contains("[Target](carta:doc:"));
    }

    #[test]
    fn trash_confirmation_yes_executes_and_no_cancels() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let document = app.current_document().unwrap();
        assert!(app.editor.insert("keep or trash"));
        app.autosave().unwrap();

        app.prepare_trash().unwrap();
        app.submit_confirmation(false).unwrap();
        assert!(app.archive.read_document(document).is_ok());

        app.prepare_trash().unwrap();
        app.submit_confirmation(true).unwrap();
        assert!(app.archive.read_document(document).is_err());
    }

    #[test]
    fn add_to_work_creates_missing_work_with_current_document() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let document = app.current_document().unwrap();

        app.select_works(SelectAction::AddToWork, true);
        let AppMode::Selector { query, .. } = &mut app.mode else {
            panic!("expected Work selector")
        };
        *query = "New Dogfooding Work".into();
        app.submit_selector().unwrap();

        let work = app
            .archive
            .works()
            .find(|work| work.title() == "New Dogfooding Work")
            .expect("new Work should exist");
        let work_id = work.id();
        assert_eq!(work.documents(), &[document]);
        assert_eq!(app.view, View::Work(work_id));
        assert_eq!(app.editor.current_document(), Some(document));
    }

    #[test]
    fn add_to_existing_work_opens_that_work_view() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let document = app.current_document().unwrap();
        let work = app
            .archive
            .create_empty_work("Existing Work".into())
            .unwrap();
        app.structural("Created existing test Work").unwrap();

        app.select_works(SelectAction::AddToWork, true);
        let AppMode::Selector { query, .. } = &mut app.mode else {
            panic!("expected Work selector")
        };
        *query = "Existing Work".into();
        app.submit_selector().unwrap();

        assert_eq!(app.archive.work(work).unwrap().documents(), &[document]);
        assert_eq!(app.view, View::Work(work));
        assert_eq!(app.editor.current_document(), Some(document));
    }

    #[test]
    fn autosave_exposes_durable_conflict_and_resolution() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let document = app.current_document().unwrap();
        assert!(app.editor.insert("base"));
        app.autosave().unwrap();
        let path = app
            .archive
            .documents()
            .find(|info| info.id() == document)
            .unwrap()
            .path()
            .join("content.md");
        fs::write(&path, "external").unwrap();
        assert!(app.editor.insert(" local"));

        app.autosave().unwrap();
        assert!(matches!(app.view, View::Conflicts { .. }));
        assert_eq!(app.conflicts.len(), 1);
        app.resolve_selected_conflict(ConflictChoice::Local, false)
            .unwrap();
        assert_eq!(
            app.archive.read_document(document).unwrap().content(),
            "base local"
        );
        assert!(app.archive.conflicts().unwrap().is_empty());
    }

    #[test]
    fn autosave_preserves_early_conflict_and_saves_later_dirty_region() {
        let temporary = tempfile::tempdir().unwrap();
        let mut archive = Archive::create(temporary.path().join("archive")).unwrap();
        let first = archive.create_document("first").unwrap();
        let second = archive.create_document("second").unwrap();
        archive
            .checkpoint(CheckpointKind::Structural, Some("Two Documents"))
            .unwrap();
        let volume = archive
            .documents()
            .find(|info| info.id() == first)
            .unwrap()
            .volume();
        let mut app = App::open(
            archive,
            Some(&Session {
                view: SavedView::Chronological {
                    year: volume.year(),
                    month: volume.month(),
                },
                position: Some(Position {
                    document: first,
                    byte: 5,
                    scroll: 0,
                }),
                work_positions: BTreeMap::new(),
                work_mru: Vec::new(),
            }),
            Instant::now(),
        )
        .unwrap();
        let first_region = app
            .editor
            .regions()
            .iter()
            .position(|region| region.document == first)
            .unwrap();
        let second_region = app
            .editor
            .regions()
            .iter()
            .position(|region| region.document == second)
            .unwrap();
        app.editor.set_cursor(
            Cursor {
                region: first_region,
                byte: 5,
            },
            false,
        );
        assert!(app.editor.insert(" local"));
        app.editor.set_cursor(
            Cursor {
                region: second_region,
                byte: 6,
            },
            false,
        );
        assert!(app.editor.insert(" saved"));
        let first_path = app
            .archive
            .documents()
            .find(|info| info.id() == first)
            .unwrap()
            .path()
            .join("content.md");
        fs::write(first_path, "first external").unwrap();

        app.autosave().unwrap();

        assert!(matches!(app.view, View::Conflicts { .. }));
        assert_eq!(app.archive.conflicts().unwrap().len(), 1);
        assert_eq!(
            app.archive.read_document(second).unwrap().content(),
            "second saved"
        );
        assert!(!app.editor.is_dirty());
    }

    #[test]
    fn autosave_continues_after_ordinary_region_error_and_stays_dirty() {
        let temporary = tempfile::tempdir().unwrap();
        let mut archive = Archive::create(temporary.path().join("archive")).unwrap();
        let first = archive.create_document("first").unwrap();
        let second = archive.create_document("second").unwrap();
        archive
            .checkpoint(CheckpointKind::Structural, Some("Two Documents"))
            .unwrap();
        let volume = archive
            .documents()
            .find(|info| info.id() == first)
            .unwrap()
            .volume();
        let mut app = App::open(
            archive,
            Some(&Session {
                view: SavedView::Chronological {
                    year: volume.year(),
                    month: volume.month(),
                },
                position: None,
                work_positions: BTreeMap::new(),
                work_mru: Vec::new(),
            }),
            Instant::now(),
        )
        .unwrap();
        let first_region = app
            .editor
            .regions()
            .iter()
            .position(|region| region.document == first)
            .unwrap();
        let second_region = app
            .editor
            .regions()
            .iter()
            .position(|region| region.document == second)
            .unwrap();
        app.editor.set_cursor(
            Cursor {
                region: first_region,
                byte: 5,
            },
            false,
        );
        assert!(app.editor.insert(" local"));
        app.editor.set_cursor(
            Cursor {
                region: second_region,
                byte: 6,
            },
            false,
        );
        assert!(app.editor.insert(" durable"));

        let error = app
            .autosave_with(|archive, document, text| {
                if document == first {
                    Err(carta_core::Error::ExternalChange("injected".into()))
                } else {
                    archive.edit_document(document, text)
                }
            })
            .unwrap_err();

        assert!(error.to_string().contains("injected"));
        assert_eq!(
            app.archive.read_document(second).unwrap().content(),
            "second durable"
        );
        assert_eq!(app.archive.read_document(first).unwrap().content(), "first");
        assert!(app.editor.is_dirty());
    }

    #[test]
    fn successful_wipe_scrubs_editor_and_undo_buffers_and_emits_completion() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let target = app.current_document().unwrap();
        assert!(app.editor.insert("sensitive text"));
        app.autosave().unwrap();
        app.archive
            .checkpoint(CheckpointKind::Manual, Some("Sensitive history"))
            .unwrap();
        let work = app
            .archive
            .create_work("Sensitive Work".into(), vec![target])
            .unwrap();
        let work_checkpoint = app
            .archive
            .checkpoint(CheckpointKind::Structural, Some("Sensitive Work state"))
            .unwrap()
            .unwrap();
        app.search_results.push(ResultRow {
            document: target,
            created: app
                .archive
                .read_document(target)
                .unwrap()
                .metadata()
                .created(),
            label: "sensitive text".into(),
            context: "sensitive text".into(),
            byte: 0,
        });
        app.history = app.archive.document_revisions(target).unwrap();
        app.work_history.push(
            app.archive
                .work_snapshot(work, work_checkpoint.id())
                .unwrap(),
        );
        app.back.push(Location {
            view: View::History {
                document: target,
                selected: 0,
            },
            cursor: Cursor { region: 0, byte: 0 },
            scroll: 0,
        });
        app.forward.push(Location {
            view: View::Search {
                query: "sensitive text".into(),
                selected: 0,
            },
            cursor: Cursor { region: 0, byte: 0 },
            scroll: 0,
        });
        app.archive.trash_document(target).unwrap();
        app.show_trash().unwrap();
        app.prepare_wipe().unwrap();
        let token = app
            .pending_wipe
            .as_ref()
            .unwrap()
            .confirmation_token()
            .to_owned();
        let AppMode::Prompt { input, .. } = &mut app.mode else {
            panic!("expected Wipe confirmation")
        };
        *input = token;

        app.submit_prompt().unwrap();

        assert!(app
            .editor
            .regions()
            .iter()
            .all(|region| region.document != target));
        assert!(!app.editor.undo());
        assert!(app.search_results.is_empty());
        assert!(app.history.is_empty());
        assert!(app.work_history.is_empty());
        assert!(app.conflicts.iter().all(|conflict| !matches!(
            conflict,
            Conflict::Document(value) if value.document() == target
        )));
        assert!(app.back.is_empty());
        assert!(app.forward.is_empty());
        assert!(matches!(app.view, View::Trash { selected: 0 }));
    }

    #[test]
    fn search_session_resume_and_back_navigation_remain_stable() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("archive");
        let mut archive = Archive::create(&root).unwrap();
        archive.create_document("needle here").unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        app.search("needle").unwrap();
        let session = app.session();
        drop(app);

        let archive = Archive::open(&root).unwrap();
        let mut resumed = App::open(archive, Some(&session), Instant::now()).unwrap();
        assert!(
            matches!(resumed.view, View::Search { ref query, selected: 0 } if query == "needle")
        );
        assert_eq!(resumed.search_results.len(), 1);
        resumed.open_selected().unwrap();
        resumed.execute(Command::Back).unwrap();
        assert!(
            matches!(resumed.view, View::Search { ref query, selected: 0 } if query == "needle")
        );
        assert_eq!(resumed.search_results.len(), 1);
    }

    #[test]
    fn search_leap_uses_only_rendered_rows_and_updates_selection_and_again() {
        let temporary = tempfile::tempdir().unwrap();
        let mut archive = Archive::create(temporary.path().join("archive")).unwrap();
        archive
            .create_document("seed target-context\nhidden-only")
            .unwrap();
        archive.create_document("seed newest").unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        app.search("seed").unwrap();
        assert_eq!(app.search_results.len(), 2);

        app.start_leap(LeapDirection::Forward, true);
        app.leap_input("hidden-only");
        let AppMode::Leap { session, .. } = &app.mode else {
            panic!("expected LEAP mode")
        };
        assert!(session.current_match().is_none());
        assert_eq!(app.editor.cursor().region, 0);
        app.cancel_leap();

        app.start_leap(LeapDirection::Forward, true);
        app.leap_input("target-context");
        assert_eq!(app.editor.cursor().region, 1);
        assert!(matches!(app.view, View::Search { selected: 1, .. }));
        app.end_leap();

        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        if let View::Search { selected, .. } = &mut app.view {
            *selected = 0;
        }
        app.leap_again(LeapDirection::Forward);
        assert_eq!(app.editor.cursor().region, 1);
        assert!(matches!(app.view, View::Search { selected: 1, .. }));
    }

    #[test]
    fn leap_cursor_lands_on_the_target_character_in_both_directions() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        assert!(app.editor.insert("alpha beta alpha"));
        app.editor.set_cursor(Cursor { region: 0, byte: 6 }, false);

        app.start_leap(LeapDirection::Forward, true);
        app.leap_input("alpha");
        assert_eq!(app.editor.cursor().byte, 11);
        app.end_leap();

        app.editor.set_cursor(Cursor { region: 0, byte: 6 }, false);
        app.start_leap(LeapDirection::Backward, true);
        app.leap_input("alpha");
        assert_eq!(app.editor.cursor().byte, 0);
        app.end_leap();
    }

    #[test]
    fn directional_leap_again_advances_from_directional_cursor() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        assert!(app.editor.insert("one one one"));

        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.start_leap(LeapDirection::Forward, true);
        app.leap_input("one");
        assert_eq!(app.editor.cursor().byte, 0);
        app.end_leap();
        app.leap_again(LeapDirection::Forward);
        assert_eq!(app.editor.cursor().byte, 4);
        app.leap_again(LeapDirection::Forward);
        assert_eq!(app.editor.cursor().byte, 8);

        app.editor.set_cursor(
            Cursor {
                region: 0,
                byte: 11,
            },
            false,
        );
        app.start_leap(LeapDirection::Backward, true);
        app.leap_input("one");
        assert_eq!(app.editor.cursor().byte, 8);
        app.end_leap();
        app.leap_again(LeapDirection::Backward);
        assert_eq!(app.editor.cursor().byte, 4);
    }

    #[test]
    fn cat_tap_leap_creeps_and_unhighlight_can_rehighlight() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        assert!(app.editor.insert("abcd"));
        app.editor.set_cursor(Cursor { region: 0, byte: 1 }, false);
        app.cat_erase_forward = true;

        app.cat_tap_leap(LeapDirection::Forward);
        assert_eq!(app.editor.cursor().byte, 2);

        assert!(app
            .editor
            .set_cat_highlight(Cursor { region: 0, byte: 1 }, Cursor { region: 0, byte: 4 },));
        app.cat_tap_leap(LeapDirection::Backward);
        assert_eq!(app.editor.cursor().byte, 1);
        assert!(app.editor.cat_highlight().is_none());
        assert!(app.extend_last_leap_highlight());
        assert_eq!(
            app.editor.cat_highlight(),
            Some((Cursor { region: 0, byte: 1 }, Cursor { region: 0, byte: 4 }))
        );
    }

    #[test]
    fn cat_typing_can_be_highlighted_and_erase_is_directional() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();

        assert!(app.cat_insert("abc"));
        assert!(!app.cat_erase_forward);
        assert!(app.extend_last_leap_highlight());
        assert_eq!(app.editor.selected_text().as_deref(), Some("abc"));
        assert!(app.cat_erase());
        assert_eq!(app.editor.current_text(), Some(""));

        assert!(app.cat_insert("xy"));
        assert!(app.cat_erase());
        assert_eq!(app.editor.current_text(), Some("x"));

        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.cat_navigation();
        assert!(app.cat_erase());
        assert_eq!(app.editor.current_text(), Some(""));
    }

    #[test]
    fn unhighlight_creep_and_rehighlight_adjust_the_active_end() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        assert!(app.editor.insert("abcdef"));
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.cat_navigation();

        app.start_leap(LeapDirection::Forward, false);
        app.leap_input("c");
        assert_eq!(app.editor.cursor().byte, 2);
        app.end_leap();
        assert!(app.extend_last_leap_highlight());
        assert_eq!(app.editor.selected_text().as_deref(), Some("abc"));

        app.cat_tap_leap(LeapDirection::Forward);
        assert_eq!(app.editor.cursor().byte, 3);
        app.cat_tap_leap(LeapDirection::Forward);
        assert_eq!(app.editor.cursor().byte, 2);
        app.cat_tap_leap(LeapDirection::Forward);
        assert_eq!(app.editor.cursor().byte, 3);
        assert!(app.extend_last_leap_highlight());
        assert_eq!(app.editor.selected_text().as_deref(), Some("abcd"));
    }

    #[test]
    fn creep_after_leap_resizes_pending_highlight_before_extension() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        assert!(app.editor.insert("abcdef"));
        app.editor.set_cursor(Cursor { region: 0, byte: 0 }, false);
        app.cat_navigation();

        app.start_leap(LeapDirection::Forward, false);
        app.leap_input("c");
        app.end_leap();
        assert_eq!(app.editor.cursor().byte, 2);

        app.cat_tap_leap(LeapDirection::Forward);
        assert_eq!(app.editor.cursor().byte, 3);
        assert!(app.extend_last_leap_highlight());
        assert_eq!(app.editor.selected_text().as_deref(), Some("abcd"));
    }

    #[test]
    fn first_creep_after_typing_only_narrows_the_cat_cursor() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();

        assert!(app.cat_insert("abc"));
        assert_eq!(app.editor.cursor().byte, 3);
        app.cat_tap_leap(LeapDirection::Forward);
        assert_eq!(app.editor.cursor().byte, 2);
        assert!(app.extend_last_leap_highlight());
        assert_eq!(app.editor.selected_text().as_deref(), Some("abc"));
    }

    #[test]
    fn erased_leap_pattern_returns_to_origin_without_creeping() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        assert!(app.editor.insert("abc abc"));
        app.editor.set_cursor(Cursor { region: 0, byte: 1 }, false);
        app.cat_navigation();

        app.start_leap(LeapDirection::Forward, false);
        app.leap_input("a");
        app.leap_backspace();
        assert_eq!(app.editor.cursor().byte, 1);
        app.end_leap();

        assert_eq!(app.editor.cursor().byte, 1);
        assert!(app.last_leap_span.is_none());
        assert!(app.cat_span_fixed.is_none());
    }

    #[test]
    fn failed_leap_rebounds_without_creating_an_extended_span() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        assert!(app.editor.insert("abcdef"));
        app.editor.set_cursor(Cursor { region: 0, byte: 2 }, false);
        app.cat_navigation();

        app.start_leap(LeapDirection::Forward, false);
        app.leap_input("zzz");
        assert_eq!(app.editor.cursor().byte, 2);
        app.end_leap();

        assert_eq!(app.editor.cursor().byte, 2);
        assert!(app.last_leap_span.is_none());
        assert!(app.cat_span_fixed.is_none());
    }

    #[test]
    fn work_document_opens_its_volume_in_chronological_view() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let document = app.current_document().unwrap();
        assert!(app.editor.insert("document text"));
        app.autosave().unwrap();
        let work = app
            .archive
            .create_work("Work".into(), vec![document])
            .unwrap();
        app.switch_view(View::Work(work), Some((document, "document ".len())), false)
            .unwrap();

        app.execute(Command::OpenChronologicalView).unwrap();

        let volume = app
            .archive
            .documents()
            .find(|info| info.id() == document)
            .unwrap()
            .volume();
        assert_eq!(app.view, View::Chronological(volume));
        assert_eq!(app.editor.current_document(), Some(document));
        assert_eq!(app.editor.cursor().byte, "document ".len());
    }

    #[test]
    fn empty_work_opens_a_writable_current_chronological_view() {
        let temporary = tempfile::tempdir().unwrap();
        let mut archive = Archive::create(temporary.path().join("archive")).unwrap();
        let work = archive.create_empty_work("Empty".into()).unwrap();
        let session = Session {
            view: SavedView::Work { id: work },
            position: None,
            work_positions: BTreeMap::new(),
            work_mru: Vec::new(),
        };
        let mut app = App::open(archive, Some(&session), Instant::now()).unwrap();

        app.execute(Command::OpenChronologicalView).unwrap();

        assert_eq!(app.view, View::Chronological(current_volume()));
        assert_eq!(app.editor.regions().len(), 1);
        assert!(app.editor.insert("writable"));
    }

    #[test]
    fn create_work_records_chronological_view_for_back_navigation() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let chronological = app.view.clone();

        app.execute(Command::CreateWork).unwrap();
        let AppMode::Prompt { input, .. } = &mut app.mode else {
            panic!("expected Work name prompt")
        };
        *input = "New Work".into();
        app.submit_prompt().unwrap();
        assert!(matches!(app.view, View::Work(_)));
        app.execute(Command::Back).unwrap();

        assert_eq!(app.view, chronological);
    }

    #[test]
    fn open_work_records_chronological_view_for_back_navigation() {
        let temporary = tempfile::tempdir().unwrap();
        let mut archive = Archive::create(temporary.path().join("archive")).unwrap();
        let work = archive.create_empty_work("Other Work".into()).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let chronological = app.view.clone();

        app.execute(Command::OpenWork).unwrap();
        app.submit_selector().unwrap();
        assert_eq!(app.view, View::Work(work));
        app.execute(Command::Back).unwrap();

        assert_eq!(app.view, chronological);
    }

    #[test]
    fn work_chronological_back_and_forward_restore_each_view() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let document = app.current_document().unwrap();
        let work = app
            .archive
            .create_work("Work".into(), vec![document])
            .unwrap();
        app.switch_view(View::Work(work), None, false).unwrap();

        app.execute(Command::OpenChronologicalView).unwrap();
        app.execute(Command::Back).unwrap();
        assert_eq!(app.view, View::Work(work));
        app.execute(Command::Forward).unwrap();

        assert!(matches!(app.view, View::Chronological(_)));
        assert_eq!(app.editor.current_document(), Some(document));
    }

    #[test]
    fn palette_inserts_current_local_date_and_time() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();

        assert!(app.commands().contains(&Command::InsertDateTime));
        app.execute(Command::InsertDateTime).unwrap();

        let text = app.editor.current_text().unwrap();
        assert_eq!(text.len(), 16);
        assert_eq!(&text[4..5], "-");
        assert_eq!(&text[7..8], "-");
        assert_eq!(&text[10..11], " ");
        assert_eq!(&text[13..14], ":");
        assert!(app.editor.is_dirty());
    }

    #[test]
    fn history_has_explicit_return_to_previous_view() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let original_view = app.view.clone();
        assert!(app.editor.insert("history test"));
        app.autosave().unwrap();
        app.archive
            .checkpoint(CheckpointKind::Manual, Some("test"))
            .unwrap();

        app.execute(Command::History).unwrap();
        assert!(matches!(app.view, View::History { .. }));
        assert!(app.commands().contains(&Command::ReturnToPreviousView));

        app.execute(Command::ReturnToPreviousView).unwrap();
        assert_eq!(app.view, original_view);
    }

    #[test]
    fn palette_undo_redo_mark_dirty_and_autosave() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let mut app = App::open(archive, None, Instant::now()).unwrap();
        let document = app.current_document().unwrap();
        assert!(app.editor.insert("first"));
        app.autosave().unwrap();
        assert!(app.editor.insert(" second"));
        assert!(app.commands().contains(&Command::Undo));
        assert!(app.palette_commands("").contains(&Command::Undo));

        app.execute(Command::Undo).unwrap();
        assert_eq!(app.editor.current_text(), Some("first"));
        assert!(app.editor.is_dirty());
        assert!(app.commands().contains(&Command::Redo));
        assert!(app.palette_commands("").contains(&Command::Redo));
        app.tick(Instant::now() + Duration::from_secs(2)).unwrap();
        assert_eq!(
            app.archive.read_document(document).unwrap().content(),
            "first"
        );

        app.execute(Command::Redo).unwrap();
        assert_eq!(app.editor.current_text(), Some("first second"));
        assert!(app.editor.is_dirty());
        app.tick(Instant::now() + Duration::from_secs(2)).unwrap();
        assert_eq!(
            app.archive.read_document(document).unwrap().content(),
            "first second"
        );
    }
}
