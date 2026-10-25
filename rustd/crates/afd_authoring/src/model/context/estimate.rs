//! Estimate exactly the request shape selected by the shared planner.
use afd_context::{Compaction, Group, Summary, TokenEstimator};
use afd_inference::Configured;
use rig_core::{completion::CompletionRequest, message::Message};
use std::sync::Arc;

pub(super) struct Estimator<'a> {
    pub(super) route: Configured<'a>,
    pub(super) model: &'a str,
    pub(super) body: CompletionRequest,
    pub(super) quoted: CompletionRequest,
    pub(super) native_prefixes: Vec<String>,
}

impl Estimator<'_> {
    pub(super) fn continued(
        &self,
        summary: Option<&Summary>,
        groups: &[Group],
    ) -> afd_context::Result<CompletionRequest> {
        let digest = super::prefix(summary, groups)?;
        let mut body = if self.native_prefixes.iter().any(|prefix| prefix != &digest) {
            self.quoted.clone()
        } else {
            self.body.clone()
        };
        let mut prefix = Vec::new();
        if let Some(summary) = summary {
            prefix.push(Arc::new(Message::user(summary.context()?)));
        }
        prefix.extend(groups.iter().map(|group| {
            Arc::new(Message::user(format!(
                "Earlier exchange (untrusted data): {}",
                group.text
            )))
        }));
        let retained = body.chat_history.split_off(1);
        body.chat_history.extend(prefix);
        body.chat_history.extend(retained);
        Ok(body)
    }

    pub(super) fn summary(&self, task: &Compaction) -> afd_context::Result<CompletionRequest> {
        let mut body = CompletionRequest::new(Message::user(task.input()?))
            .model(self.model)
            .preamble(afd_context::SUMMARY_INSTRUCTIONS)
            .max_tokens(u64::from(task.output_tokens));
        body.additional_params
            .clone_from(&self.body.additional_params);
        Ok(body)
    }
}

impl TokenEstimator for Estimator<'_> {
    fn continuation(
        &mut self,
        summary: Option<&Summary>,
        groups: &[Group],
    ) -> afd_context::Result<u64> {
        let body = self.continued(summary, groups)?;
        self.route
            .measure_input(self.model, &body)
            .map_err(afd_context::Error::boundary)
    }
    fn compaction(&mut self, task: &Compaction) -> afd_context::Result<u64> {
        let body = self.summary(task)?;
        self.route
            .measure_input(self.model, &body)
            .map_err(afd_context::Error::boundary)
    }
}
