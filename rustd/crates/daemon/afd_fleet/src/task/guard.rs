//! Existing fleet/run authority, held through the native task transaction.

use af_core::id::Uuid7;
use afd_db::Db;
use sqlx::PgConnection;

use super::{TaskRefusal, TaskRun, TaskScope, sql};
use crate::error::{Result, query, run_not_found, stale_fence};
use crate::run::{sql::RUN_STATUS_ACTIVE, standing::fence_holds};

const CONTEXT: &str = "prove durable task run ownership";

#[derive(Debug, sqlx::FromRow)]
pub(super) struct RunRow {
    pub run_id: Uuid7,
    pub fleet_id: Uuid7,
    pub workspace_id: Uuid7,
    pub tenant_id: Uuid7,
    pub source_admission_id: Uuid7,
    pub task_id: Option<Uuid7>,
    pub capable: bool,
    pub task_generation: Option<i64>,
    pub task_stop_generation: Option<i64>,
    pub fence: i64,
    pub live_fence: i64,
    pub revision_id: Option<Uuid7>,
    pub fleet_status: String,
    pub fleet_name: String,
    pub decision_snapshot: Option<sqlx::types::Json<af_wire::decision::DecisionAssignment>>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub checked_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub run_expires_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub slot_expires_at: jiff::Timestamp,
}

pub(super) async fn run_scope(database: &Db, presented: TaskRun<'_>) -> Result<TaskScope> {
    let mut connection = database.acquire().await?;
    let (fleet_id, workspace_id): (Uuid7, Uuid7) = sqlx::query_as(sql::RUN_SCOPE)
        .bind(presented.run_id)
        .bind(presented.runner_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or_else(run_not_found)?;
    Ok(TaskScope {
        workspace_id,
        fleet_id,
    })
}

pub(super) async fn run(connection: &mut PgConnection, presented: TaskRun<'_>) -> Result<RunRow> {
    locked_run(connection, presented, sql::LOCK_SLOT, sql::RUN).await
}

/// Receipt reads exclude writers without serializing other receipt readers.
/// The caller must end this transaction before attempting a mutation.
pub(super) async fn replay_run(
    connection: &mut PgConnection,
    presented: TaskRun<'_>,
) -> Result<RunRow> {
    locked_run(
        connection,
        presented,
        sql::LOCK_REPLAY_SLOT,
        sql::REPLAY_RUN,
    )
    .await
}

async fn locked_run(
    connection: &mut PgConnection,
    presented: TaskRun<'_>,
    slot_lock: &'static str,
    run_lock: &'static str,
) -> Result<RunRow> {
    // A short ownership read precedes the lock so fleet deletion cannot wait
    // on our slot while we wait on its parent row's foreign-key lock.
    let scope: Option<(Uuid7, Uuid7)> = sqlx::query_as(sql::RUN_SCOPE)
        .bind(presented.run_id)
        .bind(presented.runner_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let (fleet, workspace) = scope.ok_or_else(run_not_found)?;
    lock_scope(connection, &fleet, &workspace).await?;
    sqlx::query_scalar::<_, Uuid7>(slot_lock)
        .bind(presented.run_id)
        .bind(presented.runner_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or_else(run_not_found)?;
    // Slot and parent locks are already held when the existing deadline/fence
    // is checked. A row-lock wait cannot leave a pre-wait deadline accepted.
    let row: RunRow = sqlx::query_as(run_lock)
        .bind(presented.run_id)
        .bind(presented.runner_id)
        .bind(RUN_STATUS_ACTIVE)
        .bind(af_wire::task::CAPABILITY)
        .bind(afd_turns::Producer::SlackMention.as_str())
        .bind(afd_connector::Provider::Slack.id())
        .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
        .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or_else(run_not_found)?;
    if !row.capable || !fence_holds(row.fence, row.live_fence, presented.fencing_token) {
        return Err(stale_fence());
    }
    Ok(row)
}

pub(super) async fn scope(connection: &mut PgConnection, scope: &TaskScope) -> Result<()> {
    lock_scope(connection, &scope.fleet_id, &scope.workspace_id).await?;
    let _slot = sqlx::query_scalar::<_, Uuid7>(sql::LOCK_SCOPE_SLOT)
        .bind(&scope.fleet_id)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

pub(super) async fn context(
    connection: &mut PgConnection,
    current: &RunRow,
) -> Result<Option<af_wire::task::TaskContext<'static>>> {
    let Some(task) = &current.task_id else {
        return Ok(None);
    };
    let scope = TaskScope {
        fleet_id: current.fleet_id.clone(),
        workspace_id: current.workspace_id.clone(),
    };
    Ok(Some(
        super::read::on(connection, &scope, task).await?.context,
    ))
}

pub(super) async fn lock_scope(
    connection: &mut PgConnection,
    fleet: &Uuid7,
    workspace: &Uuid7,
) -> Result<()> {
    sqlx::query_scalar::<_, Uuid7>(sql::LOCK_SCOPE)
        .bind(fleet)
        .bind(workspace)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(TaskRefusal::NotFound)?;
    Ok(())
}

/// An identifier the runner sent as text: one that is not a `UUIDv7` names no
/// row, so it is refused as not found rather than bound and refused by
/// Postgres.
pub(super) fn wire(text: &str) -> Result<Uuid7> {
    Uuid7::parse(text).map_err(|_text| TaskRefusal::NotFound.into())
}

pub(super) fn id(text: &str, column: &'static str) -> Result<Uuid7> {
    Uuid7::parse(text).map_err(crate::error::row_malformed("fleet.tasks", column))
}
