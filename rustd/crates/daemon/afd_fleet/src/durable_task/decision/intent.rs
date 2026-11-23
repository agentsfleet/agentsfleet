//! One reservation transaction stores native identity and request.

use af_core::id::Uuid7;
use af_wire::{
    decision::DecisionPurpose,
    durable_task::{DecisionCallState, DecisionReservationInput},
};
use jiff_sqlx::ToSqlx as _;
use sqlx::{PgConnection, types::Json};

use super::{
    attempt::Attempt, budget, ownership::Execution, prepare::Prepared, quote::FrozenQuote, sql,
};
use crate::{
    Result,
    durable_task::DurableTaskRun,
    error::{query, sequence_corrupt},
};

pub(super) struct Intent<'a> {
    pub run: DurableTaskRun<'a>,
    pub input: &'a DecisionReservationInput,
    pub id: &'a Uuid7,
    pub prepared: &'a Prepared,
    pub quote: &'a FrozenQuote,
    pub attempt: &'a Attempt,
    pub key: &'a str,
    pub digest: &'a str,
}

impl Intent<'_> {
    pub(super) async fn save(&self, connection: &mut PgConnection) -> Result<()> {
        let sequence = self.sequence(connection).await?;
        self.header(connection, sequence).await?;
        self.payload(connection).await?;
        Ok(())
    }

    fn goal(&self) -> Option<&Execution> {
        self.prepared
            .task
            .as_ref()
            .filter(|_task| self.prepared.request.purpose == DecisionPurpose::Goal)
    }

    async fn sequence(&self, connection: &mut PgConnection) -> Result<i64> {
        let sequence = sqlx::query_scalar(sql::SEQUENCE)
            .bind(self.run.run_id)
            .fetch_one(connection)
            .await
            .map_err(query("allocate the native decision usage sequence"))?;
        u32::try_from(sequence).map_err(|_range| sequence_corrupt())?;
        Ok(sequence)
    }

    async fn header(&self, connection: &mut PgConnection, sequence: i64) -> Result<()> {
        let prepared = self.prepared;
        let attempt = self.attempt;
        let generation = self.goal().map(|task| task.generation);
        sqlx::query(sql::INSERT_CALL)
            .bind(self.id)
            .bind(self.run.runner_id)
            .bind(self.run.run_id)
            .bind(&prepared.context.source_turn_id)
            .bind(&prepared.current.fleet_id)
            .bind(&prepared.current.workspace_id)
            .bind(&prepared.current.tenant_id)
            .bind(self.goal().map(|task| &task.durable_task_id))
            .bind(generation)
            .bind(prepared.current.fence)
            .bind(crate::durable_task::guard::wire(&self.input.operation_id)?)
            .bind(self.digest)
            .bind(attempt.assessment_id())
            .bind(self.key)
            .bind(attempt.number())
            .bind(attempt.parent().map(|parent| &parent.id))
            .bind(
                attempt
                    .parent()
                    .map(|parent| parent.logical_expires_at.to_sqlx()),
            )
            .bind(prepared.current.checked_at.to_sqlx())
            .bind(budget::LOGICAL_SECONDS)
            .bind(prepared.request.purpose.as_str())
            .bind(DecisionCallState::Reserved.as_str())
            .bind(&prepared.provider)
            .bind(&prepared.current.fleet_name)
            .bind(Json(&prepared.assignment))
            .bind(Json(attempt.quote(self.quote)))
            .bind(Json(attempt.parent().map_or(&prepared.context, |parent| {
                &parent.context_snapshot.0
            })))
            .bind(sequence)
            .bind(budget::CALL_NANOS)
            .bind(crate::durable_task::guard::wire(
                &prepared.assignment.profile.model_id,
            )?)
            .bind(crate::durable_task::guard::wire(
                &prepared.assignment.revision_id,
            )?)
            .execute(connection)
            .await
            .map_err(query("save immutable physical decision intent"))?;
        Ok(())
    }

    async fn payload(&self, connection: &mut PgConnection) -> Result<()> {
        sqlx::query(sql::INSERT_PAYLOAD)
            .bind(self.id)
            .bind(&self.prepared.current.fleet_id)
            .bind(&self.prepared.current.workspace_id)
            .bind(&self.prepared.current.tenant_id)
            .bind(self.goal().map(|task| &task.durable_task_id))
            .bind(Json(self.attempt.request(&self.prepared.request)))
            .execute(connection)
            .await
            .map_err(query("save bounded native decision request"))?;
        Ok(())
    }
}
