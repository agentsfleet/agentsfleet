//! A source turn's plan and all its task bindings commit together.

mod commit;
mod input;
mod insert;
mod plan;
mod prepare;
mod replay;
pub(in crate::durable_task) mod source;
mod write;

use super::{DurableTaskBindings, DurableTaskRun, DurableTasks, PreparedPlan, guard, transaction};
use crate::error::{Result, query};

const CONTEXT: &str = "bind a task plan";

/// What an accepted input stores as its interpretation: a tool sends none, so
/// the accepted text is the input's whole meaning.
pub(in crate::durable_task) const NO_INTERPRETATION: &str = "";

/// [`bind`] under the operation's trace and outcome label: the lifecycle's
/// test-only entry point, kept beside what it wraps.
#[cfg(any(test, feature = "test-util"))]
pub(super) fn observed_bind<'a>(
    tasks: &'a DurableTasks,
    run: DurableTaskRun<'a>,
    plan: &'a PreparedPlan,
) -> super::DurableTaskFuture<'a, DurableTaskBindings> {
    use af_observability::metrics::label::durable_task::{
        DurableTaskOperation, DurableTaskOutcome,
    };

    let span = tracing::info_span!("task_bind", run_id = run.run_id.as_str());
    super::observe::traced(
        DurableTaskOperation::Bind,
        bind(tasks, run, plan),
        |bindings| {
            if bindings.replayed {
                DurableTaskOutcome::Replayed
            } else {
                DurableTaskOutcome::Accepted
            }
        },
        span,
    )
}

pub(super) async fn bind(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    plan: &PreparedPlan,
) -> Result<DurableTaskBindings> {
    if let Some(accepted) = existing(tasks, run, plan).await? {
        return Ok(accepted);
    }
    let scope = guard::run_scope(tasks.runs.pool(), run).await?;
    let (identities, continuations) = prepare::identities(tasks, &scope, plan).await?;
    let prepared = prepare_turns(tasks, &continuations).await?;
    let committed = commit::commit(
        tasks,
        run,
        plan,
        &scope,
        identities,
        &continuations,
        prepared,
    )
    .await?;
    if !committed.bindings.replayed {
        // Only an item that carried a wake queues a turn, so each queued
        // ordinal names one.
        let queued = committed.queued.iter();
        for wake in queued.filter_map(|ordinal| continuations.get(*ordinal)?.as_ref()) {
            wake.continuation.confirm(tasks).await;
        }
        announce(tasks, run, &scope, &committed.bindings.items).await?;
    }
    Ok(committed.bindings)
}

async fn prepare_turns<'a>(
    tasks: &DurableTasks,
    continuations: &'a [Option<prepare::Wake>],
) -> Result<Vec<Option<afd_turns::PreparedTurn<'a>>>> {
    // Budget sampling precedes locks; staging uses the caller's transaction.
    let mut prepared = Vec::with_capacity(continuations.len());
    for wake in continuations {
        prepared.push(match wake {
            Some(wake) => Some(tasks.turns.prepare(wake.continuation.turn()).await?),
            None => None,
        });
    }
    Ok(prepared)
}

pub(super) async fn existing(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    plan: &PreparedPlan,
) -> Result<Option<DurableTaskBindings>> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::replay_run(&mut tx, run).await?;
    let saved = replay::read(&mut tx, &current, plan).await?;
    tx.commit().await.map_err(query(CONTEXT))?;
    transaction::recycle(connection);
    Ok(saved)
}

async fn announce(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    scope: &super::DurableTaskScope,
    identities: &[super::DurableTaskBinding],
) -> Result<()> {
    for binding in identities {
        let Some(task) = &binding.durable_task_id else {
            continue;
        };
        let snapshot = super::read::read(tasks, scope, task).await?;
        tasks
            .publish_changed(scope, task, snapshot.resource_version)
            .await;
        tracing::info!(
            event = "dt_transition",
            durable_task_id = task.as_str(),
            run_id = run.run_id.as_str(),
            reason = "admitted"
        );
    }
    Ok(())
}
