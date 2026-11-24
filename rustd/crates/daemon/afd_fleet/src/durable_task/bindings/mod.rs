//! A run's task tool binds one span of a person's message to durable work.
//!
//! The fleet's own model calls `task_start` or `task_input`, and the verb
//! takes it at its word: no decision call authorizes the binding. What the
//! daemon proves is whose words they are — the source turn is the run's head
//! or folded into it, and a person's message — that the span lies inside it,
//! and, for new work, that its target and statements hold. Remote target reads
//! run outside the bind transaction, which then commits through `accept`.

use super::{
    DurableTaskRefusal, DurableTaskRun, DurableTasks, PreparedPlan, accept, guard, transaction,
};
use crate::Result;
use af_wire::durable_task::{
    DurableTaskBindingsInput, DurableTaskBindingsResponse, DurableTaskItemBinding,
};

mod preparation;

pub(super) async fn accept(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    input: &garde::Valid<DurableTaskBindingsInput>,
) -> Result<DurableTaskBindingsResponse<'static>> {
    let digest = af_core::etag::sha256_hex(
        &serde_json::to_vec(&input.interpretation()).map_err(af_core::json::Unreadable::from)?,
    );
    if digest != input.plan_digest {
        return Err(DurableTaskRefusal::ReplayConflict.into());
    }
    tasks.dependencies.policy.items(&input.items)?;
    let mut plan = PreparedPlan {
        source_turn_id: guard::id(&input.source_turn_id, "source_turn_id")?,
        digest,
        items: Vec::new(),
    };
    if let Some(accepted) = accept::existing(tasks, run, &plan).await? {
        return Ok(response(accepted));
    }
    let message = source(tasks, run, &plan).await?;
    for item in &input.items {
        plan.items
            .push(preparation::prepare(tasks, run, item, &message).await?);
    }
    Ok(response(accept::bind(tasks, run, &plan).await?))
}

/// The source turn's message, once the turn is proved the run's and a
/// person's. Read and released before any remote target read.
async fn source(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    plan: &PreparedPlan,
) -> Result<String> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::run(&mut tx, run).await?;
    let source = accept::source::lock(&mut tx, run, &current, plan).await?;
    accept::source::check_human(&source)?;
    let message = source.message()?.into_owned();
    tx.commit()
        .await
        .map_err(crate::error::query("finish task binding preflight"))?;
    transaction::recycle(connection);
    Ok(message)
}

fn response(accepted: super::DurableTaskBindings) -> DurableTaskBindingsResponse<'static> {
    DurableTaskBindingsResponse {
        replayed: accepted.replayed,
        execution_deadline: accepted.execution_deadline,
        current_durable_task: accepted.current_durable_task,
        items: accepted
            .items
            .into_iter()
            .map(|item| DurableTaskItemBinding {
                item_id: item.item_id.to_string(),
                durable_task_id: item.durable_task_id.map(|task| task.to_string()),
                durable_task_generation: item.durable_task_generation,
                action: item.action,
                applies_here: item.applies_here,
                input_sequence: item.input_sequence,
            })
            .collect(),
    }
}
