//! Task ownership joins ordinary lease issuance; it never claims another slot.

use super::{TaskScope, read};
use crate::{Result, error::query, lease::envelope::Acquired};
use af_core::id::Uuid7;
use af_wire::task::{TaskContext, TaskState};
use sqlx::PgConnection;

// Setup changes and Kill hold FOR UPDATE. Recheck active state after that lock
// wait, before taking the host and slot, so a selected admission cannot survive Kill.
pub(crate) async fn parents(connection: &mut PgConnection, acquired: &Acquired) -> Result<bool> {
    let parent: Option<Uuid7> = sqlx::query_scalar(super::sql::LOCK_ISSUE_SCOPE)
        .bind(&acquired.fleet_id)
        .bind(&acquired.workspace_id)
        .bind(crate::lease::sql::FLEET_STATUS_ACTIVE)
        .fetch_optional(connection)
        .await
        .map_err(query("hold surviving issue parents before host and slot"))?;
    Ok(parent.is_some())
}

pub(crate) async fn report_parents(
    connection: &mut PgConnection,
    fleet: &Uuid7,
    workspace: &Uuid7,
) -> Result<()> {
    let _parent: Option<Uuid7> = sqlx::query_scalar(super::sql::LOCK_SCOPE)
        .bind(fleet)
        .bind(workspace)
        .fetch_optional(connection)
        .await
        .map_err(query("hold surviving report parents before host and slot"))?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct Selected {
    task_id: Option<Uuid7>,
    admission_generation: Option<i64>,
    accepted_revision_id: Option<Uuid7>,
    generation: Option<i64>,
    head_revision_id: Option<Uuid7>,
    capable: bool,
    requires_tasks: bool,
}

/// Slot and lease are locked by the ordinary insert before the task lock.
/// A failed proof rolls that entire issue transaction back, including its tally.
pub(crate) async fn attach(
    connection: &mut PgConnection,
    acquired: &Acquired,
    runner: &Uuid7,
    lease: &Uuid7,
    revision: Option<&crate::lease::installed::IssueRevision>,
) -> Result<Option<(Option<TaskContext<'static>>, jiff::Timestamp)>> {
    let selected: Option<Selected> = sqlx::query_as(include_str!("sql/issue_source.sql"))
        .bind(&acquired.event_id)
        .bind(&acquired.fleet_id)
        .bind(runner)
        .bind(af_wire::task::CAPABILITY)
        .bind(super::DECISION_POLICY_PATH)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("read issued task source"))?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    if selected.requires_tasks && !selected.capable {
        return Ok(None);
    }
    if revision.is_some_and(|proof| {
        selected.head_revision_id.as_ref() != Some(&proof.current)
            || selected.accepted_revision_id.as_ref() != Some(&proof.accepted)
            || selected.generation != proof.task_generation
    }) {
        return Ok(None);
    }
    let Some(task_id) = selected.task_id else {
        return Ok(selected
            .admission_generation
            .is_none()
            .then_some((None, acquired.leased_until)));
    };
    if !selected.capable
        || revision.is_none()
        || selected.admission_generation != selected.generation
    {
        return Ok(None);
    }
    attach_task(connection, acquired, lease, &task_id, revision).await
}

async fn attach_task(
    connection: &mut PgConnection,
    acquired: &Acquired,
    lease: &Uuid7,
    task_id: &Uuid7,
    revision: Option<&crate::lease::installed::IssueRevision>,
) -> Result<Option<(Option<TaskContext<'static>>, jiff::Timestamp)>> {
    let task = task_id.clone();
    let _locked: Option<String> = sqlx::query_scalar(super::sql::LOCK_TASK)
        .bind(task_id)
        .bind(&acquired.fleet_id)
        .bind(&acquired.workspace_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("lock task for ordinary issue"))?;
    let deadline: Option<jiff_sqlx::Timestamp> =
        sqlx::query_scalar(include_str!("sql/issue_lease.sql"))
            .bind(lease)
            .bind(task_id)
            .bind(TaskState::Ready.as_str())
            .bind(TaskState::Running.as_str())
            .bind(revision.and_then(|proof| proof.task_generation))
            .bind(SLICE_SECONDS)
            .bind(NANOS_PER_SECOND)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query("attach task to ordinary lease"))?;
    let Some(deadline) = deadline else {
        return Ok(None);
    };
    let scope = TaskScope {
        fleet_id: acquired.fleet_id.clone(),
        workspace_id: acquired.workspace_id.clone(),
    };
    let context = read::on(connection, &scope, &task).await?.context;
    tracing::info!(
        event = "dt_execution_issued",
        task_id = task.as_str(),
        lease_id = lease.as_str(),
        generation = context.generation
    );
    Ok(Some((Some(context), deadline.to_jiff())))
}

pub(crate) const SLICE_SECONDS: i32 = 600;
const NANOS_PER_SECOND: i64 = 1_000_000_000;

impl crate::lease::Leases {
    /// A changed live setup blocks the old task instead of stranding a Ready owner.
    pub(crate) async fn block_changed_task_setup(&self, acquired: &Acquired) -> Result<()> {
        use sqlx::Connection as _;
        let mut connection = self.pool().acquire().await?;
        let mut tx = connection
            .begin()
            .await
            .map_err(query("begin changed task authority refusal"))?;
        if !parents(&mut tx, acquired).await? {
            return Ok(());
        }
        let _slot: Option<Uuid7> = sqlx::query_scalar(super::sql::LOCK_SCOPE_SLOT)
            .bind(&acquired.fleet_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(query("hold claimed slot before authority refusal"))?;
        match Self::installed_on(&mut tx, &acquired.fleet_id, Some(&acquired.event_id)).await {
            Err(fault) if fault.code() == af_core::error_code::TASK_INVALID_TARGET => (),
            Err(fault) => return Err(fault),
            Ok(_) => return Ok(()),
        }
        let changed: Option<(Uuid7, i64)> = sqlx::query_as(include_str!("sql/changed_setup.sql"))
            .bind(&acquired.event_id)
            .bind(&acquired.fleet_id)
            .bind(&acquired.workspace_id)
            .bind(acquired.fence.as_i64())
            .bind(TaskState::Ready.as_str())
            .bind(TaskState::Blocked.as_str())
            .bind("setup_authority_changed")
            .bind(TaskState::Running.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(query("block only the claimed current task generation"))?;
        if let Some((task, _)) = &changed {
            super::waits::cancel(&mut tx, task).await?;
            super::continuation::retire(&mut tx, task).await?;
        }
        tx.commit()
            .await
            .map_err(query("commit task setup authority refusal"))?;
        if let Some((task, version)) = changed {
            let frame = afd_api_wire::tail::TailFrame::TaskChanged {
                task_id: std::borrow::Cow::Borrowed(task.as_str()),
                resource_version: super::row::counter(version, "resource_version")?,
            };
            let subject = afd_bus::Subject::FleetTail(acquired.fleet_id.clone());
            let publication = self.streams().publish_frame(&subject, &frame).await;
            afd_bus::observe_notification(&subject, publication);
            tracing::info!(
                event = "dt_transition",
                task_id = task.as_str(),
                state = TaskState::Blocked.as_str(),
                reason = "setup_authority_changed"
            );
        }
        Ok(())
    }
}
