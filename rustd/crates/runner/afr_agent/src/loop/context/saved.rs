//! A fleet chat's saved summary, as one run holds it: read at the run's
//! start, placed ahead of the earlier turns it does not cover, and saved
//! again whenever the run's compaction covers more of them. Setup keeps its
//! draft's the same way, through the same fenced verbs (`crate::setup`).
//!
//! ```text
//!  load ─► ContextLoad ─► the thread's checkpoint, kept when it belongs to this
//!                         fleet, model and prompt
//!       ─► the earlier turns after its watermark
//!  opening: [summary] + those turns + this turn's message
//!  compaction (af_context, in the run) ─► notes over a prefix of the transcript
//!       ─► the earlier turns that prefix holds whole ─► ContextSave at the stored version
//! ```
//!
//! The saved watermark counts turn sequences; the run's own notes count
//! transcript positions. A save binds the run's notes to the earlier turns
//! they hold whole (`Compaction::finish_with`), so the next run skips exactly
//! those turns. Notes that also covered some of this run's own work repeat it
//! to the next run beside the finished turn itself: an overlap, never a loss.
//!
//! A run whose load failed keeps its notes in memory and saves nothing: it
//! knows no version to save over. A save a superseded run makes is refused
//! and fails the run, as its report would be; any other failed save is
//! logged, and the run goes on without saving again.

use std::ops::Range;

use af_context::{Checkpoint, Compaction, Group, Identity, Summary};
use af_core::error_code;
use af_wire::context::ContextSaveRequest;
use af_wire::run::Turn;
use afr_tools::{RunVerbs, Unanswered};
use guffbolt::Message;

use crate::setup::usable;

const EVENT_LOAD_FAILED: &str = "fleet_context_load_failed";
const EVENT_SAVED: &str = "fleet_context_saved";
const EVENT_SAVE_FAILED: &str = "fleet_context_save_failed";

/// The thread's saved summary and where this run's earlier turns sit. Its
/// `Debug` names versions and counts only: the notes are the conversation.
pub(crate) struct Saved {
    /// Whether a save can still be made: the load answered, and no save failed.
    saving: bool,
    /// The version the next save expects: the stored one, then each save's.
    version: u64,
    identity: Identity,
    /// The stored checkpoint's chained digest, which the next save extends.
    digest: String,
    /// The last turn the stored checkpoint covers; zero before the first.
    covered: u64,
    /// The stored notes, when this run can place them.
    summary: Option<Summary>,
    /// Each earlier turn the opening carries: its sequence and its messages'
    /// place in the transcript.
    turns: Vec<(u64, Range<usize>)>,
}

impl std::fmt::Debug for Saved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Saved")
            .field("saving", &self.saving)
            .field("version", &self.version)
            .field("covered", &self.covered)
            .field("turns", &self.turns.len())
            .finish_non_exhaustive()
    }
}

impl Saved {
    /// The thread's saved context, read through `verbs` for the run `run_id`
    /// whose checkpoints carry `model` and `pinned`; none when the read
    /// fails, which is logged.
    pub(crate) async fn load(
        verbs: &dyn RunVerbs,
        run_id: &str,
        model: String,
        pinned: String,
    ) -> Option<Self> {
        let loaded = match verbs.context_load().await {
            Ok(loaded) => loaded,
            Err(failure) => {
                let error_code = code_of(failure);
                tracing::warn!(event = EVENT_LOAD_FAILED, error_code, run_id);
                return None;
            }
        };
        let identity = Identity {
            scope: loaded.scope,
            model,
            pinned,
        };
        let checkpoint = loaded
            .checkpoint
            .and_then(|document| usable(document, &identity, run_id));
        Some(Self {
            saving: true,
            version: loaded.resource_version,
            digest: checkpoint
                .as_ref()
                .map_or_else(String::new, |kept| kept.source_digest.clone()),
            covered: checkpoint.as_ref().map_or(0, |kept| kept.covered_through),
            summary: checkpoint.map(|kept| kept.summary),
            identity,
            turns: Vec::new(),
        })
    }

    /// The saved notes, when this run can place them.
    pub(crate) const fn summary(&self) -> Option<&Summary> {
        self.summary.as_ref()
    }

    /// Whether the saved notes cover `turn`, which the opening then leaves out.
    pub(crate) const fn covers(&self, turn: &Turn<'_>) -> bool {
        turn.sequence <= self.covered
    }

    /// Records that the opening placed `sequence`'s messages at `at`.
    pub(crate) fn placed(&mut self, sequence: u64, at: Range<usize>) {
        self.turns.push((sequence, at));
    }

    /// Saves `notes`, the run's compaction over a prefix of `messages`, as
    /// the thread's next checkpoint, when that prefix holds earlier turns
    /// whole that the stored one does not cover; `fencing_token` names the
    /// run, which the supervisor sets again.
    ///
    /// # Errors
    /// The run no longer holds the thread at its fence, or a covered turn
    /// cannot be quoted.
    pub(crate) async fn save(
        &mut self,
        verbs: &dyn RunVerbs,
        fencing_token: u64,
        notes: &Checkpoint,
        messages: &[Message],
    ) -> af_context::Result<()> {
        if !self.saving {
            return Ok(());
        }
        let through = usize::try_from(notes.covered_through).unwrap_or(usize::MAX);
        let groups = self
            .turns
            .iter()
            .filter(|(sequence, at)| *sequence > self.covered && at.end <= through)
            .map(|(sequence, at)| quote(*sequence, messages, at.clone()))
            .collect::<af_context::Result<Vec<Group>>>()?;
        if groups.is_empty() {
            return Ok(());
        }
        let task = Compaction {
            identity: self.identity.clone(),
            expected_generation: self.version,
            prior_digest: self.digest.clone(),
            prior_summary: None,
            groups,
            output_tokens: 0,
        };
        let saved = task
            .finish_with(notes.summary.clone(), async move |expected, checkpoint| {
                let request = ContextSaveRequest {
                    fencing_token,
                    resource_version: expected,
                    checkpoint: serde_json::to_value(&checkpoint)?,
                };
                verbs.context_save(request).await.map_err(refused)?;
                Ok(())
            })
            .await;
        match saved {
            Ok(checkpoint) => {
                let (generation, covered_through) =
                    (checkpoint.generation, checkpoint.covered_through);
                self.version = generation;
                self.covered = covered_through;
                self.digest = checkpoint.source_digest;
                tracing::info!(event = EVENT_SAVED, generation, covered_through);
                Ok(())
            }
            Err(superseded) if superseded.is_stale_fence() => Err(superseded),
            Err(failure) => {
                // The stored version is unknown now, so no later save can
                // name it; the run keeps its notes in memory.
                self.saving = false;
                let error_code = failure.code().as_str();
                tracing::warn!(event = EVENT_SAVE_FAILED, error_code);
                Ok(())
            }
        }
    }
}

/// The earlier turn `sequence`, at `at` in `messages`, as the source group a
/// checkpoint covers.
fn quote(sequence: u64, messages: &[Message], at: Range<usize>) -> af_context::Result<Group> {
    let turn = messages.get(at).ok_or_else(af_context::Error::stale)?;
    Ok(Group {
        sequence,
        text: af_providers::context_document(turn).map_err(af_context::Error::boundary)?,
    })
}

/// A refused or unanswered save: a superseded run's is the stale-fence
/// refusal, and every other refusal a stale version.
fn refused(failure: Unanswered) -> af_context::Error {
    match failure {
        Unanswered::Refused(Some(code)) if code == error_code::RUN_STALE_FENCING_TOKEN => {
            af_context::Error::stale_fence()
        }
        Unanswered::Refused(_) => af_context::Error::stale(),
        Unanswered::Unreachable => af_context::Error::boundary(Unreachable),
    }
}

/// The registry code a refusal carried, or the tool code an unanswered or
/// uncoded verb reads as.
fn code_of(failure: Unanswered) -> &'static str {
    match failure {
        Unanswered::Refused(Some(code)) => code.as_str(),
        Unanswered::Refused(None) | Unanswered::Unreachable => failure.tool_code().as_str(),
    }
}

/// The daemon did not answer a save through the runner's retries.
#[derive(Debug, thiserror::Error)]
#[error("agentsfleetd did not answer the context save")]
struct Unreachable;
