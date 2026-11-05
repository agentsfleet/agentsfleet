//! Native continuations are ordinary admissions with an immutable task payload.

use afd_admission::{Admission, Key, Producer, Reply};
use afd_core::id::Uuid7;
use afd_wire::event::EventType;

use super::{TaskScope, Tasks};

#[derive(Debug)]
pub(super) struct Continuation {
    scope: TaskScope,
    actor: String,
    key: String,
    payload: String,
    destination: Destination,
}

#[derive(Debug)]
enum Destination { Inherit(Uuid7), Recorded(afd_connector::Provider, String), None }

impl Continuation {
    pub(super) fn new(scope: &TaskScope, task_id: &Uuid7, generation: u64, source: &Uuid7) -> Self {
        Self {
            scope: scope.clone(),
            actor: format!("task:{}", task_id.as_str()),
            key: format!("{}:{generation}", task_id.as_str()),
            payload: serde_json::json!({"kind":"task_continuation", "task_id":task_id, "generation":generation}).to_string(),
            destination: Destination::Inherit(source.clone()),
        }
    }

    pub(super) fn with_reply(mut self, provider: Option<&str>, address: Option<&str>) -> crate::Result<Self> {
        self.destination = match (provider, address) {
            (None, None) => Destination::None,
            (Some(provider), Some(address)) => Destination::Recorded(afd_connector::Provider::parse(provider).ok_or(super::TaskRefusal::InvalidTarget)?, address.to_owned()),
            _ => return Err(super::TaskRefusal::InvalidTarget.into()),
        };
        Ok(self)
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
            reply: match &self.destination {
                Destination::Recorded(provider, address) => Reply::To { connector: provider.id(), address },
                Destination::None => Reply::None,
                Destination::Inherit(source) => Reply::Inherit { event_id: source.as_str() },
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

/// Retire superseded native work without settling an issued lease or human reply.
pub(super) async fn retire(connection: &mut sqlx::PgConnection, task: &str) -> crate::Result<()> {
    sqlx::query(include_str!("sql/retire_continuations.sql")).bind(task).bind(Producer::TaskContinuation.as_str())
        .execute(connection).await.map_err(crate::error::query("retire never-issued obsolete task continuations"))?;
    Ok(())
}
