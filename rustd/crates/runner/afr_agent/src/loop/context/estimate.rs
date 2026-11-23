//! The selected provider estimates the same complete request that will be sent.
use super::{super::Harness, source::Source};
use af_context::{Compaction, Group, Summary, TokenEstimator};
use guffbolt::{Conversation as _, Message, Request, Window};

pub(super) struct Estimator<'a, 's, 'run> {
    pub(super) source: &'a Source<'a>,
    pub(super) harness: &'a Harness<'s, 'run>,
}
impl Estimator<'_, '_, '_> {
    fn measure(&self, request: Request<'_>) -> af_context::Result<u64> {
        self.harness
            .shared
            .provider
            .measure(request)
            .map_err(af_context::Error::boundary)
    }
}
impl TokenEstimator for Estimator<'_, '_, '_> {
    /// The loop's own request over the window `summary` and `groups` make,
    /// at the whole output allowance.
    fn continuation(
        &mut self,
        summary: Option<&Summary>,
        groups: &[Group],
    ) -> af_context::Result<u64> {
        let window = Window {
            messages: self.source.retained(summary, groups)?,
            max_output_tokens: Some(self.harness.shared.output_tokens),
        };
        let specs = self.harness.router.specs();
        self.measure(self.harness.request(&window, &specs))
    }
    fn compaction(&mut self, task: &Compaction) -> af_context::Result<u64> {
        let shared = self.harness.shared;
        let messages = [Message::User(task.input()?)];
        self.measure(Request {
            model: shared.model,
            instructions: af_context::SUMMARY_INSTRUCTIONS,
            messages: &messages,
            tools: &[],
            hosted: &[],
            cache_key: shared.fleet_id,
            max_output_tokens: Some(task.output_tokens),
            single_attempt: true,
        })
    }
}
