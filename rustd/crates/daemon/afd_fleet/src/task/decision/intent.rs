//! One reservation transaction stores native identity, request and task credit.

use af_core::id::Uuid7;
use af_wire::{
    decision::DecisionPurpose,
    task::{DecisionCallState, DecisionReservationInput},
};
use jiff_sqlx::ToSqlx as _;
use sqlx::{PgConnection, types::Json};

use super::{
    attempt::Attempt, budget, ownership::Execution, prepare::Prepared, quote::FrozenQuote, sql,
};
use crate::{
    Result,
    error::{query, sequence_corrupt},
    task::TaskLease,
};

pub(super) struct Intent<'a> {
    pub lease: TaskLease<'a>,
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
        if let Some(goal) = self.goal() {
            sqlx::query(sql::RESERVE_TASK)
                .bind(crate::task::guard::wire(
                    goal.snapshot.context.task_id.as_ref(),
                )?)
                .bind(&self.prepared.current.fleet_id)
                .bind(&self.prepared.current.workspace_id)
                .bind(budget::CALL_NANOS)
                .execute(connection)
                .await
                .map_err(query("reserve cumulative task decision credit"))?;
        }
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
            .bind(self.lease.lease_id)
            .bind(i64::from(af_wire::usage::DECISION_SEQ_BASE))
            .fetch_one(connection)
            .await
            .map_err(query("allocate the native decision usage sequence"))?;
        u32::try_from(sequence)
            .ok()
            .filter(|seq| *seq >= af_wire::usage::DECISION_SEQ_BASE)
            .ok_or_else(sequence_corrupt)?;
        Ok(sequence)
    }

    async fn header(&self, connection: &mut PgConnection, sequence: i64) -> Result<()> {
        let prepared = self.prepared;
        let attempt = self.attempt;
        let generation = self.goal().map(|task| task.generation);
        sqlx::query(sql::INSERT_CALL)
            .bind(self.id)
            .bind(self.lease.runner_id)
            .bind(self.lease.lease_id)
            .bind(&prepared.context.source_admission_id)
            .bind(&prepared.current.fleet_id)
            .bind(&prepared.current.workspace_id)
            .bind(&prepared.current.tenant_id)
            .bind(
                self.goal()
                    .map(|task| task.snapshot.context.task_id.as_ref())
                    .map(crate::task::guard::wire)
                    .transpose()?,
            )
            .bind(generation)
            .bind(prepared.current.fence)
            .bind(crate::task::guard::wire(&self.input.operation_id)?)
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
            .bind(crate::task::guard::wire(
                &prepared.assignment.profile.model_id,
            )?)
            .bind(crate::task::guard::wire(&prepared.assignment.revision_id)?)
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
            .bind(
                self.goal()
                    .map(|task| task.snapshot.context.task_id.as_ref())
                    .map(crate::task::guard::wire)
                    .transpose()?,
            )
            .bind(Json(self.attempt.request(&self.prepared.request)))
            .execute(connection)
            .await
            .map_err(query("save bounded native decision request"))?;
        Ok(())
    }
}
