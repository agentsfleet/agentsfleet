//! Runtime effects retain the shared ordering and the run's real usage meter.
use super::super::shared::Shared;
use af_context::{Checkpoint, CheckpointStore, Compaction, PortFuture, Summarizer, Summary};
use guffbolt::{Message, Request, Step};
use tracing::Instrument as _;

pub(super) async fn compact(
    shared: &Shared<'_>,
    stop: &tokio_util::sync::CancellationToken,
    store: &mut Store,
    task: Compaction,
    visible: bool,
) -> af_context::Result<Checkpoint> {
    let mut trace = af_context::Trace::start(shared.run_id.into(), &task);
    let mut progress = crate::events::compaction::Notifications::new(shared.events, &task, visible);
    let result = af_context::execute(
        task,
        &mut Model { shared, stop },
        store,
        || stop.is_cancelled(),
        |value| progress.observe(value),
    )
    .instrument(trace.span().clone())
    .await;
    match &result {
        Ok(checkpoint) => trace.completed(checkpoint.generation),
        Err(error) => trace.failed(error.code()),
    }
    result
}

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
                instructions: af_context::SUMMARY_INSTRUCTIONS,
                messages: &messages,
                tools: &[],
                cache_key: self.shared.fleet_id,
                max_output_tokens: Some(task.output_tokens),
                single_attempt: true,
            };
            // A summary is private: nothing of it streams live, and only its
            // spend is heard, by the run's meter.
            let turn = tokio::select! {
                biased;
                () = self.stop.cancelled() => return Err(af_context::Error::cancelled()),
                step = Step::read(self.shared.provider.stream(request), self.shared.meter) => step.map_err(af_context::Error::boundary)?,
            };
            if turn.cut
                || !turn.finished
                || !turn.calls.is_empty()
                || turn.usage.prompt() == 0
                || turn.usage.output == 0
            {
                return Err(af_context::Error::summary());
            }
            Summary::decode(&self.shared.scrub.clean(turn.text))
        })
    }
}

/// The run's own checkpoint generation. A compaction's summary lives in the
/// loop's state, where the next model call reads it; nothing outside the run
/// reads it, so nothing is published.
pub(super) struct Store {
    pub(super) generation: u64,
}
impl CheckpointStore for Store {
    fn commit(&mut self, expected_generation: u64, checkpoint: Checkpoint) -> PortFuture<'_, ()> {
        let current = self.generation == expected_generation;
        if current {
            self.generation = checkpoint.generation;
        }
        Box::pin(std::future::ready(if current {
            Ok(())
        } else {
            Err(af_context::Error::stale())
        }))
    }
}

#[cfg(test)]
mod tests {
    use af_context::{Checkpoint, CheckpointStore as _};
    use serde_json::json;

    use super::Store;

    /// A run's checkpoint at `generation`.
    fn checkpoint(generation: u64) -> serde_json::Result<Checkpoint> {
        serde_json::from_value(json!({
            "format_version": 1,
            "identity": {"scope": "run:019f0000-0000-7000-8000-0000000000a1",
                "model": "m", "pinned": "p"},
            "generation": generation,
            "covered_through": 2,
            "source_digest": "a".repeat(64),
            "summary": {"goal": "a daily support check", "constraints": [], "decisions": [],
                "completed": [], "remaining": [], "references": []},
        }))
    }

    #[tokio::test]
    async fn a_commit_at_the_runs_generation_advances_it() -> serde_json::Result<()> {
        let mut store = Store { generation: 4 };

        let committed = store.commit(4, checkpoint(5)?).await;

        assert_eq!(committed.ok(), Some(()));
        assert_eq!(store.generation, 5);
        Ok(())
    }

    #[tokio::test]
    async fn a_commit_over_another_generation_is_stale_and_moves_nothing() -> serde_json::Result<()>
    {
        let mut store = Store { generation: 4 };

        let committed = store.commit(3, checkpoint(5)?).await;

        assert!(committed.is_err_and(|failure| failure.is_stale()));
        assert_eq!(store.generation, 4);
        Ok(())
    }
}
