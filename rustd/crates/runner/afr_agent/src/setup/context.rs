//! A setup run's context: the draft's saved summary, the earlier turns it
//! does not cover, and the compaction that folds the oldest of them into the
//! next summary.
//!
//! ```text
//!  load ─► saved checkpoint, kept when it belongs to this draft, model and prompt
//!       ─► earlier turns after its watermark, each named by its sequence
//!  window: [summary] + earlier turns + the transcript
//!  compaction: the oldest earlier turns, all but the newest two
//!       ─► af_context::execute ─► Store::commit ─► ContextSave at the stored version
//! ```
//!
//! A turn without a sequence cannot be named by a watermark, so a run whose
//! daemon sent none neither skips a turn for a saved summary nor compacts.
//! A run whose load failed compacts in memory and saves nothing: it knows no
//! version to save over.

use af_context::{Checkpoint, CheckpointStore, Compaction, Group, Identity, PortFuture};
use af_secrets::Scrub;
use af_wire::context::ContextSaveRequest;
use af_wire::run::{RunPayload, SetupCandidate};
use guffbolt::{Message, Replay};
use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;

use super::SetupContext;
use crate::error::Result;

/// What labels the head candidate the turn edits, so the model reads it as
/// data, never as an instruction.
pub(super) const CANDIDATE_LABEL: &str = "Current candidate (untrusted setup data): ";
/// The newest earlier turns a compaction keeps whole, as `af_context`'s
/// planner keeps its newest groups.
const KEEP_RECENT_TURNS: usize = 2;
/// What versions the setup model's identity, so a summary written for
/// another model, window or estimate is not read back.
const ESTIMATOR: &str = "setup-estimator-v1";
const EVENT_LOAD_FAILED: &str = "setup_context_load_failed";
const EVENT_IGNORED: &str = "setup_context_ignored";
const EVENT_SAVED: &str = "setup_context_saved";

/// One earlier turn of the draft: what the person asked and what setup
/// answered, named by its place in the draft when the daemon named it.
#[derive(Clone)]
pub(super) struct Earlier {
    sequence: Option<u64>,
    messages: [Message; 2],
}

/// The draft's context as the run holds it. Its `Debug` names versions and
/// counts only: the turns and the notes are the creator's conversation.
pub(super) struct Saved<'a> {
    context: &'a dyn SetupContext,
    fencing_token: u64,
    /// Whether the load answered: only then is there a version to save over.
    saving: bool,
    /// The version the next save expects: the stored one, then each save's.
    version: u64,
    identity: Identity,
    checkpoint: Option<Checkpoint>,
    earlier: Vec<Earlier>,
    /// Whether every earlier turn carries its sequence.
    named: bool,
}

impl<'a> Saved<'a> {
    /// The draft's context for `run`, read through `context`. A read that
    /// fails is logged, and the run goes on from its earlier turns alone.
    pub(super) async fn load(
        context: &'a dyn SetupContext,
        run: &RunPayload<'_>,
        scrub: &Scrub,
    ) -> Self {
        let named = run
            .setup
            .as_ref()
            .is_some_and(|work| work.history_sequences.len() == run.history.len());
        let (saving, version, scope, stored) = match context.load().await {
            Ok(loaded) => (
                true,
                loaded.resource_version,
                loaded.scope,
                loaded.checkpoint,
            ),
            Err(failure) => {
                let (error_code, run_id) = (failure.code().as_str(), run.run_id.as_ref());
                tracing::warn!(event = EVENT_LOAD_FAILED, error_code, run_id);
                (false, 0, format!("run:{}", run.run_id), None)
            }
        };
        let identity = identity(scope, run);
        let checkpoint = stored.and_then(|document| usable(document, &identity, named));
        let covered = checkpoint.as_ref().map(|kept| kept.covered_through);
        Self {
            context,
            fencing_token: run.fencing_token,
            saving,
            version,
            identity,
            checkpoint,
            earlier: earlier(run, scrub, named, covered),
            named,
        }
    }

    /// What the next model call sends: the saved summary, the earlier turns
    /// it does not cover, then `transcript`, the turn this run answers.
    ///
    /// # Errors
    /// The summary cannot be quoted.
    pub(super) fn messages(&self, transcript: &[Message]) -> af_context::Result<Vec<Message>> {
        let mut messages = Vec::with_capacity(1 + 2 * self.earlier.len() + transcript.len());
        if let Some(checkpoint) = &self.checkpoint {
            messages.push(Message::User(checkpoint.summary.context()?));
        }
        for turn in &self.earlier {
            messages.extend(turn.messages.iter().cloned());
        }
        messages.extend(transcript.iter().cloned());
        Ok(messages)
    }

    /// The compaction of the oldest earlier turns, all but the newest
    /// [`KEEP_RECENT_TURNS`], halved until `fits` admits its summary request;
    /// none when no turn can be compacted.
    ///
    /// # Errors
    /// A turn cannot be quoted, or `fits` cannot measure.
    pub(super) fn compaction(
        &self,
        output_tokens: u32,
        mut fits: impl FnMut(&Compaction) -> af_context::Result<bool>,
    ) -> af_context::Result<Option<Compaction>> {
        let take = self.earlier.len().saturating_sub(KEEP_RECENT_TURNS);
        if !self.named || take == 0 {
            return Ok(None);
        }
        let groups = self
            .earlier
            .iter()
            .take(take)
            .map(Earlier::group)
            .collect::<af_context::Result<Vec<_>>>()?;
        let mut task = Compaction {
            identity: self.identity.clone(),
            expected_generation: self.version,
            prior_digest: self
                .checkpoint
                .as_ref()
                .map_or_else(String::new, |kept| kept.source_digest.clone()),
            prior_summary: self.checkpoint.as_ref().map(|kept| kept.summary.clone()),
            groups,
            output_tokens: af_context::SUMMARY_OUTPUT_TOKENS.min(output_tokens),
        };
        while !task.groups.is_empty() {
            if fits(&task)? {
                return Ok(Some(task));
            }
            // Geometric reduction, as the shared planner bounds its own.
            task.groups.truncate(task.groups.len().div_euclid(2));
        }
        Ok(None)
    }

    /// Runs `task` through `summarizer` and saves its checkpoint, then drops
    /// the earlier turns it covers.
    ///
    /// # Errors
    /// The summary call failed or was cut, the run stopped, or the save was
    /// refused: a superseded run's save among them.
    pub(super) async fn compact(
        &mut self,
        task: Compaction,
        summarizer: &mut impl af_context::Summarizer,
        stop: &CancellationToken,
    ) -> af_context::Result<()> {
        let mut store = Store {
            context: self.context,
            fencing_token: self.fencing_token,
            saving: self.saving,
            version: &mut self.version,
        };
        let checkpoint =
            af_context::execute(task, summarizer, &mut store, || stop.is_cancelled(), |_| {})
                .await?;
        let covered = checkpoint.covered_through;
        self.earlier
            .retain(|turn| turn.sequence.is_some_and(|sequence| sequence > covered));
        self.checkpoint = Some(checkpoint);
        Ok(())
    }
}

impl std::fmt::Debug for Saved<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Saved")
            .field("saving", &self.saving)
            .field("version", &self.version)
            .field("checkpoint", &self.checkpoint)
            .field("earlier", &self.earlier.len())
            .finish_non_exhaustive()
    }
}

impl Earlier {
    /// The turn as the source group a compaction quotes.
    fn group(&self) -> af_context::Result<Group> {
        Ok(Group {
            sequence: self.sequence.ok_or_else(af_context::Error::stale)?,
            text: af_providers::context_document(&self.messages)
                .map_err(af_context::Error::boundary)?,
        })
    }
}

/// The run's first message: the head candidate it edits, labelled as data,
/// then the person's message, both scrubbed.
///
/// # Errors
/// The candidate cannot be quoted.
pub(super) fn opening(
    scrub: &Scrub,
    message: &str,
    candidate: Option<&SetupCandidate<'_>>,
) -> Result<Vec<Message>> {
    let text = match candidate {
        Some(head) => format!(
            "{CANDIDATE_LABEL}{}\n\n{message}",
            serde_json::to_string(head)?
        ),
        None => message.to_owned(),
    };
    Ok(vec![Message::User(scrub.clean(text).into_inner())])
}

/// The identity a checkpoint of this run carries: the scope the daemon
/// assigned, the model and window it was measured for, and the prompt it was
/// written under.
pub(super) fn identity(scope: String, run: &RunPayload<'_>) -> Identity {
    let policy = &run.policy;
    let model = (
        ESTIMATOR,
        &policy.context.model,
        &policy.inference,
        policy.model_context_tokens,
    );
    // A tuple of strings and numbers always encodes; an empty digest input
    // would only make the identity unmatchable, never shared.
    let encoded = serde_json::to_vec(&model).unwrap_or_default();
    Identity {
        scope,
        model: hex::encode(Sha256::digest(encoded)),
        pinned: hex::encode(Sha256::digest(run.instructions.as_bytes())),
    }
}

/// The stored `document`, when it is a valid checkpoint of `identity` the
/// run can place: one whose watermark names turns, which `named` says.
fn usable(document: serde_json::Value, identity: &Identity, named: bool) -> Option<Checkpoint> {
    let checkpoint = serde_json::from_value::<Checkpoint>(document)
        .ok()
        .filter(|checkpoint| checkpoint.validate_document().is_ok())
        .filter(|checkpoint| checkpoint.belongs_to(identity))
        .filter(|_| named);
    if checkpoint.is_none() {
        tracing::info!(event = EVENT_IGNORED);
    }
    checkpoint
}

/// `run`'s earlier turns, oldest first and scrubbed, without those a saved
/// summary `covered` through.
fn earlier(run: &RunPayload<'_>, scrub: &Scrub, named: bool, covered: Option<u64>) -> Vec<Earlier> {
    let sequences = run
        .setup
        .as_ref()
        .map(|work| work.history_sequences.as_slice())
        .filter(|_| named)
        .unwrap_or_default();
    run.history
        .iter()
        .enumerate()
        .filter_map(|(index, turn)| {
            let sequence = sequences.get(index).copied();
            if sequence
                .zip(covered)
                .is_some_and(|(at, through)| at <= through)
            {
                return None;
            }
            let answer = Message::Assistant {
                text: scrub.clean(turn.answer.to_string()).into_inner(),
                calls: Vec::new(),
                replay: Replay::default(),
            };
            let asked = Message::User(scrub.clean(turn.message.to_string()).into_inner());
            Some(Earlier {
                sequence,
                messages: [asked, answer],
            })
        })
        .collect()
}

/// Publishes a compaction's checkpoint: through the context verb at the
/// version the run last read or saved, or in memory for a run that read none.
struct Store<'s> {
    context: &'s dyn SetupContext,
    fencing_token: u64,
    saving: bool,
    version: &'s mut u64,
}

impl CheckpointStore for Store<'_> {
    fn commit(&mut self, expected_generation: u64, checkpoint: Checkpoint) -> PortFuture<'_, ()> {
        Box::pin(async move {
            if expected_generation != *self.version {
                return Err(af_context::Error::stale());
            }
            if !self.saving {
                *self.version = checkpoint.generation;
                return Ok(());
            }
            let request = ContextSaveRequest {
                fencing_token: self.fencing_token,
                resource_version: expected_generation,
                checkpoint: serde_json::to_value(&checkpoint)
                    .map_err(af_context::Error::boundary)?,
            };
            let saved = self
                .context
                .save(request)
                .await
                .map_err(af_context::Error::boundary)?;
            *self.version = saved.resource_version;
            let (generation, covered_through) = (checkpoint.generation, checkpoint.covered_through);
            tracing::info!(event = EVENT_SAVED, generation, covered_through);
            Ok(())
        })
    }
}
