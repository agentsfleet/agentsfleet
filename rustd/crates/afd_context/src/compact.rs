//! Validation precedes publication; cancellation never reverses model accounting.

use crate::{Checkpoint, CheckpointStore, Group, Identity, Result, Summarizer, Summary};

/// One bounded summary request selected by the shared planner.
#[derive(Debug, Clone)]
pub struct Compaction {
    /// Scope, model and trusted pinned-state identity.
    pub identity: Identity,
    /// The store generation observed before the physical send.
    pub expected_generation: u64,
    /// Digest of the previous covered prefix, empty on the first compaction.
    pub prior_digest: String,
    /// The previous untrusted summary, when continuing a checkpoint.
    pub prior_summary: Option<Summary>,
    /// Whole source groups; adapters must not split opaque replay/tool pairs.
    pub groups: Vec<Group>,
    /// Finite output, also subject to the host's remaining budget.
    pub output_tokens: u32,
}

/// Observations carry counts only, never source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Complete history groups read from the authoritative source.
    Read(usize),
    /// About to spend one physical model call.
    Summarizing,
    /// One validated checkpoint was conditionally published.
    Committed(u64),
}

impl Compaction {
    /// The inclusive source watermark of this prepared update.
    #[must_use]
    pub fn covered_through(&self) -> u64 {
        self.groups.last().map_or(0, |group| group.sequence)
    }

    /// Quote source data for the tools-disabled summary request.
    ///
    /// # Errors
    /// Source data cannot be serialized.
    pub fn input(&self) -> Result<String> {
        Ok(serde_json::to_string(&serde_json::json!({
            "previous_summary": self.prior_summary,
            "conversation_groups": self.groups,
        }))?)
    }

    /// Consumes one prepared update and invokes its publication callback once.
    /// The callback still owns durable generation and live-authority checks.
    ///
    /// # Errors
    /// A stale generation or rejected publication leaves the old checkpoint intact.
    pub async fn finish_with<F, Fut>(self, summary: Summary, commit: F) -> Result<Checkpoint>
    where
        F: FnOnce(u64, Checkpoint) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let checkpoint = Checkpoint::new(&self, summary)?;
        commit(self.expected_generation, checkpoint.clone()).await?;
        Ok(checkpoint)
    }
}

/// Execute the same metered summarize/validate/commit ordering for either host.
///
/// # Errors
/// Cancellation, a failed/unknown physical call or a stale guarded commit.
pub async fn execute<S, C, Stop, Observe>(
    task: Compaction,
    summarizer: &mut S,
    store: &mut C,
    stopped: Stop,
    mut observe: Observe,
) -> Result<Checkpoint>
where
    S: Summarizer + ?Sized,
    C: CheckpointStore + ?Sized,
    Stop: Fn() -> bool,
    Observe: FnMut(Progress),
{
    if stopped() {
        return Err(crate::Error::cancelled());
    }
    observe(Progress::Summarizing);
    let summary = summarizer.summarize(&task).await?;
    if stopped() {
        return Err(crate::Error::cancelled());
    }
    let checkpoint = task
        .finish_with(summary, |generation, checkpoint| {
            store.commit(generation, checkpoint)
        })
        .await?;
    observe(Progress::Committed(checkpoint.generation));
    Ok(checkpoint)
}
