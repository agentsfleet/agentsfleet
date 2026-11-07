//! The `agentsfleetd` verbs one lease's tools reach, fenced by that lease.
//!
//! The supervisor holds the lease's id and fencing token; a tool holds
//! neither. Every call is one attempt: a schedule create is not idempotent and
//! a line that landed and lost its answer would post twice, so the model, not
//! a retry loop, decides whether to try again.

use afd_core::id::Uuid7;
use afd_wire::message_verb::MessagePosted;
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
        input: afd_wire::task::DecisionReservationInput,
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
        mut query: afd_wire::task::TaskInputsQuery,
    ) -> Result<afd_wire::task::TaskInputsResponse<'static>, Unanswered> {
        query.fencing_token = self.fencing_token;
        self.plane
            .task_inputs(self.lease_id, &query)
            .await
            .map_err(|failure| unanswered(&failure))
    }

    async fn task_bindings(
        &self,
        mut input: afd_wire::task::TaskBindingsInput,
    ) -> Result<afd_wire::task::TaskBindingsResponse<'static>, Unanswered> {
        input.fencing_token = self.fencing_token;
        self.plane
            .task_bindings(self.lease_id, &input)
            .await
            .map_err(|failure| unanswered(&failure))
    }

    async fn task_input_receipt(
        &self,
        mut input: afd_wire::task::TaskInputReceiptInput,
    ) -> Result<afd_wire::task::TaskInputReceiptResult, Unanswered> {
        input.fencing_token = self.fencing_token;
        self.plane
            .task_input_receipt(self.lease_id, &input)
            .await
            .map_err(|failure| unanswered(&failure))
    }

    async fn task_publish(
        &self,
        mut input: afd_wire::task::TaskEffectInput,
    ) -> Result<afd_wire::task::TaskEffect, Unanswered> {
        input.fencing_token = self.fencing_token;
        self.plane
            .task_publish(self.lease_id, &input)
            .await
            .map_err(|failure| unanswered(&failure))
    }

    async fn task_effect(&self, effect_id: &str) -> Result<afd_wire::task::TaskEffect, Unanswered> {
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
        mut query: afd_wire::task::TaskEffectsQuery,
    ) -> Result<afd_wire::task::TaskEffectsPage, Unanswered> {
        query.fencing_token = self.fencing_token;
        self.plane
            .task_effects(self.lease_id, &query)
            .await
            .map_err(|failure| unanswered(&failure))
    }
}

/// A failed call as a tool reads it: a refusal keeps its registry code, and
/// anything else is `agentsfleetd` out of reach.
fn unanswered(failure: &crate::Error) -> Unanswered {
    match failure.refusal_status() {
        Some(_status) => Unanswered::Refused(failure.refusal_code()),
        None => Unanswered::Unreachable,
    }
}

#[cfg(test)]
mod tests;
