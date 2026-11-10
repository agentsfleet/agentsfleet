//! Private run working state, separate from reusable semantic memory.
use af_context::{Checkpoint, Result};

/// Durable effects supplied by the supervisor under the live run fence.
#[async_trait::async_trait]
pub trait ContextCheckpoints: Send + Sync + std::fmt::Debug {
    /// Publish one complete summary against the generation read before sending.
    ///
    /// # Errors
    /// The assignment or generation changed; the existing checkpoint remains.
    async fn commit(&self, branch: &str, expected: u64, checkpoint: Checkpoint) -> Result<()>;
}
