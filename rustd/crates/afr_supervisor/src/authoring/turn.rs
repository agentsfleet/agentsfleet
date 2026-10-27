//! Only conversation, bounded model calls and native proposal receipts enter a turn.

use super::{Ending, meter::Observed, proposal};
use crate::{Result, client::ControlPlane, error};
use afd_core::{clock::Clock, id::Uuid7};
use afd_crypto::entropy::Entropy;
use afd_wire::authoring::{ContextRole, Lease, ModelCall, ModelMessage, ModelRequest};
use serde_json::Value;

pub(super) struct Turn<'a> {
    pub(super) plane: &'a ControlPlane,
    pub(super) lease: &'a Lease<Value>,
    pub(super) id: &'a Uuid7,
    pub(super) clock: &'a dyn Clock,
    pub(super) observed: &'a Observed,
}

impl Turn<'_> {
    pub(super) async fn run(&self) -> Result<Ending> {
        let mut messages = self.context()?;
        let mut activity_sequence = 1;
        for call_index in 1..=u32::try_from(self.lease.limits.max_calls.min(8)).unwrap_or(0) {
            let meter = self.observed.snapshot()?;
            let budget = u64::try_from(self.lease.limits.output_per_turn)
                .map_err(|_negative| error::config("the authoring output allowance is invalid"))?;
            let remaining = budget.saturating_sub(meter.output_tokens.unwrap_or(budget));
            if remaining == 0 {
                break;
            }
            let request = ModelCall {
                capability_token: self.lease.model_access.token.clone(),
                attempt_id: self.lease.attempt_id.clone(),
                fence: self.lease.fence.clone(),
                dispatch_id: Entropy::new().uuid7(self.clock.now())?.to_string(),
                call_index,
                request: ModelRequest {
                    supports_compaction: true,
                    messages: messages.clone(),
                    max_output_tokens: u32::try_from(remaining).map_err(|_overflow| {
                        error::config("the authoring output allowance exceeds the wire bound")
                    })?,
                },
            };
            let result = self.plane.authoring_model(self.id, &request).await?;
            if result.dispatch_id != request.dispatch_id || result.call_index != call_index {
                return Err(error::config(
                    "the daemon returned a different authoring dispatch",
                ));
            }
            self.observed.accept(&result.usage)?;
            if matches!(result.usage, afd_wire::authoring::ModelUsage::Unknown) {
                return Ok(Ending::unknown());
            }
            if result.compacted {
                continue;
            }
            if result.cut {
                return Ok(Ending::incomplete());
            }
            if result.proposal_calls.is_empty() {
                return Ok(Ending::completed(Some(result.text)));
            }
            let answer = result.text.clone();
            messages.push(ModelMessage::Assistant {
                text: result.text,
                proposal_calls: result.proposal_calls.clone(),
            });
            let mut all_accepted = true;
            for call in result.proposal_calls {
                let (message, accepted) =
                    proposal::apply(self.plane, self.id, self.lease, activity_sequence, &call)
                        .await?;
                messages.push(message);
                activity_sequence += u64::from(accepted);
                all_accepted &= accepted;
            }
            if all_accepted && !answer.trim().is_empty() {
                return Ok(Ending::completed(Some(answer)));
            }
        }
        Ok(Ending::incomplete())
    }

    fn context(&self) -> Result<Vec<ModelMessage>> {
        let mut messages: Vec<_> = self
            .lease
            .messages
            .iter()
            .map(|message| match message.role {
                ContextRole::User => ModelMessage::User {
                    text: message.text.clone(),
                },
                ContextRole::Assistant => ModelMessage::Assistant {
                    text: message.text.clone(),
                    proposal_calls: Vec::new(),
                },
            })
            .collect();
        if let Some(candidate) = &self.lease.candidate {
            let candidate = serde_json::to_string(candidate).map_err(error::encode)?;
            // Keep the person's current request last, after the setup data it edits.
            messages.insert(
                messages.len().saturating_sub(1),
                ModelMessage::User {
                    text: format!("Current candidate (untrusted setup data): {candidate}"),
                },
            );
        }
        Ok(messages)
    }
}
