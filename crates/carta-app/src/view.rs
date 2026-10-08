use crate::HelpKind;
use carta_core::{DocumentId, Volume, WorkId};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    CreationDate(Volume),
    ModificationDate,
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
    Help {
        kind: HelpKind,
        selected: usize,
    },
}

pub struct Scheduler {
    last_edit: Option<Instant>,
    last_checkpoint: Instant,
    last_sync: Instant,
    autosave_after: Duration,
    checkpoint_after: Duration,
    sync_after: Duration,
    sync_pending: bool,
    sync_generation: u64,
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
            sync_generation: 0,
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

    pub fn sync_due(&self, now: Instant) -> bool {
        self.sync_pending || now.duration_since(self.last_sync) >= self.sync_after
    }

    pub fn sync_pending(&mut self) {
        self.sync_pending = true;
        self.sync_generation = self.sync_generation.wrapping_add(1);
    }

    /// Tracks commits queued while an earlier background transfer is running.
    pub fn sync_generation(&self) -> u64 {
        self.sync_generation
    }

    /// A successful push may acknowledge only the commits known when it
    /// started; a later commit must remain queued for its own push.
    pub fn sync_finished(&mut self, started_generation: u64, now: Instant) {
        self.last_sync = now;
        if self.sync_generation == started_generation {
            self.sync_pending = false;
        }
    }

    pub fn is_sync_pending(&self) -> bool {
        self.sync_pending
    }

    pub fn sync_attempted(&mut self, now: Instant) {
        self.last_sync = now;
        self.sync_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_made_during_push_remains_queued_after_earlier_push_succeeds() {
        let start = Instant::now();
        let mut scheduler = Scheduler::new(start);
        scheduler.sync_pending();
        let first_generation = scheduler.sync_generation();
        scheduler.sync_pending(); // Another checkpoint while Git was working.
        scheduler.sync_finished(first_generation, start + Duration::from_secs(1));
        assert!(scheduler.is_sync_pending(), "second commit must still be pushed");
        let second_generation = scheduler.sync_generation();
        scheduler.sync_finished(second_generation, start + Duration::from_secs(2));
        assert!(!scheduler.is_sync_pending());
    }

    #[test]
    fn failed_push_can_retry_without_acknowledging_pending_checkpoint() {
        let start = Instant::now();
        let mut scheduler = Scheduler::new(start);
        scheduler.sync_pending();
        let generation = scheduler.sync_generation();
        // A failed worker does NOT call sync_finished.
        assert!(scheduler.is_sync_pending());
        scheduler.sync_finished(generation, start + Duration::from_secs(3));
        assert!(!scheduler.is_sync_pending());
    }
}
