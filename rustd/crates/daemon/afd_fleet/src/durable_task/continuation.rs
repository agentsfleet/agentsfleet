//! Native continuations are ordinary turns with an immutable task payload.

use af_core::id::Uuid7;
use af_wire::turn::TurnKind;
use afd_turns::{IncomingTurn, Key, Producer, Reply};

use super::{DurableTaskScope, DurableTasks};

#[derive(Debug)]
pub(super) struct Continuation {
    scope: DurableTaskScope,
    actor: String,
    key: String,
    payload: String,
    destination: Destination,
}

#[derive(Debug)]
enum Destination {
    Inherit(Uuid7),
    Recorded(afd_connector::Provider, String),
    None,
}

impl Continuation {
    pub(super) fn new(
        scope: &DurableTaskScope,
        durable_task_id: &Uuid7,
        generation: u64,
        source: &Uuid7,
    ) -> Self {
        Self {
            scope: scope.clone(),
            actor: format!("durable_task:{}", durable_task_id.as_str()),
            key: format!("{}:{generation}", durable_task_id.as_str()),
            payload: serde_json::json!({"kind":"durable_task_continuation", "durable_task_id":durable_task_id, "generation":generation}).to_string(),
            destination: Destination::Inherit(source.clone()),
        }
    }

    pub(super) fn with_reply(
        mut self,
        provider: Option<&str>,
        address: Option<&str>,
    ) -> crate::Result<Self> {
        self.destination = match (provider, address) {
            (None, None) => Destination::None,
            (Some(provider), Some(address)) => Destination::Recorded(
                afd_connector::Provider::parse(provider)
                    .ok_or(super::DurableTaskRefusal::InvalidTarget)?,
                address.to_owned(),
            ),
            _ => return Err(super::DurableTaskRefusal::InvalidTarget.into()),
        };
        Ok(self)
    }

    pub(super) fn turn(&self) -> IncomingTurn<'_> {
        IncomingTurn {
            producer: Producer::DurableTaskContinuation,
            key: Key::Repeated(&self.key),
            fleet: &self.scope.fleet_id,
            workspace: &self.scope.workspace_id,
            actor: &self.actor,
            event_type: TurnKind::Continuation,
            request_json: &self.payload,
            reply: match &self.destination {
                Destination::Recorded(provider, address) => Reply::To {
                    connector: provider.id(),
                    address,
                },
                Destination::None => Reply::None,
                Destination::Inherit(source) => Reply::Inherit { turn_id: source },
            },
        }
    }

    pub(super) async fn confirm(&self, tasks: &DurableTasks) {
        // A lost wake ring cannot undo a committed turn; normal ledger
        // discovery still finds the row and this call's replay remains stable.
        if let Err(error) = tasks
            .turns
            .confirm(Producer::DurableTaskContinuation, &self.key)
            .await
        {
            tracing::warn!(event = "dt_wake_hint_failed", error_code = %error.code());
        }
    }
}

/// Retire superseded native work without settling an issued run or human reply.
pub(super) async fn retire(connection: &mut sqlx::PgConnection, task: &Uuid7) -> crate::Result<()> {
    sqlx::query(include_str!("sql/retire_continuations.sql"))
        .bind(task)
        .bind(Producer::DurableTaskContinuation.as_str())
        .execute(connection)
        .await
        .map_err(crate::error::query(
            "retire never-issued obsolete task continuations",
        ))?;
    Ok(())
}

#[cfg(test)]
#[path = "continuation_tests.rs"]
mod tests;
