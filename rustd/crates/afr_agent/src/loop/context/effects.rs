//! Runtime effects retain the shared ordering and the run's real usage meter.
use super::super::shared::Shared;
use afd_context::{Checkpoint, CheckpointStore, Compaction, PortFuture, Summarizer, Summary};
use afr_providers::{Message, Request};

pub(super) struct Model<'a, 'run> {
    pub(super) shared: &'a Shared<'run>,
    pub(super) stop: &'a tokio_util::sync::CancellationToken,
}

impl Summarizer for Model<'_, '_> {
    fn summarize<'a>(&'a mut self, task: &'a Compaction) -> PortFuture<'a, Summary> {
        Box::pin(async move {
            let messages = [Message::User(task.input()?)];
            let request = Request {
                model: self.shared.model,
                instructions: afd_context::SUMMARY_INSTRUCTIONS,
                messages: &messages,
                tools: &[],
                hosted: &[],
                cache_key: self.shared.fleet_id,
                max_output_tokens: Some(task.output_tokens),
                single_attempt: true,
            };
            let mut live = crate::events::Live::silent(self.shared.scrub, self.shared.started);
            let turn = tokio::select! {
                biased;
                () = self.stop.cancelled() => return Err(afd_context::Error::cancelled()),
                turn = crate::turn::take(self.shared.provider.stream(request), &mut live, self.shared.meter) => turn.map_err(afd_context::Error::boundary)?,
            };
            if turn.cut
                || !turn.finished
                || !turn.calls.is_empty()
                || turn.usage.prompt() == 0
                || turn.usage.output == 0
            {
                return Err(afd_context::Error::summary());
            }
            Summary::decode(&self.shared.scrub.clean(turn.text))
        })
    }
}

pub(super) struct Store<'a> {
    pub(super) port: &'a dyn crate::ContextCheckpoints,
    pub(super) branch: &'a str,
}
impl CheckpointStore for Store<'_> {
    fn load(&mut self) -> PortFuture<'_, Option<Checkpoint>> {
        Box::pin(self.port.load(self.branch))
    }
    fn commit(&mut self, expected_generation: u64, checkpoint: Checkpoint) -> PortFuture<'_, ()> {
        Box::pin(
            self.port
                .commit(self.branch, expected_generation, checkpoint),
        )
    }
}
