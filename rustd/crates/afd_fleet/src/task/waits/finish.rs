//! A stale observation never consumes a newer wait or reopens a stopped task.

use std::time::Duration;
use afd_core::clock;
use afd_wire::task::TaskState;
use jiff_sqlx::ToSqlx as _;
use sqlx::{Connection as _, PgConnection, types::Json};
use super::{claim, row::Wait};
use crate::{Result, error::query, task::{Tasks, TaskScope, continuation::Continuation, guard, github::status::Snapshot, transaction}};

pub(super) struct Probe { pub owner: Wait, pub claim: afd_core::id::Uuid7, pub result: Result<Snapshot> }

pub(super) async fn finish(tasks: &Tasks, probe: Probe) -> Result<bool> {
    let scope = TaskScope { fleet_id: guard::id(&probe.owner.fleet_id, "fleet_id")?, workspace_id: guard::id(&probe.owner.workspace_id, "workspace_id")? };
    let wake = Continuation::new(&scope, &guard::id(&probe.owner.task_id, "task_id")?,
        u64::try_from(probe.owner.task_generation).ok().and_then(|value| value.checked_add(1)).ok_or(crate::task::TaskRefusal::LimitExhausted)?,
        &guard::id(&probe.owner.source_admission_id, "source_admission_id")?)
        .with_reply(probe.owner.reply_provider.as_deref(), probe.owner.reply_address.as_deref())?;
    let prepare = if probe.result.as_ref().is_ok_and(|snapshot| actionable(snapshot, &probe.owner)) {
        Some(tasks.admissions.prepare(wake.admission()).await?)
    } else { None };
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    guard::scope(&mut tx, &scope).await?;
    let _task: Option<String> = sqlx::query_scalar(crate::task::sql::LOCK_TASK).bind(&probe.owner.task_id)
        .bind(&probe.owner.fleet_id).bind(&probe.owner.workspace_id).fetch_optional(&mut *tx).await.map_err(query("hold task before consuming its wait"))?;
    let Some(owner) = claim::owner(&mut tx, &probe.owner.id).await? else { return Ok(false); };
    if !owner.current() || owner.claim_valid != Some(true) || owner.claim_id.as_deref() != Some(probe.claim.as_str()) { return Ok(false); }
    let woken = apply(tasks, &mut tx, &owner, &probe, prepare).await?;
    tx.commit().await.map_err(query("commit native wait observation and its sole continuation"))?;
    transaction::recycle(connection);
    if woken { wake.confirm(tasks).await; }
    let task = guard::id(&owner.task_id, "task_id")?;
    if let Ok(snapshot) = crate::task::read::read(tasks, &scope, &task).await {
        tasks.publish_changed(&scope, &task, snapshot.resource_version).await;
    }
    Ok(woken)
}

async fn apply(tasks: &Tasks, connection: &mut PgConnection, owner: &Wait, probe: &Probe,
    prepared: Option<afd_admission::PreparedAdmission<'_>>) -> Result<bool> {
    if !owner.within_limits {
        claim::stop(connection, owner, TaskState::Exhausted, "cumulative_limit_reached").await?;
        return Ok(false);
    }
    if let Err(fault) = claim::authority(connection, owner).await {
        if fault.is_datastore_unavailable() { return Err(fault); }
        claim::stop(connection, owner, TaskState::Blocked, "dependency_access_unavailable").await?;
        return Ok(false);
    }
    match &probe.result {
        Err(fault) => {
            tracing::warn!(event = "dt_wait_read_deferred", task_id = owner.task_id.as_str(), error_code = %fault.code());
            update(connection, owner, probe, None, fault.remote_cooldown()).await?;
            Ok(false)
        }
        Ok(snapshot) if !snapshot.open || snapshot.head != owner.expected_head => {
            let reason = if snapshot.open { "pull_request_head_changed" } else { "pull_request_closed" };
            claim::stop(connection, owner, TaskState::Blocked, reason).await?;
            Ok(false)
        }
        Ok(snapshot) if actionable(snapshot, owner) => {
            let Some(prepared) = prepared else { return Err(crate::task::TaskRefusal::StaleState.into()); };
            let mut savepoint = connection.begin().await.map_err(query("begin wait continuation savepoint"))?;
            let admitted = match tasks.admissions.stage(&mut savepoint, prepared).await {
                Ok(admitted) => admitted,
                Err(fault) if fault.is_over_capacity() || fault.code() == afd_core::error_code::AUTHORING_POLICY => {
                    savepoint.rollback().await.map_err(query("retain wait evidence after admission refusal"))?;
                    claim::stop(connection, owner, TaskState::Blocked, "continuation_unavailable").await?;
                    return Ok(false);
                }
                Err(fault) => return Err(fault.into()),
            };
            sqlx::query(include_str!("sql/transition.sql")).bind(&owner.task_id).bind(TaskState::Ready.as_str())
                .bind(None::<&str>).bind(false).execute(&mut *savepoint).await.map_err(query("make consumed dependency continuation ready"))?;
            crate::task::continuation::retire(&mut savepoint, &owner.task_id).await?;
            sqlx::query(crate::task::sql::CONTROL_ADMISSION).bind(&owner.task_id).bind(&admitted.stored.id)
                .execute(&mut *savepoint).await.map_err(query("bind single native wait continuation"))?;
            sqlx::query(include_str!("sql/consume.sql")).bind(&owner.id).bind(probe.claim.as_str()).bind(&admitted.stored.id)
                .execute(&mut *savepoint).await.map_err(query("consume current wait exactly once"))?;
            savepoint.commit().await.map_err(query("commit wait continuation savepoint"))?;
            tracing::info!(event = "dt_wait_consumed", task_id = owner.task_id.as_str(), wait_id = owner.id.as_str());
            Ok(true)
        }
        Ok(snapshot) => { update(connection, owner, probe, Some(snapshot), None).await?; Ok(false) }
    }
}

fn actionable(snapshot: &Snapshot, owner: &Wait) -> bool {
    snapshot.open && snapshot.head == owner.expected_head && snapshot.checks.iter().any(|check| check.status == "completed"
        && !owner.checks.iter().any(|prior| prior == check))
}

async fn update(connection: &mut PgConnection, owner: &Wait, probe: &Probe, snapshot: Option<&Snapshot>, cooldown: Option<jiff::Timestamp>) -> Result<()> {
    let checks = snapshot.map_or(owner.checks.0.as_slice(), |snapshot| snapshot.checks.as_slice());
    let digest = match snapshot { Some(snapshot) => snapshot.digest()?, None => owner.baseline_digest.clone() };
    let delay = match owner.fallback_count { 0 => 60, 1 => 120, 2 => 240, _ => 300 };
    sqlx::query(include_str!("sql/update.sql")).bind(&owner.id).bind(probe.claim.as_str()).bind(Json(checks)).bind(digest)
        // Consume only the signal visible when this claim started. A racing signal remains due.
        .bind(probe.owner.signalled_at).bind(cooldown.unwrap_or_else(clock::now).to_sqlx()).bind(Duration::from_secs(delay)).bind(probe.owner.signal_sequence)
        .execute(connection).await.map_err(query("retain unchanged dependency without allocating agent work"))?;
    Ok(())
}
