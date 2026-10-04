//! Checkpoints for the suites that run a loop without a daemon.
//!
//! [`Discard`] drops every checkpoint, for a suite that does not read them.
//! [`Recording`] hands each one to a receiver, so a suite proves when the loop
//! wrote its memory back and what it carried.

use std::sync::mpsc::{self, Receiver, Sender};

use afd_wire::memory::MemoryDelta;

use crate::engine::Checkpoint;

/// A checkpoint that writes nothing anywhere.
#[derive(Debug, Clone, Copy, Default)]
pub struct Discard;

#[async_trait::async_trait]
impl Checkpoint for Discard {
    async fn push(&self, _memory: Vec<MemoryDelta<'static>>) -> crate::Result<()> {
        Ok(())
    }
}

/// A checkpoint that hands each push to its receiver.
#[derive(Debug)]
pub struct Recording {
    pushed: Sender<Vec<MemoryDelta<'static>>>,
}

impl Recording {
    /// A checkpoint, and where each push it is handed arrives.
    #[must_use]
    pub fn new() -> (Self, Receiver<Vec<MemoryDelta<'static>>>) {
        let (pushed, received) = mpsc::channel();
        (Self { pushed }, received)
    }
}

#[async_trait::async_trait]
impl Checkpoint for Recording {
    async fn push(&self, memory: Vec<MemoryDelta<'static>>) -> crate::Result<()> {
        // A suite that dropped its receiver asserts nothing about checkpoints.
        let _unread = self.pushed.send(memory);
        Ok(())
    }
}
