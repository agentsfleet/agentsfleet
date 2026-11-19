//! Ownership, the complete binding and its immutable receipt share one transaction.

use super::super::{
    PreparedPlan, TaskBinding, TaskBindings, TaskRun, TaskScope, Tasks, guard, transaction,
};
use super::{
    CONTEXT, authorize, mark_interpreted,
    prepare::Wake,
    replay, source,
    write::{Writer, Written},
};
use crate::{Result, error::query};
use afd_turns::PreparedTurn;

pub(super) struct Committed {
    pub bindings: TaskBindings,
    pub queued: Vec<usize>,
}

pub(super) async fn commit(
    tasks: &Tasks,
    run: TaskRun<'_>,
    plan: &PreparedPlan,
    scope: &TaskScope,
    mut identities: Vec<TaskBinding>,
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
    source::check(&source, &current, plan)?;
    authorize(&mut tx, run, &current, plan, &source).await?;
    let Written { bound, queued } = Writer {
        tasks,
        current: &current,
        source: &source,
        plan,
        run,
    }
    .apply(&mut tx, &mut identities, continuations, prepared)
    .await?;
    mark_interpreted(&mut tx, &current, plan).await?;
    let current_task = match bound {
        Some(id) => Some(super::super::read::on(&mut tx, scope, &id).await?.context),
        None => None,
    };
    let execution_deadline = crate::run::deadline::read(&mut tx, run.run_id).await?;
    tx.commit().await.map_err(query(CONTEXT))?;
    transaction::recycle(connection);
    Ok(Committed {
        bindings: TaskBindings {
            items: identities,
            current_task,
            execution_deadline,
            replayed: false,
        },
        queued,
    })
}
