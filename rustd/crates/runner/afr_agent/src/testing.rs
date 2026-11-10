//! Checkpoints for the suites that run a loop without a daemon.
//!
//! [`Discard`] drops every checkpoint, for a suite that does not read them.
//! [`Recording`] hands each one to a receiver, so a suite proves when the loop
//! wrote its memory back and what it carried.

use std::sync::mpsc::{self, Receiver, Sender};

use af_wire::memory::MemoryDelta;

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

/// A per-test private working-context store, with real generation semantics.
#[derive(Debug, Default)]
pub struct ContextMemory(
    std::sync::Mutex<std::collections::HashMap<String, af_context::Checkpoint>>,
);

impl ContextMemory {
    /// The checkpoint `branch` holds, as a test reads back what a run
    /// published; none on a poisoned store.
    #[must_use]
    pub fn stored(&self, branch: &str) -> Option<af_context::Checkpoint> {
        self.0.lock().ok()?.get(branch).cloned()
    }
}

#[async_trait::async_trait]
impl crate::ContextCheckpoints for ContextMemory {
    async fn commit(
        &self,
        branch: &str,
        expected: u64,
        checkpoint: af_context::Checkpoint,
    ) -> af_context::Result<()> {
        let mut entries = self
            .0
            .lock()
            .map_err(|_poisoned| af_context::Error::stale())?;
        if entries.get(branch).map_or(0, |entry| entry.generation) != expected {
            return Err(af_context::Error::stale());
        }
        entries.insert(branch.into(), checkpoint);
        Ok(())
    }
}

/// An inbox no message ever reaches: a run that folds nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoInbox;

#[async_trait::async_trait]
impl crate::Inbox for NoInbox {
    async fn claim(&self, _after_sequence: i64) -> Vec<crate::Folded> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "controlled poisoned-store fixture"
    )]
    use super::ContextMemory;
    use crate::ContextCheckpoints as _;
    use std::sync::Arc;

    // Value: a poisoned store cannot replace the last accepted checkpoint.
    #[tokio::test]
    async fn a_poisoned_context_store_refuses_publication() {
        let store = Arc::new(ContextMemory::default());
        let checkpoint: af_context::Checkpoint = serde_json::from_value(serde_json::json!({
            "format_version":1,
            "identity":{"scope":"private-run","model":"selected-model","pinned":"revision"},
            "generation":1,"covered_through":2,"source_digest":"a".repeat(64),
            "summary":{"goal":"private goal","constraints":[],"decisions":[],
                "completed":[],"remaining":[],"references":[]}
        }))
        .unwrap();
        store.commit("branch", 0, checkpoint.clone()).await.unwrap();
        let poisoned = Arc::clone(&store);
        assert!(
            std::thread::spawn(move || {
                let _guard = poisoned.0.lock().unwrap();
                panic!("controlled failure while holding the fixture store");
            })
            .join()
            .is_err()
        );
        assert!(
            store
                .commit("branch", 1, checkpoint.clone())
                .await
                .unwrap_err()
                .is_stale()
        );
        assert_eq!(
            store.0.lock().unwrap_err().get_ref().get("branch"),
            Some(&checkpoint)
        );
    }
}
