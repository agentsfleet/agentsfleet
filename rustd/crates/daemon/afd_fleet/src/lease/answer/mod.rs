//! What a poll answers, as the bytes a runner reads.
//!
//! Split from [`super::pull`] because it is the half with no decisions in it:
//! everything here renders a conclusion someone else reached. Keeping it apart
//! is what lets the verb next door read as a sequence of decisions rather than
//! as a sequence of decisions interleaved with serialization.

use af_core::id::Uuid7;
use af_wire::decision::DecisionAssignment;
use af_wire::event::{EventEnvelope, EventType};
use af_wire::lease::{LeasePayload, SecretDelivery, Turn};
use af_wire::memory::MemoryHydrateResponse;
use af_wire::policy::ExecutionPolicy;

use crate::lease::envelope::Acquired;
use crate::lease::installed::Installed;

/// The event a poll that issued nothing is logged under.
pub(super) const EVENT_NO_WORK: &str = "runner_lease_no_work";

/// The event an issued lease is logged under.
pub(super) const EVENT_LEASED: &str = "runner_lease_issued";

/// The event a refused delivery is logged under.
pub(super) const EVENT_REFUSED: &str = "runner_lease_refused";

/// A placement decision that sends no frame; its reason stays in bounded operator telemetry.
pub(super) fn no_work(runner: &Uuid7, reason: &str) {
    let runner_id = runner.as_str();
    tracing::debug!(event = EVENT_NO_WORK, runner_id, reason);
}

/// What a lease carries about the fleet beyond its event: the thread's earlier
/// turns and the memory window, both read after the issue.
#[derive(Debug, Default)]
pub(super) struct Context<'a> {
    pub(super) history: Vec<Turn<'a>>,
    pub(super) memory: Option<MemoryHydrateResponse<'a>>,
    pub(super) decision: Option<Box<DecisionAssignment>>,
    pub(super) task: Option<af_wire::task::TaskContext<'static>>,
    pub(super) expires_at: Option<jiff::Timestamp>,
    pub(super) execution_deadline: Option<jiff::Timestamp>,
}

/// Assemble the issued payload while its policy still borrows the admitted work.
pub(super) fn payload<'a>(
    lease_id: &'a Uuid7,
    acquired: &'a Acquired,
    event_type: EventType,
    installed: &'a Installed,
    policy: ExecutionPolicy<'a>,
    context: Context<'a>,
) -> LeasePayload<'a> {
    LeasePayload {
        lease_id: lease_id.as_str().into(),
        fencing_token: acquired.fence.as_u64(),
        lease_expires_at: context.expires_at.unwrap_or(acquired.leased_until),
        execution_deadline: context.execution_deadline,
        // The one delivery this port serves. `scoped` and `proxy` are wire
        // values with no implementation on either side.
        secret_delivery: SecretDelivery::Inline,
        event: EventEnvelope {
            event_id: acquired.event_id.as_str().into(),
            fleet_id: acquired.fleet_id.as_str().into(),
            workspace_id: acquired.workspace_id.as_str().into(),
            actor: acquired.actor.as_str().into(),
            event_type,
            request_json: acquired.request_json.as_str().into(),
            created_at: acquired.event_created_at,
        },
        policy,
        instructions: installed.instructions.as_str().into(),
        // No fleet carries a size yet, so every runner builds its defaults.
        limits: None,
        history: context.history,
        resume_hold: acquired.resume_hold,
        decision: context.decision,
        task: context.task,
        durable_tasks: installed.config.decision_policy().is_some(),
        memory: context.memory,
        inputs_waiting: acquired.opened.inputs_waiting,
    }
}

#[cfg(all(test, feature = "test-util"))]
mod tests;
