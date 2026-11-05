//! Explicit reviewed question adoption fences old assessments without resetting work.

use afd_core::id::Uuid7;
use afd_wire::task::TaskState;
use sqlx::PgConnection;

use super::{TaskAdoption, TaskRefusal, TaskScope, TaskSnapshot, Tasks, continuation::Continuation,
    decision::compile, fetch, guard, read, row, sql, transaction};
use crate::{Result, error::{ErrorKind, query}, lease::Leases};

const CONTEXT: &str = "adopt reviewed task goal";

#[derive(sqlx::FromRow)]
struct Locked { can_resume: bool }

#[derive(sqlx::FromRow)]
struct Origin {
    admission_id: String,
    reply_provider: Option<String>, reply_address: Option<String>,
    generation: i64,
    state: String,
    accepted_revision_id: String,
    request_digest: Option<String>,
}

pub(super) async fn apply(
    tasks: &Tasks,
    scope: &TaskScope,
    task: &Uuid7,
    expected_tag: &str,
    adoption: &TaskAdoption,
) -> Result<TaskSnapshot> {
    let digest = digest(expected_tag, adoption)?;
    let origin = origin(tasks, scope, task, adoption).await?;
    if let Some(saved) = origin.request_digest.as_deref() {
        if saved != digest { return Err(ErrorKind::TaskMutation {
            state: row::state(&origin.state)?, source: Box::new(TaskRefusal::ReplayConflict.into()),
        }.into()); }
        return read::read(tasks, scope, task).await;
    }
    let eligible = matches!(row::state(&origin.state)?, TaskState::Running | TaskState::Ready | TaskState::Waiting);
    let changed = origin.accepted_revision_id != adoption.goal_revision_id.as_str();
    let wake = if eligible && changed { Some(Continuation::new(scope, task,
        row::counter(origin.generation, "generation")?.checked_add(1).ok_or(TaskRefusal::LimitExhausted)?,
        &guard::id(&origin.admission_id, "source_admission_id")?).with_reply(origin.reply_provider.as_deref(), origin.reply_address.as_deref())?) } else { None };
    let prepared = match &wake { Some(wake) => Some(tasks.admissions.prepare(wake.admission()).await?), None => None };
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    guard::scope(&mut tx, scope).await?;
    let locked: Locked = sqlx::query_as(sql::LOCK_TASK).bind(task.as_str()).bind(scope.fleet_id.as_str())
        .bind(scope.workspace_id.as_str()).fetch_optional(&mut *tx).await.map_err(query(CONTEXT))?.ok_or(TaskRefusal::NotFound)?;
    let current = read::on(&mut tx, scope, task).await?;
    if replay(&mut tx, scope, task, adoption, &digest).await.map_err(|source| ErrorKind::TaskMutation {
        state: current.context.state, source: Box::new(source),
    })? {
        tx.commit().await.map_err(query(CONTEXT))?;
        transaction::recycle(connection);
        return Ok(current);
    }
    check(&current, expected_tag, adoption)?;
    if eligible && changed && !locked.can_resume { return Err(TaskRefusal::LimitExhausted.into()); }
    let questions = questions(&mut tx, scope, &current.context.accepted_revision_id, &adoption.goal_revision_id).await
        .map_err(|source| ErrorKind::TaskMutation { state: current.context.state, source: Box::new(source) })?;
    let changed = current.context.accepted_revision_id.as_ref() != adoption.goal_revision_id.as_str();
    if changed {
        super::waits::cancel(&mut tx, task.as_str()).await?;
        let goal_digest = afd_core::etag::sha256_hex(&serde_json::to_vec(&(&current.context.objective, &questions))
            .map_err(afd_core::json::Unreadable::from)?);
        sqlx::query(include_str!("sql/adopt_goal.sql"))
            .bind(task.as_str()).bind(adoption.goal_revision_id.as_str()).bind(sqlx::types::Json(questions))
            .bind(goal_digest).bind(TaskState::Ready.as_str()).bind(eligible)
            .execute(&mut *tx).await.map_err(query(CONTEXT))?;
        super::continuation::retire(&mut tx, task.as_str()).await?;
        if eligible {
            let admitted = tasks.admissions.stage(&mut tx, prepared.ok_or(TaskRefusal::StaleState)?).await
                .map_err(|source| ErrorKind::TaskControlAdmission { state: current.context.state, source })?;
            sqlx::query(sql::CONTROL_ADMISSION).bind(task.as_str()).bind(&admitted.stored.id)
                .execute(&mut *tx).await.map_err(query(CONTEXT))?;
        }
    }
    sqlx::query(include_str!("sql/adoption_receipt.sql")).bind(adoption.operation_id.as_str())
        .bind(task.as_str()).bind(scope.fleet_id.as_str()).bind(scope.workspace_id.as_str()).bind(digest)
        .execute(&mut *tx).await.map_err(query(CONTEXT))?;
    let snapshot = read::on(&mut tx, scope, task).await?;
    tx.commit().await.map_err(query(CONTEXT))?;
    transaction::recycle(connection);
    if changed {
        tasks.publish_changed(scope, task, snapshot.resource_version).await;
        if let Some(wake) = wake { wake.confirm(tasks).await; }
    }
    Ok(snapshot)
}

fn check(current: &TaskSnapshot, expected_tag: &str, adoption: &TaskAdoption) -> Result<()> {
    if current.etag() != expected_tag {
        return Err(ErrorKind::TaskPrecondition { current_etag: current.etag() }.into());
    }
    if current.context.generation != adoption.expected_generation || current.context.state.is_terminal() {
        return Err(ErrorKind::TaskControlConflict { state: current.context.state }.into());
    }
    Ok(())
}

async fn questions(connection: &mut PgConnection, scope: &TaskScope, previous: &str, revision: &Uuid7) -> Result<Vec<afd_wire::decision::NamedQuestion>> {
    let reviewed: bool = sqlx::query_scalar(include_str!("sql/adoption_revision.sql"))
        .bind(scope.fleet_id.as_str()).bind(scope.workspace_id.as_str()).bind(revision.as_str())
        .fetch_one(&mut *connection).await.map_err(query(CONTEXT))?;
    if !reviewed { return Err(TaskRefusal::InvalidTarget.into()); }
    let config = Leases::stored_config_on(connection, scope.fleet_id.as_str(), Some(revision.as_str()))
        .await?.ok_or(TaskRefusal::InvalidTarget)?;
    let original = Leases::stored_config_on(connection, scope.fleet_id.as_str(), Some(previous))
        .await?.ok_or(TaskRefusal::InvalidTarget)?;
    if !original.same_task_authority(&config) { return Err(TaskRefusal::InvalidTarget.into()); }
    compile::completion(config.decision_policy().ok_or(TaskRefusal::DecisionUnavailable)?)
}

async fn replay(connection: &mut PgConnection, scope: &TaskScope, task: &Uuid7, adoption: &TaskAdoption, digest: &str) -> Result<bool> {
    let saved: Option<String> = sqlx::query_scalar(include_str!("sql/adoption_replay.sql"))
        .bind(task.as_str()).bind(adoption.operation_id.as_str()).bind(scope.fleet_id.as_str()).bind(scope.workspace_id.as_str())
        .fetch_optional(connection).await.map_err(query(CONTEXT))?;
    if saved.as_deref().is_some_and(|saved| saved != digest) { return Err(TaskRefusal::ReplayConflict.into()); }
    Ok(saved.is_some())
}

fn digest(tag: &str, adoption: &TaskAdoption) -> Result<String> {
    Ok(afd_core::etag::sha256_hex(&serde_json::to_vec(&(tag, adoption.goal_revision_id.as_str(), adoption.expected_generation))
        .map_err(afd_core::json::Unreadable::from)?))
}

async fn origin(tasks: &Tasks, scope: &TaskScope, task: &Uuid7, adoption: &TaskAdoption) -> Result<Origin> {
    fetch::rows(tasks.leases.pool(), sqlx::query_as(include_str!("sql/adoption_origin.sql"))
        .bind(task.as_str()).bind(scope.fleet_id.as_str()).bind(scope.workspace_id.as_str())
        .bind(adoption.operation_id.as_str()), None, CONTEXT).await?
        .into_iter().next().ok_or_else(|| TaskRefusal::NotFound.into())
}
