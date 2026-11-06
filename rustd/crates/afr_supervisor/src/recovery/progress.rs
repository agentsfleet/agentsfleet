//! One operation owns deduplicated ciphertext accounting and update cadence.

use afd_wire::activity::{TaskActivityPhase, TaskTransferBytes};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
enum State {
    Missing(u64),
    Acknowledged,
    Reused,
}

#[derive(Debug, Default)]
pub(super) struct Progress {
    objects: BTreeMap<String, State>,
    pub(super) bytes: TaskTransferBytes,
    pub(super) completed: u32,
    pub(super) total: Option<u32>,
    last: Option<(TaskActivityPhase, Instant)>,
}

impl Progress {
    pub(super) fn discover(&mut self, id: &str, bytes: u64, reused: bool) {
        if self.objects.contains_key(id) {
            return;
        }
        let state = if reused {
            self.bytes.reused += bytes;
            self.completed += 1;
            State::Reused
        } else {
            self.bytes.missing += bytes;
            State::Missing(bytes)
        };
        self.objects.insert(id.to_owned(), state);
    }

    pub(super) fn acknowledge(&mut self, id: &str) {
        if let Some(state @ State::Missing(_)) = self.objects.get_mut(id) {
            let State::Missing(bytes) = *state else {
                return;
            };
            self.bytes.missing -= bytes;
            self.bytes.acknowledged += bytes;
            self.completed += 1;
            *state = State::Acknowledged;
        }
    }

    pub(super) fn due(&mut self, phase: TaskActivityPhase) -> bool {
        if self.last.is_some_and(|(previous, at)| {
            previous == phase && at.elapsed() < Duration::from_secs(1)
        }) {
            return false;
        }
        self.last = Some((phase, Instant::now()));
        true
    }
}
