//! The `agentsfleetd` verbs one lease's tools reach, fenced by that lease.
//!
//! The supervisor holds the lease's id and fencing token; a tool holds
//! neither. Every call is one attempt: a schedule create is not idempotent and
//! a line that landed and lost its answer would post twice, so the model, not
//! a retry loop, decides whether to try again.

use af_core::id::Uuid7;
use af_wire::message_verb::MessagePosted;
use afr_agent::{ScheduleCall, Unanswered};

use crate::client::ControlPlane;

mod decision;

/// The verbs of one held lease, over the control plane, each call carrying
/// the lease's fence: [`afr_agent::LeaseVerbs`] as the supervisor serves it.
#[derive(Debug)]
pub(crate) struct FencedVerbs<'a> {
    plane: &'a ControlPlane,
    lease_id: &'a Uuid7,
    fencing_token: u64,
    spool: &'a crate::report_spool::ReportSpool,
    held: &'a tokio::sync::Notify,
    stop: &'a tokio_util::sync::CancellationToken,
}

impl<'a> FencedVerbs<'a> {
    /// The verbs of `lease_id`, fenced by `fencing_token`, through `plane`.
    pub(crate) const fn new(
        plane: &'a ControlPlane,
        lease_id: &'a Uuid7,
        fencing_token: u64,
        spool: &'a crate::report_spool::ReportSpool,
        held: &'a tokio::sync::Notify,
        stop: &'a tokio_util::sync::CancellationToken,
    ) -> Self {
        Self {
            plane,
            lease_id,
            fencing_token,
            spool,
            held,
            stop,
        }
    }
}

#[async_trait::async_trait]
impl afr_agent::LeaseVerbs for FencedVerbs<'_> {
    async fn task_decision(
        &self,
        input: af_wire::task::DecisionReservationInput,
    ) -> Result<afr_tools::verbs::DecisionEvaluation, Unanswered> {
        self.evaluate(input)
            .await
            .map_err(|failure| unanswered(&failure))
    }

    async fn schedules(&self, call: ScheduleCall<'_>) -> Result<String, Unanswered> {
        self.plane
            .schedules(self.lease_id, self.fencing_token, call)
            .await
            .map(|body| body.text())
            .map_err(|failure| unanswered(&failure))
    }

    async fn message(&self, text: &str) -> Result<bool, Unanswered> {
        let body = self
            .plane
            .message(self.lease_id, self.fencing_token, text)
            .await
            .map_err(|failure| unanswered(&failure))?;
        body.decode::<MessagePosted>()
            .map(|posted| posted.delivered)
            .map_err(|failure| unanswered(&failure))
    }

    async fn task_inputs(
        &self,
        mut query: af_wire::task::TaskInputsQuery,
    ) -> Result<af_wire::task::TaskInputsResponse<'static>, Unanswered> {
        query.fencing_token = self.fencing_token;
        self.plane
            .task_inputs(self.lease_id, &query)
            .await
            .map_err(|failure| unanswered(&failure))
    }

    async fn task_bindings(
        &self,
        mut input: af_wire::task::TaskBindingsInput,
    ) -> Result<af_wire::task::TaskBindingsResponse<'static>, Unanswered> {
        input.fencing_token = self.fencing_token;
        self.plane
            .task_bindings(self.lease_id, &input)
            .await
            .map_err(|failure| unanswered(&failure))
    }

    async fn task_input_receipt(
        &self,
        mut input: af_wire::task::TaskInputReceiptInput,
    ) -> Result<af_wire::task::TaskInputReceiptResult, Unanswered> {
        input.fencing_token = self.fencing_token;
        self.plane
            .task_input_receipt(self.lease_id, &input)
            .await
            .map_err(|failure| unanswered(&failure))
    }

    async fn task_publish(
        &self,
        mut input: af_wire::task::TaskEffectInput,
    ) -> Result<af_wire::task::TaskEffect, Unanswered> {
        input.fencing_token = self.fencing_token;
        self.plane
            .task_publish(self.lease_id, &input)
            .await
            .map_err(|failure| unanswered(&failure))
    }

    async fn task_effect(&self, effect_id: &str) -> Result<af_wire::task::TaskEffect, Unanswered> {
        let id = Uuid7::parse(effect_id).map_err(|source| {
            let failure = crate::error::Error::from(source);
            tracing::debug!(event = "task_effect_identity_refused", error_code = %failure.code());
            Unanswered::Refused(Some(failure.code()))
        })?;
        self.plane
            .task_effect(self.lease_id, &id, self.fencing_token)
            .await
            .map_err(|failure| unanswered(&failure))
    }

    async fn task_effects(
        &self,
        mut query: af_wire::task::TaskEffectsQuery,
    ) -> Result<af_wire::task::TaskEffectsPage, Unanswered> {
        query.fencing_token = self.fencing_token;
        self.plane
            .task_effects(self.lease_id, &query)
            .await
            .map_err(|failure| unanswered(&failure))
    }
}

/// A failed call as a tool reads it: a refusal keeps its registry code, and
/// anything else is `agentsfleetd` out of reach.
const fn unanswered(failure: &crate::Error) -> Unanswered {
    match failure.refusal_status() {
        Some(_status) => Unanswered::Refused(failure.refusal_code()),
        None => Unanswered::Unreachable,
    }
}

#[cfg(test)]
mod tests;

#[async_trait::async_trait]
impl afr_agent::Inbox for FencedVerbs<'_> {
    async fn claim(&self, after_sequence: i64) -> Vec<afr_agent::Folded> {
        // The loop drops this future when the lease stops, so a stopped run's
        // last claim leaves this record alone.
        let event = EVENT_FOLD_CLAIM_STARTED;
        tracing::debug!(after_sequence, event);
        let claimed: crate::Result<Vec<afr_agent::Folded>> = self
            .plane
            .folds(self.lease_id, self.fencing_token, after_sequence)
            .await
            .and_then(|body| {
                body.decode::<af_wire::fold::FoldResponse<'_>>()
                    .map(|reply| {
                        reply
                            .inputs
                            .into_iter()
                            .map(|input| afr_agent::Folded {
                                sequence: input.sequence,
                                text: input.text.into_owned(),
                            })
                            .collect()
                    })
            });
        match claimed {
            Ok(folded) => {
                let (count, event) = (folded.len(), EVENT_FOLD_CLAIM_COMPLETED);
                tracing::debug!(after_sequence, claimed = count, event);
                folded
            }
            Err(failure) => {
                // The next claim asks from the same place, so nothing is lost.
                let error_code = failure.code().as_str();
                tracing::warn!(event = EVENT_FOLD_CLAIM_FAILED, error_code, after_sequence);
                Vec::new()
            }
        }
    }
}

/// A claim of the messages the fleet received while the run works was sent.
const EVENT_FOLD_CLAIM_STARTED: &str = "fold_claim_started";
/// A claim was answered, with however many messages.
const EVENT_FOLD_CLAIM_COMPLETED: &str = "fold_claim_completed";
/// A claim went unanswered.
const EVENT_FOLD_CLAIM_FAILED: &str = "fold_claim_failed";
