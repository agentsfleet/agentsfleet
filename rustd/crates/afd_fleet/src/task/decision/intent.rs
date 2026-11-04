//! One reservation transaction stores native identity, request and task credit.

use afd_core::id::Uuid7;
use afd_wire::{
    decision::DecisionPurpose,
    task::{DecisionCallState, DecisionReservationInput, TaskContext},
};
use jiff_sqlx::ToSqlx as _;
use sqlx::{PgConnection, types::Json};

use super::{attempt::Attempt, budget, prepare::Prepared, quote::FrozenQuote, sql};
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
                .bind(goal.task_id.as_ref())
                .bind(&self.prepared.current.fleet_id)
                .bind(&self.prepared.current.workspace_id)
                .bind(budget::CALL_NANOS)
                .execute(connection)
                .await
                .map_err(query("reserve cumulative task decision credit"))?;
        }
        Ok(())
    }

    fn goal(&self) -> Option<&TaskContext<'_>> {
        self.prepared
            .task
            .as_ref()
            .filter(|_task| self.prepared.request.purpose == DecisionPurpose::Goal)
            .map(|task| &task.context)
    }

    async fn sequence(&self, connection: &mut PgConnection) -> Result<i64> {
        let sequence = sqlx::query_scalar(sql::SEQUENCE)
            .bind(self.lease.lease_id.as_str())
            .bind(i64::from(afd_wire::usage::DECISION_SEQ_BASE))
            .fetch_one(connection)
            .await
            .map_err(query("allocate the native decision usage sequence"))?;
        u32::try_from(sequence)
            .ok()
            .filter(|seq| *seq >= afd_wire::usage::DECISION_SEQ_BASE)
            .ok_or_else(sequence_corrupt)?;
        Ok(sequence)
    }

    async fn header(&self, connection: &mut PgConnection, sequence: i64) -> Result<()> {
        let prepared = self.prepared;
        let attempt = self.attempt;
        let generation = self
            .goal()
            .map(|task| i64::try_from(task.generation))
            .transpose()
            .map_err(|source| {
                crate::error::Error::from(crate::error::ErrorKind::TaskCounter {
                    column: "task_generation",
                    source,
                })
            })?;
        sqlx::query(sql::INSERT_CALL)
            .bind(self.id.as_str())
            .bind(self.lease.runner_id.as_str())
            .bind(self.lease.lease_id.as_str())
            .bind(&prepared.context.source_admission_id)
            .bind(&prepared.current.fleet_id)
            .bind(&prepared.current.workspace_id)
            .bind(&prepared.current.tenant_id)
            .bind(self.goal().map(|task| task.task_id.as_ref()))
            .bind(generation)
            .bind(prepared.current.fence)
            .bind(&self.input.operation_id)
            .bind(self.digest)
            .bind(attempt.assessment_id())
            .bind(self.key)
            .bind(attempt.number())
            .bind(attempt.parent().map(|parent| parent.id.as_str()))
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
            .execute(connection)
            .await
            .map_err(query("save immutable physical decision intent"))?;
        Ok(())
    }

    async fn payload(&self, connection: &mut PgConnection) -> Result<()> {
        sqlx::query(sql::INSERT_PAYLOAD)
            .bind(self.id.as_str())
            .bind(&self.prepared.current.fleet_id)
            .bind(&self.prepared.current.workspace_id)
            .bind(&self.prepared.current.tenant_id)
            .bind(self.goal().map(|task| task.task_id.as_ref()))
            .bind(Json(self.attempt.request(&self.prepared.request)))
            .execute(connection)
            .await
            .map_err(query("save bounded native decision request"))?;
        Ok(())
    }
}
