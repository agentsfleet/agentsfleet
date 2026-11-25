//! Root-only coordination of a task-bound run: its native inputs read before
//! the sandbox, and the accounted goal judgments its action groups end with.
//!
//! A run is a task run only when its payload carries a bound task. Nothing
//! here interprets a person's message: the fleet's own model binds a message
//! to a task with `task_start` or `task_input`, and the decision model only
//! judges whether a task is done (`goal`).

pub(crate) mod goal;

use crate::Result;
use af_providers::Provider;
use af_wire::durable_task::{
    DecisionDurableTaskVersion, DurableTaskContext, DurableTaskInputsQuery,
    DurableTaskInputsResponse,
};
use afr_tools::RunVerbs;
use guffbolt::{Request, Step};

/// A task run's native inputs, read before any sandbox is built.
#[derive(Debug, Clone)]
pub struct PreparedDurableTask {
    /// Immutable native run execution ceiling, narrowed by the task's binding.
    pub execution_deadline: Option<jiff::Timestamp>,
    /// Complete native read, including the bound task and recovery cursors.
    pub inputs: DurableTaskInputsResponse<'static>,
}

pub(crate) fn unavailable() -> crate::Error {
    crate::error::ErrorKind::DurableTaskDecision.into()
}

pub(crate) fn boundary(
    context: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> crate::Error {
    crate::error::ErrorKind::DurableTaskBoundary {
        context,
        source: Box::new(source),
    }
    .into()
}

pub(crate) fn native(reason: afr_tools::verbs::Unanswered) -> crate::Error {
    crate::error::ErrorKind::DurableTaskNative { reason }.into()
}

/// Whether `agentsfleetd` refused a decision only because the request does
/// not fit what the decision model reads whole. It answers that refusal under
/// the shared byte-bound code before reserving, funding or sending any call,
/// so a shorter description of the same evidence may still be assessed.
pub(crate) fn exceeds_input(failure: &crate::Error) -> bool {
    matches!(failure.kind(), crate::error::ErrorKind::DurableTaskNative {
        reason: afr_tools::verbs::Unanswered::Refused(Some(code)),
    } if *code == af_core::error_code::PAYLOAD_TOO_LARGE)
}

pub(crate) fn identity() -> Result<String> {
    Ok(af_crypto::entropy::Entropy::new()
        .uuid7(af_core::clock::now())?
        .to_string())
}

/// The task-bound run's inputs, read under its fence.
///
/// # Errors
/// The read was refused or unanswered, or it carries no current task: the
/// daemon issued the run bound, so a read that names none is not one the run
/// may act on.
pub(crate) async fn prepare(
    fencing_token: u64,
    verbs: &dyn RunVerbs,
) -> Result<PreparedDurableTask> {
    let inputs = verbs
        .durable_task_inputs(DurableTaskInputsQuery {
            fencing_token,
            starting_after: None,
            after_input_sequence: None,
            limit: 4,
        })
        .await
        .map_err(native)?;
    if inputs.current_durable_task.is_none() {
        return Err(unavailable());
    }
    Ok(PreparedDurableTask {
        execution_deadline: inputs.execution_deadline,
        inputs,
    })
}

/// The answer of one tool-free model step, read whole: a step that asks for a
/// call, stops at its output limit or never ends answers nothing.
pub(crate) async fn collect(
    provider: &Provider,
    request: Request<'_>,
    meter: &crate::Meter,
) -> Result<String> {
    let step = Step::read(provider.stream(request), meter).await?;
    if !step.finished || step.cut || !step.calls.is_empty() {
        return Err(unavailable());
    }
    Ok(step.text)
}

pub(crate) fn version(task: &DurableTaskContext<'_>) -> DecisionDurableTaskVersion {
    DecisionDurableTaskVersion {
        durable_task_id: task.durable_task_id.to_string(),
        generation: task.generation,
    }
}
