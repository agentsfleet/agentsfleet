//! Parent, connection, slot, run and admission locks precede recovery ownership.

use af_core::id::Uuid7;
use af_wire::run::{CheckpointOperationId, TurnScope, TurnWorkspace};
use sqlx::{Connection as _, PgConnection, Postgres, Transaction};

use super::{TurnCheckpointRefusal, TurnRun};
use crate::{
    Result,
    error::{query, run_cancelled, run_lost, run_not_found, stale_fence},
    run::{Runs, sql},
};

#[derive(sqlx::FromRow)]
pub(super) struct Current {
    pub id: Uuid7,
    pub fleet_id: Uuid7,
    pub workspace_id: Uuid7,
    pub event_id: Uuid7,
    pub setup_revision_id: Option<Uuid7>,
    pub fence: i64,
    pub slot_fence: i64,
    pub fleet_status: String,
    pub model: String,
    pub provider: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub run_expires_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub slot_expires_at: jiff::Timestamp,
}

#[derive(sqlx::FromRow)]
struct Admission {
    id: Uuid7,
    run_fence: Option<i64>,
    task_id: Option<Uuid7>,
    task_generation: Option<i64>,
    parked: bool,
    settled: bool,
}

/// Proof cannot outlive its transaction. The outer connection owner closes on cancellation.
pub(super) struct TurnRunGuard<'a> {
    pub tx: Transaction<'a, Postgres>,
    pub current: Current,
    pub setup: Uuid7,
}

impl<'a> TurnRunGuard<'a> {
    pub(super) async fn lock(
        runs: &Runs,
        connection: &'a mut PgConnection,
        presented: TurnRun<'_>,
    ) -> Result<Self> {
        let mut tx = connection
            .begin()
            .await
            .map_err(query("begin turn checkpoint ownership"))?;
        parents(&mut tx, presented).await?;
        runs.share_host(&mut tx, presented.runner_id)
            .await?
            .filter(|host| host.can_continue())
            .ok_or_else(run_lost)?;
        let current = current(&mut tx, presented).await?;
        if !exact_fence(current.fence, current.slot_fence, presented.fencing_token) {
            return Err(stale_fence());
        }
        if current.fleet_status != sql::FLEET_STATUS_ACTIVE {
            return Err(run_cancelled());
        }
        admission(&mut tx, &current).await?;
        let setup = current
            .setup_revision_id
            .clone()
            .ok_or(TurnCheckpointRefusal::Incompatible)?;
        let mut owner = Self { tx, current, setup };
        owner.fresh().await?;
        Ok(owner)
    }

    /// Every subsequent lock wait is followed by this database-clock proof.
    pub(super) async fn fresh(&mut self) -> Result<jiff::Timestamp> {
        let now: jiff_sqlx::Timestamp = sqlx::query_scalar(include_str!("sql/fresh_clock.sql"))
            .fetch_one(&mut *self.tx)
            .await
            .map_err(query("read post-lock turn clock"))?;
        let now = now.to_jiff();
        if !live(
            self.current.run_expires_at,
            self.current.slot_expires_at,
            now,
        ) {
            return Err(run_lost());
        }
        Ok(now)
    }

    pub(super) async fn commit(mut self) -> Result<()> {
        self.fresh().await?;
        self.tx
            .commit()
            .await
            .map_err(query("commit ordinary checkpoint ownership"))
    }

    pub(super) fn scope(&self, workspace: TurnWorkspace) -> TurnScope {
        TurnScope {
            workspace_id: CheckpointOperationId(self.current.workspace_id.as_str().to_owned()),
            fleet_id: CheckpointOperationId(self.current.fleet_id.as_str().to_owned()),
            event_id: CheckpointOperationId(self.current.event_id.as_str().to_owned()),
            setup_revision_id: CheckpointOperationId(self.setup.as_str().to_owned()),
            workspace,
        }
    }
}

async fn parents(connection: &mut PgConnection, presented: TurnRun<'_>) -> Result<()> {
    let (fleet, workspace): (Uuid7, Uuid7) = sqlx::query_as(include_str!("sql/scope.sql"))
        .bind(presented.run_id)
        .bind(presented.runner_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("read turn run scope"))?
        .ok_or_else(run_not_found)?;
    sqlx::query_scalar::<_, Uuid7>(include_str!("sql/lock_scope.sql"))
        .bind(fleet)
        .bind(workspace)
        .fetch_optional(connection)
        .await
        .map_err(query("hold turn workspace and fleet"))?
        .ok_or_else(run_not_found)?;
    Ok(())
}

async fn current(connection: &mut PgConnection, presented: TurnRun<'_>) -> Result<Current> {
    sqlx::query_scalar::<_, Uuid7>(include_str!("sql/lock_slot.sql"))
        .bind(presented.run_id)
        .bind(presented.runner_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("hold turn slot"))?
        .ok_or_else(run_not_found)?;
    sqlx::query_as(include_str!("sql/lock_run.sql"))
        .bind(presented.run_id)
        .bind(presented.runner_id)
        .bind(sql::RUN_STATUS_ACTIVE)
        .fetch_optional(connection)
        .await
        .map_err(query("hold ordinary turn run"))?
        .ok_or_else(run_not_found)
}

async fn admission(connection: &mut PgConnection, current: &Current) -> Result<()> {
    let admission: Admission = sqlx::query_as(include_str!("sql/lock_admission.sql"))
        .bind(&current.event_id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_optional(connection)
        .await
        .map_err(query("hold ordinary source admission"))?
        .ok_or_else(run_not_found)?;
    if admission.id != current.event_id
        || admission.run_fence != Some(current.fence)
        || admission.parked
        || admission.settled
        || admission.task_id.is_some()
        || admission.task_generation.is_some()
    {
        return Err(run_lost());
    }
    Ok(())
}

pub(super) fn id(value: &CheckpointOperationId) -> Result<Uuid7> {
    Uuid7::parse(&value.0).map_err(|_invalid| TurnCheckpointRefusal::Incompatible.into())
}

pub(super) fn exact_fence(run: i64, slot: i64, presented: u64) -> bool {
    run > 0 && run == slot && u64::try_from(run) == Ok(presented)
}

pub(super) fn live(run: jiff::Timestamp, slot: jiff::Timestamp, now: jiff::Timestamp) -> bool {
    run > now && slot > now
}
