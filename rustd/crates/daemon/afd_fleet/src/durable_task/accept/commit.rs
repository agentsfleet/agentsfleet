//! Ownership, the complete binding and its immutable receipt share one transaction.

use super::super::{
    DurableTaskBinding, DurableTaskBindings, DurableTaskRun, DurableTaskScope, DurableTasks,
    PreparedPlan, guard, transaction,
};
use super::{
    CONTEXT, plan as source_plan,
    prepare::Wake,
    replay, source,
    write::{Writer, Written},
};
use crate::{Result, error::query};
use afd_turns::PreparedTurn;

pub(super) struct Committed {
    pub bindings: DurableTaskBindings,
    pub queued: Vec<usize>,
}

pub(super) async fn commit(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    plan: &PreparedPlan,
    scope: &DurableTaskScope,
    mut identities: Vec<DurableTaskBinding>,
    continuations: &[Option<Wake>],
    prepared: Vec<Option<PreparedTurn<'_>>>,
) -> Result<Committed> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::run(&mut tx, run).await?;
    let source = source::lock(&mut tx, run, &current, plan).await?;
    if let Some(bindings) = replay::read(&mut tx, &current, plan).await? {
        tx.commit().await.map_err(query(CONTEXT))?;
        transaction::recycle(connection);
        return Ok(Committed {
            bindings,
            queued: Vec::new(),
        });
    }
    source::check(&source, plan)?;
    source_plan::write(&mut tx, &current, plan, &identities).await?;
    let Written { bound, queued } = Writer {
        tasks,
        current: &current,
        source: &source,
        plan,
    }
    .apply(&mut tx, &mut identities, continuations, prepared)
    .await?;
    let current_durable_task = match bound {
        Some(id) => Some(super::super::read::on(&mut tx, scope, &id).await?.context),
        None => None,
    };
    let execution_deadline = crate::run::deadline::read(&mut tx, run.run_id).await?;
    tx.commit().await.map_err(query(CONTEXT))?;
    transaction::recycle(connection);
    Ok(Committed {
        bindings: DurableTaskBindings {
            items: identities,
            current_durable_task,
            execution_deadline,
            replayed: false,
        },
        queued,
    })
}
