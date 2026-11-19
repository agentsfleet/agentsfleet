//! The window `guffbolt` asks for before each model call, fitted by
//! `af_context`: preflight over the transcript's groups, a compaction when they
//! no longer fit, and preflight again over the notes it wrote.
//!
//! The message the loop answers stays pinned at its place in every window,
//! and a provider's replay from before the last compaction is dropped, because
//! the prefix it was recorded over changed. The transcript is never edited.
use super::Harness;
use af_context::{Checkpoint, Identity, Plan, Window};
use af_core::clock::{Clock as _, SystemClock};
use sha2::{Digest as _, Sha256};

mod effects;
mod estimate;
mod index;
mod source;

pub(super) struct State {
    pub(super) pinned: usize,
    pub(super) branch: Option<String>,
    /// The last notes this loop published; none before its first, because a
    /// branch this run minted holds none to load.
    checkpoint: Option<Checkpoint>,
    replay_from: usize,
    index: index::Index,
}
impl State {
    pub(super) const fn new(pinned: usize) -> Self {
        Self {
            pinned,
            branch: None,
            checkpoint: None,
            replay_from: 0,
            index: index::Index::new(pinned),
        }
    }
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
                    self.context.checkpoint = Some(checkpoint);
                    self.context.replay_from = self.transcript.messages().len();
                }
            }
        }
    }
    fn context_parameters(&mut self) -> af_context::Result<(Identity, Window)> {
        if self.context.branch.is_none() {
            self.context.branch = Some(if self.depth == 0 {
                self.shared.lease_id.to_owned()
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
        let identity = Identity {
            scope: format!(
                "run:{}:{}:{branch}",
                self.shared.fleet_id, self.shared.lease_id
            ),
            model: hex::encode(Sha256::digest(
                serde_json::to_vec(&(
                    "wire-estimator-v1",
                    self.shared.model,
                    self.shared.inference,
                    self.shared.model_context_tokens,
                ))
                .map_err(af_context::Error::boundary)?,
            )),
            pinned: hex::encode(Sha256::digest(self.shared.instructions.as_bytes())),
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
