//! Preflight, compact and continue over the same private working-context engine.
use super::Harness;
use afd_context::{Checkpoint, CheckpointStore as _, Identity, Plan, Window};
use afd_core::clock::{Clock as _, SystemClock};
use afr_providers::Message;
use sha2::{Digest as _, Sha256};

mod effects;
mod estimate;
mod index;
mod source;

pub(super) struct State {
    pub(super) pinned: usize,
    pub(super) branch: Option<String>,
    checkpoint: Option<Checkpoint>,
    loaded: bool,
    replay_from: usize,
    index: index::Index,
    pub(super) working: Vec<Message>,
    pub(super) output: Option<u32>,
}
impl State {
    pub(super) fn new(pinned: usize) -> Self {
        Self {
            pinned,
            branch: None,
            checkpoint: None,
            loaded: false,
            replay_from: 0,
            index: index::Index::new(pinned),
            working: Vec::new(),
            output: None,
        }
    }
}

impl Harness<'_, '_> {
    pub(super) async fn preflight(&mut self) -> afd_context::Result<()> {
        let physical = self.shared.model_context_tokens;
        // A scripted provider can deliberately have no physical model. Production
        // connections reject that unresolved policy before a loop starts.
        if physical == 0 {
            self.context.working.clone_from(&self.messages);
            return Ok(());
        }
        let (branch, identity, window) = self.context_parameters().await?;
        self.context.index.refresh(&self.messages)?;
        let mut store = effects::Store {
            port: self.shared.context_checkpoints,
            branch: &branch,
        };
        loop {
            let source = source::Source::new(
                &self.messages,
                self.context.pinned,
                &self.context.index.groups,
                self.context.replay_from,
            );
            let mut estimator = estimate::Estimator {
                source: &source,
                harness: self,
            };
            let mut paging = source.pages();
            let plan = afd_context::prepare(
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
                    self.context.working = source.retained(summary.as_ref(), &groups)?;
                    self.context.output = Some(output_tokens);
                    return Ok(());
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
                    self.context.replay_from = self.messages.len();
                }
            }
        }
    }
    async fn context_parameters(&mut self) -> afd_context::Result<(String, Identity, Window)> {
        if self.context.branch.is_none() {
            self.context.branch = Some(if self.depth == 0 {
                self.shared.lease_id.to_owned()
            } else {
                afd_crypto::entropy::Entropy::new()
                    .uuid7(SystemClock.now())
                    .map_err(afd_context::Error::boundary)?
                    .to_string()
            });
        }
        let branch = self
            .context
            .branch
            .as_deref()
            .ok_or_else(afd_context::Error::stale)?;
        let mut store = effects::Store {
            port: self.shared.context_checkpoints,
            branch,
        };
        if !self.context.loaded {
            self.context.checkpoint = store.load().await?;
            if self.context.checkpoint.is_some() {
                self.context.replay_from = self.messages.len();
            }
            self.context.loaded = true;
        }
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
                .map_err(afd_context::Error::boundary)?,
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
        Ok((branch.to_owned(), identity, window))
    }
}
