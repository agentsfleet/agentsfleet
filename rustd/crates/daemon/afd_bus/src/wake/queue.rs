//! Ordered reminders with constant expected membership and bounded storage.

use std::collections::{HashSet, VecDeque};

use af_core::id::Uuid7;
use af_observability::metrics::label::fleet::WakeQueueOutcome;

#[derive(Debug)]
pub(super) struct WakeQueue {
    queue: VecDeque<Uuid7>,
    members: HashSet<Uuid7>,
    capacity: usize,
    recovery_generation: u64,
    recovery_pending: bool,
}

impl WakeQueue {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            members: HashSet::with_capacity(capacity),
            capacity,
            recovery_generation: 0,
            recovery_pending: false,
        }
    }

    pub(super) fn recheck(&mut self) {
        self.recovery_generation = self.recovery_generation.wrapping_add(1);
        self.recovery_pending = true;
    }

    pub(super) fn recovery(&self) -> Option<super::Recovery> {
        self.recovery_pending
            .then_some(super::Recovery(self.recovery_generation))
    }

    pub(super) fn recovered(&mut self, recovery: super::Recovery) {
        if recovery.0 == self.recovery_generation {
            self.recovery_pending = false;
        }
    }

    pub(super) fn ring(&mut self, fleet: Uuid7) -> WakeQueueOutcome {
        if self.members.contains(&fleet) {
            return WakeQueueOutcome::Duplicate;
        }
        let outcome = if self.queue.len() == self.capacity {
            self.take();
            WakeQueueOutcome::Evicted
        } else {
            WakeQueueOutcome::Added
        };
        self.members.insert(fleet.clone());
        self.queue.push_back(fleet);
        outcome
    }

    pub(super) fn take(&mut self) -> Option<Uuid7> {
        let fleet = self.queue.pop_front()?;
        self.members.remove(&fleet);
        Some(fleet)
    }

    pub(super) fn fleets(&self) -> HashSet<Uuid7> {
        self.members.clone()
    }

    pub(super) fn clear(&mut self) {
        self.queue.clear();
        self.members.clear();
    }
}
