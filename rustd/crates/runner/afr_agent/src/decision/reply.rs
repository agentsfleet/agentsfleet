//! Accepted conversation/status items answer their own source without action tools.

use super::{Intake, interpretation, unavailable};
use crate::Result;
use af_providers::{Message, Request};
use af_wire::task::{TaskAcceptedInterpretation, TaskInputsResponse, TaskRecoveredItem};

impl Intake<'_> {
    pub(crate) async fn reply(
        &self,
        accepted: &TaskAcceptedInterpretation,
        item: &TaskRecoveredItem,
        inputs: &TaskInputsResponse<'_>,
    ) -> Result<String> {
        let scrub = af_secrets::Scrub::new(&self.lease.policy)?;
        let context = serde_json::json!({ "original_message": accepted.message,
            "accepted_item": item, "current_task": inputs.current_task, "current_work": inputs.tasks });
        let instructions = format!(
            "{}\n\nAnswer only this already accepted conversation, status or clarification item. Do not investigate or execute work. Report native task facts accurately; missing evidence remains unknown. Other items in the complete source retain their own obligations. Return a concise plain-text answer for the original person.",
            self.lease.instructions
        );
        let instructions = scrub.clean(instructions).into_inner();
        let messages = [Message::User(scrub.clean(context.to_string()).into_inner())];
        let request = Request {
            model: &self.lease.policy.context.model,
            instructions: &instructions,
            messages: &messages,
            tools: &[],
            hosted: &[],
            cache_key: &self.lease.event.fleet_id,
            max_output_tokens: self
                .lease
                .policy
                .inference
                .as_ref()
                .map(|inference| inference.max_output_tokens.min(4096)),
            single_attempt: true,
        };
        let limit = u64::from(self.lease.policy.model_context_tokens)
            .saturating_sub(u64::from(request.max_output_tokens.unwrap_or(4096)));
        if self.provider.measure(request)? > limit {
            return Err(unavailable());
        }
        let text = tokio::select! {
            biased;
            () = self.stop.cancelled() => return Err(unavailable()),
            result = tokio::time::timeout(std::time::Duration::from_secs(30), interpretation::collect(self.provider, request, self.meter)) =>
                result.map_err(|source| crate::decision::boundary("task decision input or deadline", source))??,
        };
        let text = scrub.clean(text).into_inner();
        if text.trim().is_empty() {
            return Err(unavailable());
        }
        Ok(text)
    }
}
