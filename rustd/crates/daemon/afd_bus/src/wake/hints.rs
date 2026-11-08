//! Ordered reminders with constant expected membership and bounded storage.

use std::collections::{HashSet, VecDeque};

use af_core::id::Uuid7;
use af_observability::metrics::label::fleet::ReadyHint;

#[derive(Debug)]
pub(super) struct Hints {
    queue: VecDeque<Uuid7>,
    members: HashSet<Uuid7>,
    capacity: usize,
}

impl Hints {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            members: HashSet::with_capacity(capacity),
            capacity,
        }
    }

    pub(super) fn ring(&mut self, fleet: Uuid7) -> ReadyHint {
        if self.members.contains(&fleet) {
            return ReadyHint::Duplicate;
        }
        let outcome = if self.queue.len() == self.capacity {
            self.take();
            ReadyHint::Evicted
        } else {
            ReadyHint::Added
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

    pub(super) fn clear(&mut self) {
        self.queue.clear();
        self.members.clear();
    }
}
