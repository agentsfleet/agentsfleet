//! A complete interpretation and all its task bindings commit together.

mod insert;
mod replay;
mod prepare;
mod input;
pub(in crate::task) mod source;

use sqlx::types::Json;

use super::{
    PreparedItem, PreparedPlan, TaskBinding, TaskBindings, TaskLease, Tasks, decision, guard, transaction,
};
use crate::error::{Result, query};

const CONTEXT: &str = "bind durable interpretation";

pub(super) async fn bind(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    plan: &PreparedPlan,
) -> Result<TaskBindings> {
    if let Some(accepted) = existing(tasks, lease, plan).await? { return Ok(accepted); }
    let scope = guard::lease_scope(tasks.leases.pool(), lease).await?;
    let (mut identities, continuations) = prepare::identities(tasks, &scope, plan).await?;
    // Backlog sampling precedes locks; staging below uses the caller's transaction.
    let mut prepared = Vec::with_capacity(continuations.len());
    for wake in &continuations {
        prepared.push(match wake { Some(wake) => Some(tasks.admissions.prepare(wake.continuation.admission()).await?), None => None });
    }
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::lease(&mut tx, lease).await?;
    let source = source::lock(&mut tx, lease, &current, plan).await?;
    if let Some(bindings) = replay::read(&mut tx, &current, plan).await? {
        tx.commit().await.map_err(query(CONTEXT))?;
        transaction::recycle(connection);
        return Ok(bindings);
    }
    source::check(&source, &current, plan)?;
    if let Some(authorization) = &plan.authorization {
        decision::authorize::binding(&mut tx, lease, authorization).await?;
        sqlx::query(include_str!("sql/interpretation.sql")).bind(plan.source_admission_id.as_str())
            .bind(&current.fleet_id).bind(&current.workspace_id).bind(&current.tenant_id).bind(&plan.digest)
            .bind(afd_wire::event::message_of(&source.request_json).as_ref()).bind(Json(authorization))
            .bind(lease.lease_id.as_str()).bind(current.fence).execute(&mut *tx).await.map_err(query(CONTEXT))?;
    }
    let mut queued = Vec::new();
    let mut bound = current.task_id.clone();
    for (ordinal, ((item, binding), continuation)) in
        plan.items.iter().zip(&mut identities).zip(prepared).enumerate()
    {
        insert::item(&mut tx, &current, plan, item, binding, ordinal).await?;
        let should_queue = match item {
            PreparedItem::New { task, .. } => {
                let active = bound.is_none();
                insert::task(&mut tx, &current, plan, task, binding, &source, active).await?;
                binding.applies_here = active;
                binding.task_generation = Some(1);
                if active {
                    insert::lease(&mut tx, lease, binding).await?;
                    insert::admission(&mut tx, plan.source_admission_id.as_str(), binding).await?;
                    bound = binding.task_id.as_ref().map(ToString::to_string);
                }
                !active
            }
            PreparedItem::Existing { .. } => input::accept(&mut tx, &current, lease, item, binding, &source,
                continuations.get(ordinal).and_then(Option::as_ref)).await?,
        };
        if should_queue {
            let admitted = tasks
                .admissions
                .stage(&mut tx, continuation.ok_or(super::TaskRefusal::StaleState)?)
                .await?;
            insert::admission(&mut tx, &admitted.stored.id, binding).await?;
            queued.push(ordinal);
        }
    }
    if plan.authorization.is_some() {
        sqlx::query(include_str!("sql/interpreted.sql")).bind(plan.source_admission_id.as_str())
            .bind(&current.fleet_id).bind(&current.workspace_id).execute(&mut *tx).await.map_err(query(CONTEXT))?;
    }
    let current_task = match bound {
        Some(id) => Some(super::read::on(&mut tx, &scope, &guard::id(&id, "task_id")?).await?.context),
        None => None,
    };
    tx.commit().await.map_err(query(CONTEXT))?;
    transaction::recycle(connection);
    for ordinal in queued {
        if let Some(Some(wake)) = continuations.get(ordinal) {
            wake.continuation.confirm(tasks).await;
        }
    }
    for binding in &identities {
        let Some(task) = &binding.task_id else { continue; };
        let snapshot = super::read::read(tasks, &scope, task).await?;
        tasks.publish_changed(&scope, task, snapshot.resource_version).await;
        tracing::info!(
            event = "dt_transition",
            task_id = task.as_str(),
            lease_id = lease.lease_id.as_str(),
            reason = "admitted"
        );
    }
    Ok(TaskBindings {
        items: identities,
        current_task,
        replayed: false,
    })
}

pub(super) async fn existing(tasks: &Tasks, lease: TaskLease<'_>, plan: &PreparedPlan) -> Result<Option<TaskBindings>> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::lease(&mut tx, lease).await?;
    let saved = replay::read(&mut tx, &current, plan).await?;
    tx.commit().await.map_err(query(CONTEXT))?;
    transaction::recycle(connection);
    Ok(saved)
}
