//! The window `guffbolt` asks for before each model call, fitted by
//! `af_context`: preflight over the transcript's groups, a compaction when they
//! no longer fit, and preflight again over the notes it wrote.
//!
//! The message the loop answers stays pinned at its place in every window,
//! and a provider's replay from before the last compaction is dropped, because
//! the prefix it was recorded over changed. The transcript is never edited.
//!
//! A fleet chat's root also keeps its thread's saved summary (`saved`): it
//! opens the transcript, and each compaction that covers earlier turns whole
//! saves the thread's next one.
use super::Harness;
use af_context::{Checkpoint, Identity, Plan, Window};
use af_core::clock::{Clock as _, SystemClock};
use sha2::{Digest as _, Sha256};

mod effects;
mod estimate;
mod index;
mod saved;
mod source;

pub(super) use self::saved::Saved;

/// What versions the fleet model's identity, so notes written for another
/// model, window or estimate are not read back.
const ESTIMATOR: &str = "wire-estimator-v1";

pub(super) struct State {
    pub(super) pinned: usize,
    pub(super) branch: Option<String>,
    /// The last notes this loop published; none before its first, because a
    /// branch this run minted holds none to load.
    checkpoint: Option<Checkpoint>,
    replay_from: usize,
    index: index::Index,
    /// The thread's saved summary, on a fleet chat's root that read it.
    pub(super) saved: Option<Saved>,
}
impl State {
    pub(super) const fn new(pinned: usize) -> Self {
        Self {
            pinned,
            branch: None,
            checkpoint: None,
            replay_from: 0,
            index: index::Index::new(pinned),
            saved: None,
        }
    }
}

/// The model and pinned digests a checkpoint of a run on `model`, under
/// `inference` and a window of `context_tokens`, with `instructions` as its
/// system prompt, carries: the run's own notes and the thread's saved ones
/// alike.
pub(super) fn digests(
    model: &str,
    inference: Option<&af_wire::inference::ModelInference>,
    context_tokens: u32,
    instructions: &str,
) -> af_context::Result<(String, String)> {
    let measured = serde_json::to_vec(&(ESTIMATOR, model, inference, context_tokens))
        .map_err(af_context::Error::boundary)?;
    Ok((
        hex::encode(Sha256::digest(measured)),
        hex::encode(Sha256::digest(instructions.as_bytes())),
    ))
}

impl Harness<'_, '_> {
    /// The next model call's window: what of the transcript fits, after as
    /// many compactions as fitting it takes.
    pub(super) async fn preflight(&mut self) -> af_context::Result<guffbolt::Window> {
        let physical = self.shared.model_context_tokens;
        // A scripted provider can deliberately have no physical model. Production
        // connections reject that unresolved policy before a loop starts.
        if physical == 0 {
            return Ok(guffbolt::Window::whole(self.transcript.messages()));
        }
        let (identity, window) = self.context_parameters()?;
        self.context.index.refresh(self.transcript.messages())?;
        let mut store = effects::Store {
            generation: self
                .context
                .checkpoint
                .as_ref()
                .map_or(0, |checkpoint| checkpoint.generation),
        };
        loop {
            let source = source::Source::new(
                self.transcript.messages(),
                self.context.pinned,
                &self.context.index.groups,
                self.context.replay_from,
            );
            let mut estimator = estimate::Estimator {
                source: &source,
                harness: self,
            };
            let mut paging = source.pages();
            let plan = af_context::prepare(
                &mut paging,
                &mut estimator,
                identity.clone(),
                self.context.checkpoint.clone(),
                window,
                || self.stop.is_cancelled(),
                |_| {},
            )
            .await?;
            match plan {
                Plan::Ready {
                    summary,
                    groups,
                    output_tokens,
                } => {
                    return Ok(guffbolt::Window {
                        messages: source.retained(summary.as_ref(), &groups)?,
                        max_output_tokens: Some(output_tokens),
                    });
                }
                Plan::Compact(task) => {
                    let checkpoint = effects::compact(
                        self.shared,
                        &self.stop,
                        &mut store,
                        task,
                        self.depth == 0,
                    )
                    .await?;
                    if let Some(saved) = self.context.saved.as_mut() {
                        saved
                            .save(
                                self.shared.run.verbs,
                                self.shared.fencing_token,
                                &checkpoint,
                                self.transcript.messages(),
                            )
                            .await?;
                    }
                    self.context.checkpoint = Some(checkpoint);
                    self.context.replay_from = self.transcript.messages().len();
                }
            }
        }
    }
    fn context_parameters(&mut self) -> af_context::Result<(Identity, Window)> {
        if self.context.branch.is_none() {
            self.context.branch = Some(if self.depth == 0 {
                self.shared.run_id.to_owned()
            } else {
                af_crypto::entropy::Entropy::new()
                    .uuid7(SystemClock.now())
                    .map_err(af_context::Error::boundary)?
                    .to_string()
            });
        }
        let branch = self
            .context
            .branch
            .as_deref()
            .ok_or_else(af_context::Error::stale)?;
        let (model, pinned) = digests(
            self.shared.model,
            self.shared.inference,
            self.shared.model_context_tokens,
            &self.shared.instructions,
        )?;
        let identity = Identity {
            scope: format!(
                "run:{}:{}:{branch}",
                self.shared.fleet_id, self.shared.run_id
            ),
            model,
            pinned,
        };
        let threshold = self.shared.context.stage_chunk_threshold;
        let percent = (1_u8..=90)
            .rfind(|percent| f32::from(*percent) / 100.0 <= threshold)
            .map_or(1, u32::from);
        let window = Window::new(self.shared.model_context_tokens, self.shared.output_tokens)?
            .with_operating_tokens(self.shared.context.context_cap_tokens)
            .with_trigger_percent(percent);
        Ok((identity, window))
    }
}
