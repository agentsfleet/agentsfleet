//! Task ownership joins ordinary run issuance; it never takes another thread.

use super::{DurableTaskScope, read};
use crate::{
    Result,
    error::query,
    run::envelope::{Acquired, HeadKind},
};
use af_core::id::Uuid7;
use af_wire::durable_task::{DurableTaskContext, DurableTaskState};
use sqlx::PgConnection;

// Setup changes and Kill hold FOR UPDATE. Recheck active state after that lock
// wait, before taking the host and thread, so a selected turn cannot survive Kill.
pub(crate) async fn parents(connection: &mut PgConnection, acquired: &Acquired) -> Result<bool> {
    let parent: Option<Uuid7> = sqlx::query_scalar(super::sql::LOCK_ISSUE_SCOPE)
        .bind(&acquired.fleet_id)
        .bind(&acquired.workspace_id)
        .bind(crate::run::sql::FLEET_STATUS_ACTIVE)
        .fetch_optional(connection)
        .await
        .map_err(query("hold surviving issue parents before host and thread"))?;
    Ok(parent.is_some())
}

#[derive(sqlx::FromRow)]
struct Selected {
    durable_task_id: Option<Uuid7>,
    turn_generation: Option<i64>,
    accepted_revision_id: Option<Uuid7>,
    generation: Option<i64>,
    head_revision_id: Option<Uuid7>,
    capable: bool,
    requires_durable_tasks: bool,
}

/// Thread and run are locked by the ordinary insert before the task lock.
/// A failed proof rolls that entire issue transaction back, so nothing counts it acquired.
pub(crate) async fn attach(
    connection: &mut PgConnection,
    acquired: &Acquired,
    (runner, run): (&Uuid7, &Uuid7),
    revision: Option<&crate::run::installed::IssueRevision>,
    slice_seconds: i32,
) -> Result<Option<(Option<DurableTaskContext<'static>>, jiff::Timestamp)>> {
    // A plain turn under the assignment's setup proof: the run insert
    // rechecked the head revision and the host, so there is nothing to attach
    // and nothing left to read. Without a proof the read below is
    // the only setup check, so it stays.
    if revision.is_some() && acquired.opened.head == HeadKind::Plain {
        return Ok(Some((None, acquired.held_until)));
    }
    let selected: Option<Selected> = sqlx::query_as(include_str!("sql/issue_source.sql"))
        .bind(&acquired.turn_id)
        .bind(&acquired.fleet_id)
        .bind(runner)
        .bind(af_wire::durable_task::CAPABILITY)
        .bind(super::DECISION_POLICY_PATH)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("read issued task source"))?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    if selected.requires_durable_tasks && !selected.capable {
        return Ok(None);
    }
    if revision.is_some_and(|proof| {
        selected.head_revision_id.as_ref() != Some(&proof.current)
            || selected.accepted_revision_id.as_ref() != Some(&proof.accepted)
            || selected.generation != proof.durable_task_generation
    }) {
        return Ok(None);
    }
    let Some(durable_task_id) = selected.durable_task_id else {
        return Ok(selected
            .turn_generation
            .is_none()
            .then_some((None, acquired.held_until)));
    };
    if !selected.capable || revision.is_none() || selected.turn_generation != selected.generation {
        return Ok(None);
    }
    attach_durable_task(
        connection,
        acquired,
        run,
        (&durable_task_id, revision),
        slice_seconds,
    )
    .await
}

async fn attach_durable_task(
    connection: &mut PgConnection,
    acquired: &Acquired,
    run: &Uuid7,
    (durable_task_id, revision): (&Uuid7, Option<&crate::run::installed::IssueRevision>),
    slice_seconds: i32,
) -> Result<Option<(Option<DurableTaskContext<'static>>, jiff::Timestamp)>> {
    let task = durable_task_id.clone();
    let _locked: Option<String> = sqlx::query_scalar(super::sql::LOCK_DURABLE_TASK)
        .bind(durable_task_id)
        .bind(&acquired.fleet_id)
        .bind(&acquired.workspace_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("lock task for ordinary issue"))?;
    let deadline: Option<jiff_sqlx::Timestamp> =
        sqlx::query_scalar(include_str!("sql/issue_run.sql"))
            .bind(run)
            .bind(durable_task_id)
            .bind(DurableTaskState::Ready.as_str())
            .bind(DurableTaskState::Running.as_str())
            .bind(revision.and_then(|proof| proof.durable_task_generation))
            .bind(slice_seconds)
            .bind(NANOS_PER_SECOND)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query("attach task to ordinary run"))?;
    let Some(deadline) = deadline else {
        return Ok(None);
    };
    let scope = DurableTaskScope {
        fleet_id: acquired.fleet_id.clone(),
        workspace_id: acquired.workspace_id.clone(),
    };
    let context = read::on(connection, &scope, &task).await?.context;
    tracing::info!(
        event = "dt_execution_issued",
        durable_task_id = task.as_str(),
        run_id = run.as_str(),
        generation = context.generation
    );
    Ok(Some((Some(context), deadline.to_jiff())))
}

/// A task's slice length, by default.
pub(crate) const SLICE: std::time::Duration = std::time::Duration::from_mins(10);
const NANOS_PER_SECOND: i64 = 1_000_000_000;

impl crate::run::Runs {
    /// A changed live setup blocks the old task instead of stranding a Ready owner.
    pub(crate) async fn block_changed_durable_task_setup(&self, acquired: &Acquired) -> Result<()> {
        // Cancellation closes the socket, so a blocked statement holds no lock.
        let mut connection = super::transaction::acquire(self.pool()).await?;
        let mut tx = super::transaction::begin(&mut connection).await?;
        if !parents(&mut tx, acquired).await? {
            return Ok(());
        }
        let _thread: Option<Uuid7> = sqlx::query_scalar(super::sql::LOCK_SCOPE_THREAD)
            .bind(&acquired.fleet_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(query("hold assigned thread before authority refusal"))?;
        match Self::installed_on(
            self.definitions(),
            &mut tx,
            &acquired.fleet_id,
            Some(&acquired.turn_id),
        )
        .await
        {
            Err(fault) if fault.code() == af_core::error_code::DURABLE_TASK_INVALID_TARGET => (),
            Err(fault) => return Err(fault),
            Ok(_) => return Ok(()),
        }
        let changed: Option<(Uuid7, i64)> = sqlx::query_as(include_str!("sql/changed_setup.sql"))
            .bind(&acquired.turn_id)
            .bind(&acquired.fleet_id)
            .bind(&acquired.workspace_id)
            .bind(acquired.fence.as_i64())
            .bind(DurableTaskState::Ready.as_str())
            .bind(DurableTaskState::Blocked.as_str())
            .bind("setup_authority_changed")
            .bind(DurableTaskState::Running.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(query("block only the assigned current task generation"))?;
        if let Some((task, _)) = &changed {
            super::waits::cancel(&mut tx, task).await?;
            super::continuation::retire(&mut tx, task).await?;
        }
        tx.commit()
            .await
            .map_err(query("commit task setup authority refusal"))?;
        super::transaction::recycle(connection);
        if let Some((task, version)) = changed {
            let frame = afd_api_wire::tail::TailFrame::DurableTaskChanged {
                durable_task_id: std::borrow::Cow::Borrowed(task.as_str()),
                resource_version: super::row::counter(version, "resource_version")?,
            };
            let subject = afd_bus::Subject::ThreadItems(acquired.fleet_id.clone());
            let publication = self.streams().publish_frame(&subject, &frame).await;
            afd_bus::observe_notification(&subject, publication);
            tracing::info!(
                event = "dt_transition",
                durable_task_id = task.as_str(),
                state = DurableTaskState::Blocked.as_str(),
                reason = "setup_authority_changed"
            );
        }
        Ok(())
    }
}
