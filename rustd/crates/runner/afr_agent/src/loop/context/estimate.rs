//! The selected provider estimates the same complete request that will be sent.
use super::{super::Harness, source::Source};
use af_context::{Compaction, Group, Summary, TokenEstimator};
use af_providers::{Message, Request};

pub(super) struct Estimator<'a, 's, 'run> {
    pub(super) source: &'a Source<'a>,
    pub(super) harness: &'a Harness<'s, 'run>,
}
impl Estimator<'_, '_, '_> {
    fn measure(
        &self,
        messages: &[Message],
        summary: bool,
        output: Option<u32>,
    ) -> af_context::Result<u64> {
        let shared = self.harness.shared;
        shared
            .provider
            .measure(Request {
                model: shared.model,
                instructions: if summary {
                    af_context::SUMMARY_INSTRUCTIONS
                } else {
                    &shared.instructions
                },
                messages,
                tools: if summary { &[] } else { &self.harness.specs },
                hosted: if summary { &[] } else { &self.harness.hosted },
                cache_key: shared.fleet_id,
                max_output_tokens: output,
                single_attempt: summary,
            })
            .map_err(af_context::Error::boundary)
    }
}
impl TokenEstimator for Estimator<'_, '_, '_> {
    fn continuation(
        &mut self,
        summary: Option<&Summary>,
        groups: &[Group],
    ) -> af_context::Result<u64> {
        self.measure(
            &self.source.retained(summary, groups)?,
            false,
            Some(self.harness.shared.output_tokens),
        )
    }
    fn compaction(&mut self, task: &Compaction) -> af_context::Result<u64> {
        self.measure(
            &[Message::User(task.input()?)],
            true,
            Some(task.output_tokens),
        )
    }
}
