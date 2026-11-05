//! Root-only coordination: native identity, accounted judgments and complete plans.

pub(crate) mod interpretation;
mod routing;
mod reply;
#[cfg(test)]
mod interpretation_tests;
pub(crate) mod goal;

use afd_wire::{lease::LeasePayload, task::*};
use afr_tools::LeaseVerbs;
use crate::{Meter, Result};

/// Native inputs and accepted meaning prepared before any sandbox is built.
#[derive(Debug, Clone)]
pub struct PreparedTask {
    /// Complete native read, including current ownership and recovery cursors.
    pub inputs: TaskInputsResponse<'static>,
    /// True only when native binding permits an action on this lease's task.
    pub allow_actions: bool,
}

pub(crate) fn unavailable() -> crate::Error {
    crate::error::ErrorKind::TaskDecision.into()
}

pub(crate) fn native(reason: afr_tools::verbs::Unanswered) -> crate::Error {
    crate::error::ErrorKind::TaskNative { reason }.into()
}

pub(crate) fn identity() -> Result<String> {
    Ok(afd_crypto::entropy::Entropy::new().uuid7(afd_core::clock::now())?.to_string())
}

pub(crate) async fn prepare(
    lease: &LeasePayload<'_>, verbs: &dyn LeaseVerbs, provider: &dyn afr_providers::Provider,
    meter: &Meter, stop: &tokio_util::sync::CancellationToken,
) -> Result<PreparedTask> {
    let inputs = verbs.task_inputs(TaskInputsQuery { fencing_token: lease.fencing_token,
        starting_after: None, after_input_sequence: None, limit: 4 }).await.map_err(crate::decision::native)?;
    let mut prepared = PreparedTask { allow_actions: inputs.current_message.is_none()
        && inputs.current_task.is_some(), inputs };
    if let Some(accepted) = &prepared.inputs.current_interpretation {
        prepared.allow_actions = accepted.items.iter().any(|item| actionable(&item.binding));
        return Ok(prepared);
    }
    let Some(message) = prepared.inputs.current_message.as_deref() else { return Ok(prepared); };
    let accepted = Intake { lease, verbs, provider, meter, stop }
        .bind(&prepared.inputs, &prepared.inputs.source_admission_id, message).await;
    if let Err(failure) = accepted {
        if stop.is_cancelled() { return Err(failure); }
        tracing::warn!(event = "task_intent_unavailable", error_code = %failure.code());
        // A lost binding acknowledgement may already have committed. The
        // native read below recovers that meaning before choosing clarification.
        // No replay or unavailable classification grants action tools.
    }
    // Read the immutable interpretation back, including native item identities.
    prepared.inputs = verbs.task_inputs(TaskInputsQuery { fencing_token: lease.fencing_token,
        starting_after: None, after_input_sequence: None, limit: 4 }).await.map_err(crate::decision::native)?;
    prepared.allow_actions = prepared.inputs.current_interpretation.as_ref()
        .is_some_and(|accepted| accepted.items.iter().any(|item| actionable(&item.binding)));
    Ok(prepared)
}

/// One root's existing coding/decision seams; no additional worker or queue.
pub(crate) struct Intake<'a> {
    pub(crate) lease: &'a LeasePayload<'a>,
    pub(crate) verbs: &'a dyn LeaseVerbs,
    pub(crate) provider: &'a dyn afr_providers::Provider,
    pub(crate) meter: &'a Meter,
    pub(crate) stop: &'a tokio_util::sync::CancellationToken,
}

impl Intake<'_> {
    pub(crate) async fn bind(&self, inputs: &TaskInputsResponse<'_>, source: &str, message: &str)
        -> Result<TaskBindingsResponse<'static>> {
        let Self { lease, verbs, provider, meter, stop } = *self;
    let assignment = lease.decision.as_ref().ok_or_else(unavailable)?;
    let evaluated = verbs.task_decision(DecisionReservationInput {
        operation_id: identity()?, fencing_token: lease.fencing_token,
        source_admission_id: source.to_owned(), assessment: DecisionAssessment::Message,
        task: version(inputs.current_task.as_ref()), retry_of: None,
        evidence: serde_json::json!({}),
    }).await.map_err(crate::decision::native)?;
    let answers = routing::answers(&evaluated)?;
    let count = routing::multiplicity(answers, &assignment.intent)?;
    let proposed = interpretation::propose(lease, provider, meter, stop, message, &inputs.tasks, count).await?;
    let mut binding = TaskBindingsInput { fencing_token: lease.fencing_token,
        source_admission_id: source.to_owned(),
        accepted_revision_id: inputs.accepted_revision_id.clone().ok_or_else(unavailable)?,
        plan_digest: String::new(), message_decision_call_id: evaluated.call_id.clone(),
        items: proposed.into_iter().map(|proposed| TaskBindingItem { proposed, decision_call_id: None }).collect() };
    binding.plan_digest = afd_core::etag::sha256_hex(&serde_json::to_vec(&binding.interpretation())?);
    for (ordinal, item) in binding.items.iter_mut().enumerate() {
        if count == routing::Multiplicity::Single {
            routing::check(&evaluated, &item.proposed, &assignment.intent)?;
        } else {
            let item_call = verbs.task_decision(DecisionReservationInput {
                operation_id: identity()?, fencing_token: lease.fencing_token,
                source_admission_id: binding.source_admission_id.clone(),
                assessment: DecisionAssessment::Item { ordinal: u8::try_from(ordinal).map_err(|_| unavailable())?,
                    interpretation_digest: binding.plan_digest.clone(), source_start: item.proposed.source_start,
                    source_end: item.proposed.source_end }, task: version(inputs.current_task.as_ref()),
                retry_of: None, evidence: serde_json::to_value(&item.proposed)?,
            }).await.map_err(crate::decision::native)?;
            routing::check(&item_call, &item.proposed, &assignment.intent)?;
            item.decision_call_id = Some(item_call.call_id);
        }
    }
        verbs.task_bindings(binding).await.map_err(crate::decision::native)
    }
}

fn actionable(item: &TaskItemBinding) -> bool {
    item.applies_here && item.task_id.is_some()
        && matches!(item.action, TaskInputAction::NewWork | TaskInputAction::Steer | TaskInputAction::Refresh)
}

pub(crate) fn version(task: Option<&TaskContext<'_>>) -> Option<DecisionTaskVersion> {
    task.map(|task| DecisionTaskVersion { task_id: task.task_id.to_string(), generation: task.generation })
}
