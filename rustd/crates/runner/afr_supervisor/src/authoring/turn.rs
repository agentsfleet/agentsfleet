//! Only conversation, bounded model calls and native proposal receipts enter a turn.

use super::{Ending, meter::Observed, proposal};
use crate::{Result, client::ControlPlane, error};
use af_core::{clock::Clock, id::Uuid7};
use af_crypto::entropy::Entropy;
use af_wire::authoring::{
    ContextRole, ModelCall, ModelMessage, ModelRequest, ModelResult, ProposalCall,
    ProposalRejection, Run,
};
use serde_json::Value;
use std::ops::ControlFlow;

/// One accepted private attempt and the authority it may call.
pub(super) struct Turn<'a> {
    pub(super) plane: &'a ControlPlane,
    pub(super) run: &'a Run<Value>,
    pub(super) id: &'a Uuid7,
    pub(super) clock: &'a dyn Clock,
    pub(super) observed: &'a Observed,
}

struct Progress {
    messages: Vec<ModelMessage>,
    activity_sequence: u64,
    accepted_any: bool,
    corrections: u32,
}

impl Turn<'_> {
    pub(super) async fn run(&self) -> Result<Ending> {
        let mut progress = Progress {
            messages: self.context()?,
            activity_sequence: 1,
            accepted_any: false,
            corrections: 0,
        };
        let max_calls = u32::try_from(
            self.run
                .limits
                .max_calls
                .min(i64::from(af_wire::authoring::MODEL_CALLS_MAX)),
        )
        .map_err(|source| error::boundary("the authoring call allowance is invalid", source))?;
        for call_index in 1..=max_calls {
            let Some(request) = self.request(&progress, call_index)? else {
                return Ok(settled(progress.accepted_any));
            };
            let result = self.invoke(&request).await?;
            if let ControlFlow::Break(ending) = self.fold(&mut progress, call_index, result).await?
            {
                return Ok(ending);
            }
        }
        ended(max_calls, "calls_exhausted");
        Ok(settled(progress.accepted_any))
    }

    fn request(&self, progress: &Progress, call_index: u32) -> Result<Option<ModelCall>> {
        let meter = self.observed.snapshot()?;
        let budget = u64::try_from(self.run.limits.output_per_turn).map_err(|source| {
            error::boundary("the authoring output allowance is invalid", source)
        })?;
        let remaining = budget.saturating_sub(meter.output_tokens.unwrap_or(budget));
        if remaining == 0 {
            ended(call_index - 1, "output_budget");
            return Ok(None);
        }
        Ok(Some(ModelCall {
            capability_token: self.run.model_access.token.clone(),
            attempt_id: self.run.attempt_id.clone(),
            fence: self.run.fence.clone(),
            dispatch_id: Entropy::new().uuid7(self.clock.now())?.to_string(),
            call_index,
            request: ModelRequest {
                messages: progress.messages.clone(),
                max_output_tokens: u32::try_from(remaining).map_err(|source| {
                    error::boundary(
                        "the authoring output allowance exceeds the wire bound",
                        source,
                    )
                })?,
            },
        }))
    }

    async fn invoke(&self, request: &ModelCall) -> Result<ModelResult> {
        let result = self.plane.authoring_model(self.id, request).await?;
        if result.dispatch_id != request.dispatch_id || result.call_index != request.call_index {
            return Err(error::config(
                "the daemon returned a different authoring dispatch",
            ));
        }
        self.observed.accept(&result.usage)?;
        record_call(request.call_index, &result);
        Ok(result)
    }

    async fn fold(
        &self,
        progress: &mut Progress,
        call_index: u32,
        result: ModelResult,
    ) -> Result<ControlFlow<Ending>> {
        if matches!(result.usage, af_wire::authoring::ModelUsage::Unknown) {
            return Ok(ControlFlow::Break(Ending::unknown()));
        }
        if result.compacted {
            return Ok(ControlFlow::Continue(()));
        }
        if result.cut {
            ended(call_index, "cut");
            return Ok(ControlFlow::Break(settled(progress.accepted_any)));
        }
        if result.proposal_calls.is_empty() {
            ended(call_index, "answered");
            return Ok(ControlFlow::Break(Ending::completed(Some(result.text))));
        }
        self.proposals(progress, call_index, result).await
    }

    /// Refused changes spend corrections; requests for an answer spend only calls.
    async fn proposals(
        &self,
        progress: &mut Progress,
        call_index: u32,
        result: ModelResult,
    ) -> Result<ControlFlow<Ending>> {
        let answer = result.text.clone();
        progress.messages.push(ModelMessage::Assistant {
            text: result.text,
            proposal_calls: result.proposal_calls.clone(),
        });
        let closing = progress.accepted_any;
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
            let (message, accepted) = proposal::apply(
                self.plane,
                self.id,
                self.run,
                progress.activity_sequence,
                &call,
            )
            .await?;
            progress.messages.push(message);
            progress.activity_sequence += u64::from(accepted);
            all_accepted &= accepted;
            progress.accepted_any |= accepted;
        }
        if all_accepted && !answer.trim().is_empty() {
            ended(call_index, "proposed_and_answered");
            return Ok(ControlFlow::Break(Ending::completed(Some(answer))));
        }
        if closing {
            // Its text may describe a refused change, so none is retained.
            ended(call_index, "proposed");
            return Ok(ControlFlow::Break(Ending::completed(None)));
        }
        if !progress.accepted_any && !asked_for_an_answer {
            if progress.corrections
                >= self
                    .run
                    .limits
                    .max_proposal_corrections
                    .min(af_wire::authoring::PROPOSAL_CORRECTIONS_MAX)
            {
                ended(call_index, "corrections_exhausted");
                return Ok(ControlFlow::Break(Ending::incomplete()));
            }
            progress.corrections += 1;
        }
        Ok(ControlFlow::Continue(()))
    }

    fn context(&self) -> Result<Vec<ModelMessage>> {
        let mut messages: Vec<_> = self
            .run
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
        if let Some(candidate) = &self.run.candidate {
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
