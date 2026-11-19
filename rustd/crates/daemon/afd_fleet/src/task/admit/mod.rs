//! A complete interpretation and all its task bindings commit together.

mod commit;
mod input;
mod insert;
mod prepare;
mod replay;
pub(in crate::task) mod source;
mod write;

use sqlx::types::Json;

use super::{PreparedPlan, TaskBindings, TaskLease, Tasks, decision, guard, transaction};
use crate::error::{Result, query};

const CONTEXT: &str = "bind durable interpretation";

/// [`bind`] under the operation's trace and outcome label: the lifecycle's
/// test-only entry point, kept beside what it wraps.
#[cfg(any(test, feature = "test-util"))]
pub(super) fn observed_bind<'a>(
    tasks: &'a Tasks,
    lease: TaskLease<'a>,
    plan: &'a PreparedPlan,
) -> super::TaskFuture<'a, TaskBindings> {
    use af_observability::metrics::label::task::{TaskOperation, TaskOutcome};

    let span = tracing::info_span!("task_bind", lease_id = lease.lease_id.as_str());
    super::observe::traced(
        TaskOperation::Bind,
        bind(tasks, lease, plan),
        |bindings| {
            if bindings.replayed {
                TaskOutcome::Replayed
            } else {
                TaskOutcome::Accepted
            }
        },
        span,
    )
}

pub(super) async fn bind(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    plan: &PreparedPlan,
) -> Result<TaskBindings> {
    if let Some(accepted) = existing(tasks, lease, plan).await? {
        return Ok(accepted);
    }
    let scope = guard::lease_scope(tasks.leases.pool(), lease).await?;
    let (identities, continuations) = prepare::identities(tasks, &scope, plan).await?;
    let prepared = prepare_admissions(tasks, &continuations).await?;
    let committed = commit::commit(
        tasks,
        lease,
        plan,
        &scope,
        identities,
        &continuations,
        prepared,
    )
    .await?;
    if !committed.bindings.replayed {
        for ordinal in committed.queued {
            if let Some(Some(wake)) = continuations.get(ordinal) {
                wake.continuation.confirm(tasks).await;
            }
        }
        announce(tasks, lease, &scope, &committed.bindings.items).await?;
    }
    Ok(committed.bindings)
}

async fn prepare_admissions<'a>(
    tasks: &Tasks,
    continuations: &'a [Option<prepare::Wake>],
) -> Result<Vec<Option<afd_admission::PreparedAdmission<'a>>>> {
    // Budget sampling precedes locks; staging uses the caller's transaction.
    let mut prepared = Vec::with_capacity(continuations.len());
    for wake in continuations {
        prepared.push(match wake {
            Some(wake) => Some(
                tasks
                    .admissions
                    .prepare(wake.continuation.admission())
                    .await?,
            ),
            None => None,
        });
    }
    Ok(prepared)
}

pub(super) async fn existing(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    plan: &PreparedPlan,
) -> Result<Option<TaskBindings>> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::replay_lease(&mut tx, lease).await?;
    let saved = replay::read(&mut tx, &current, plan).await?;
    tx.commit().await.map_err(query(CONTEXT))?;
    transaction::recycle(connection);
    Ok(saved)
}

async fn authorize(
    connection: &mut sqlx::PgConnection,
    lease: TaskLease<'_>,
    current: &guard::LeaseRow,
    plan: &PreparedPlan,
    source: &source::Source,
) -> Result<()> {
    if let Some(authorization) = &plan.authorization {
        decision::authorize::binding(connection, lease, authorization).await?;
        sqlx::query(include_str!("sql/interpretation.sql"))
            .bind(&plan.source_admission_id)
            .bind(&current.fleet_id)
            .bind(&current.workspace_id)
            .bind(&current.tenant_id)
            .bind(&plan.digest)
            .bind(source.message()?.as_ref())
            .bind(Json(authorization))
            .bind(&source.actor)
            .bind(&source.producer)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
    }
    Ok(())
}

async fn announce(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    scope: &super::TaskScope,
    identities: &[super::TaskBinding],
) -> Result<()> {
    for binding in identities {
        let Some(task) = &binding.task_id else {
            continue;
        };
        let snapshot = super::read::read(tasks, scope, task).await?;
        tasks
            .publish_changed(scope, task, snapshot.resource_version)
            .await;
        tracing::info!(
            event = "dt_transition",
            task_id = task.as_str(),
            lease_id = lease.lease_id.as_str(),
            reason = "admitted"
        );
    }
    Ok(())
}

async fn mark_interpreted(
    connection: &mut sqlx::PgConnection,
    current: &guard::LeaseRow,
    plan: &PreparedPlan,
) -> Result<()> {
    if plan.authorization.is_some() {
        sqlx::query(include_str!("sql/interpreted.sql"))
            .bind(&plan.source_admission_id)
            .bind(&current.fleet_id)
            .bind(&current.workspace_id)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
    }
    Ok(())
}
