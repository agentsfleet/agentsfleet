//! Native continuations are ordinary admissions with an immutable task payload.

use afd_admission::{Admission, Key, Producer, Reply};
use afd_core::id::Uuid7;
use afd_wire::event::EventType;

use super::{TaskScope, Tasks};

pub(super) struct Continuation {
    scope: TaskScope,
    actor: String,
    key: String,
    payload: String,
    source_admission_id: Uuid7,
}

impl Continuation {
    pub(super) fn new(scope: &TaskScope, task_id: &Uuid7, generation: u64, source: &Uuid7) -> Self {
        Self {
            scope: scope.clone(),
            actor: format!("task:{}", task_id.as_str()),
            key: format!("{}:{generation}", task_id.as_str()),
            payload: serde_json::json!({"kind":"task_continuation", "task_id":task_id, "generation":generation}).to_string(),
            source_admission_id: source.clone(),
        }
    }

    pub(super) fn admission(&self) -> Admission<'_> {
        Admission {
            producer: Producer::TaskContinuation,
            key: Key::Repeated(&self.key),
            fleet: self.scope.fleet_id.as_str(),
            workspace: self.scope.workspace_id.as_str(),
            actor: &self.actor,
            event_type: EventType::Continuation,
            request_json: &self.payload,
            reply: Reply::Inherit {
                event_id: self.source_admission_id.as_str(),
            },
        }
    }

    pub(super) async fn confirm(&self, tasks: &Tasks) {
        // A missed doorbell cannot undo a committed admission; normal ledger
        // discovery still finds the row and this call's replay remains stable.
        if let Err(error) = tasks
            .admissions
            .confirm(Producer::TaskContinuation, &self.key)
            .await
        {
            tracing::warn!(event = "dt_wake_hint_failed", error_code = %error.code());
        }
    }
}
