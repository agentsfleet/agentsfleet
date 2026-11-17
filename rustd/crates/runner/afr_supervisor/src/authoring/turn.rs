//! Only conversation, bounded model calls and native proposal receipts enter a turn.

use super::{Ending, meter::Observed, proposal};
use crate::{Result, client::ControlPlane, error};
use af_core::{clock::Clock, id::Uuid7};
use af_crypto::entropy::Entropy;
use af_wire::authoring::{
    ContextRole, Lease, ModelCall, ModelMessage, ModelRequest, ModelResult, ProposalCall,
    ProposalRejection,
};
use serde_json::Value;

/// The corrected proposals one message may send after a refusal (owner, Nov 16, 2026).
///
/// A refusal that says the message asks for no change is not one: the right
/// reply is a sentence, which a typed Save and every question need, so those
/// refusals spend only the call allowance.
const CORRECTIONS_MAX: u32 = 1;

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
        let mut accepted_any = false;
        let mut corrections = 0;
        let max_calls = u32::try_from(self.lease.limits.max_calls.min(8))
            .map_err(|source| error::boundary("the authoring call allowance is invalid", source))?;
        for call_index in 1..=max_calls {
            let meter = self.observed.snapshot()?;
            let budget = u64::try_from(self.lease.limits.output_per_turn).map_err(|source| {
                error::boundary("the authoring output allowance is invalid", source)
            })?;
            let remaining = budget.saturating_sub(meter.output_tokens.unwrap_or(budget));
            if remaining == 0 {
                ended(call_index - 1, "output_budget");
                return Ok(settled(accepted_any));
            }
            let request = ModelCall {
                capability_token: self.lease.model_access.token.clone(),
                attempt_id: self.lease.attempt_id.clone(),
                fence: self.lease.fence.clone(),
                dispatch_id: Entropy::new().uuid7(self.clock.now())?.to_string(),
                call_index,
                request: ModelRequest {
                    messages: messages.clone(),
                    max_output_tokens: u32::try_from(remaining).map_err(|source| {
                        error::boundary(
                            "the authoring output allowance exceeds the wire bound",
                            source,
                        )
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
            record_call(call_index, &result);
            if matches!(result.usage, af_wire::authoring::ModelUsage::Unknown) {
                return Ok(Ending::unknown());
            }
            if result.compacted {
                continue;
            }
            if result.cut {
                ended(call_index, "cut");
                return Ok(settled(accepted_any));
            }
            if result.proposal_calls.is_empty() {
                ended(call_index, "answered");
                return Ok(Ending::completed(Some(result.text)));
            }
            let answer = result.text.clone();
            messages.push(ModelMessage::Assistant {
                text: result.text,
                proposal_calls: result.proposal_calls.clone(),
            });
            // The one call after an accepted setup asks for its sentence and ends the turn.
            let closing = accepted_any;
            let asked_for_an_answer = result.proposal_calls.iter().all(|call| {
                matches!(
                    call,
                    ProposalCall::Refused {
                        reason: ProposalRejection::EditNotRequested,
                        ..
                    }
                )
            });
            let mut all_accepted = true;
            for call in result.proposal_calls {
                let (message, accepted) =
                    proposal::apply(self.plane, self.id, self.lease, activity_sequence, &call)
                        .await?;
                messages.push(message);
                activity_sequence += u64::from(accepted);
                all_accepted &= accepted;
                accepted_any |= accepted;
            }
            if all_accepted && !answer.trim().is_empty() {
                ended(call_index, "proposed_and_answered");
                return Ok(Ending::completed(Some(answer)));
            }
            if closing {
                // Its text may describe a change that was just refused, so none is kept.
                ended(call_index, "proposed");
                return Ok(Ending::completed(None));
            }
            if !accepted_any && !asked_for_an_answer {
                if corrections == CORRECTIONS_MAX {
                    ended(call_index, "corrections_exhausted");
                    return Ok(Ending::incomplete());
                }
                corrections += 1;
            }
        }
        ended(max_calls, "calls_exhausted");
        Ok(settled(accepted_any))
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

/// One value-free line per model call says why a turn did or did not end.
fn record_call(call_index: u32, result: &ModelResult) {
    let text_empty = result.text.trim().is_empty();
    let proposals = result.proposal_calls.len();
    let cut = result.cut;
    let compacted = result.compacted;
    tracing::info!(
        event = "setup_model_call",
        call_index,
        text_empty,
        proposals,
        cut,
        compacted
    );
}

/// A message whose setup was accepted is done, whatever followed; the runner
/// writes no answer of its own. Without one, the turn failed.
fn settled(accepted: bool) -> Ending {
    if accepted {
        Ending::completed(None)
    } else {
        Ending::incomplete()
    }
}

/// How a turn ended, after how many model calls.
fn ended(calls: u32, ending: &'static str) {
    tracing::info!(event = "setup_turn_ended", calls, ending);
}
