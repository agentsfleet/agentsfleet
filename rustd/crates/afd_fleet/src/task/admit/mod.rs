//! A complete interpretation and all its task bindings commit together.

mod insert;
mod replay;
pub(in crate::task) mod source;

use afd_core::clock;
use sqlx::Acquire as _;

use super::{
    PreparedPlan, TaskBinding, TaskBindings, TaskLease, Tasks, continuation::Continuation, guard,
};
use crate::error::{Result, query};

const CONTEXT: &str = "bind durable interpretation";

pub(super) async fn bind(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    plan: &PreparedPlan,
) -> Result<TaskBindings> {
    let scope = guard::lease_scope(tasks.leases.pool(), lease).await?;
    let now = clock::now();
    let mut identities = Vec::with_capacity(plan.items.len());
    let mut continuations = Vec::with_capacity(plan.items.len());
    for _item in &plan.items {
        let task_id = tasks.leases.entropy().uuid7(now)?;
        let item_id = tasks.leases.entropy().uuid7(now)?;
        continuations.push(Continuation::new(
            &scope,
            &task_id,
            1,
            &plan.source_admission_id,
        ));
        identities.push(TaskBinding { item_id, task_id });
    }
    // Backlog sampling precedes locks; staging below uses the caller's transaction.
    let mut prepared = Vec::with_capacity(continuations.len());
    for continuation in &continuations {
        prepared.push(tasks.admissions.prepare(continuation.admission()).await?);
    }
    let mut connection = tasks.leases.pool().acquire().await?;
    let mut transaction = connection.begin().await.map_err(query(CONTEXT))?;
    let current = guard::lease(&mut transaction, lease).await?;
    let source = source::lock(&mut transaction, lease, &current, plan).await?;
    if let Some(bindings) = replay::read(&mut transaction, &current, plan).await? {
        transaction.commit().await.map_err(query(CONTEXT))?;
        return Ok(bindings);
    }
    source::check(&source, &current, plan)?;
    let mut queued = Vec::new();
    for (ordinal, ((item, binding), continuation)) in
        plan.items.iter().zip(&identities).zip(prepared).enumerate()
    {
        let active = ordinal == 0 && current.task_id.is_none();
        insert::item(&mut transaction, &current, plan, item, binding, ordinal).await?;
        insert::task(
            &mut transaction,
            &current,
            plan,
            item,
            binding,
            &source,
            active,
        )
        .await?;
        if active {
            insert::lease(&mut transaction, lease, binding).await?;
            insert::admission(&mut transaction, plan.source_admission_id.as_str(), binding).await?;
        } else {
            let admitted = tasks
                .admissions
                .stage(&mut transaction, continuation)
                .await?;
            insert::admission(&mut transaction, &admitted.stored.id, binding).await?;
            queued.push(ordinal);
        }
    }
    transaction.commit().await.map_err(query(CONTEXT))?;
    for ordinal in queued {
        if let Some(continuation) = continuations.get(ordinal) {
            continuation.confirm(tasks).await;
        }
    }
    for binding in &identities {
        tasks.publish_changed(&scope, &binding.task_id, 1).await;
        tracing::info!(
            event = "dt_transition",
            task_id = binding.task_id.as_str(),
            lease_id = lease.lease_id.as_str(),
            reason = "admitted"
        );
    }
    Ok(TaskBindings {
        items: identities,
        replayed: false,
    })
}
