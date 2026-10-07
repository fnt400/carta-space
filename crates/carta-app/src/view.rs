use crate::HelpKind;
use carta_core::{DocumentId, Volume, WorkId};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    CreationDate(Volume),
    ModificationDate,
    Work(WorkId),
    Search { query: String, selected: usize },
    History { document: DocumentId, selected: usize },
    WorkHistory { work: WorkId, selected: usize },
    Trash { selected: usize },
    Conflicts { selected: usize },
    Help { kind: HelpKind, selected: usize },
}

pub struct Scheduler {
    last_edit: Option<Instant>,
    last_checkpoint: Instant,
    last_sync: Instant,
    autosave_after: Duration,
    checkpoint_after: Duration,
    sync_after: Duration,
    sync_pending: bool,
}

impl Scheduler {
    pub fn new(now: Instant) -> Self {
        Self {
            last_edit: None,
            last_checkpoint: now,
            last_sync: now,
            autosave_after: Duration::from_secs(1),
            checkpoint_after: Duration::from_secs(600),
            sync_after: Duration::from_secs(180),
            sync_pending: false,
        }
    }

    pub fn edited(&mut self, now: Instant) { self.last_edit = Some(now); }

    pub fn autosave_due(&self, now: Instant, dirty: bool) -> bool {
        dirty
            && self
                .last_edit
                .is_some_and(|last| now.duration_since(last) >= self.autosave_after)
    }

    pub fn checkpoint_due(&self, now: Instant) -> bool {
        now.duration_since(self.last_checkpoint) >= self.checkpoint_after
    }

    pub fn saved(&mut self) { self.last_edit = None; }

    pub fn checkpointed(&mut self, now: Instant) { self.last_checkpoint = now; }

    pub fn sync_due(&self, now: Instant) -> bool {
        self.sync_pending || now.duration_since(self.last_sync) >= self.sync_after
    }

    pub fn sync_pending(&mut self) { self.sync_pending = true; }

    pub fn is_sync_pending(&self) -> bool { self.sync_pending }

    pub fn sync_attempted(&mut self, now: Instant) {
        self.last_sync = now;
        self.sync_pending = false;
    }
}
