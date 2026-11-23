//! Production interpretation prepares remote targets outside the native bind transaction.

use super::{
    DurableTaskRefusal, DurableTaskRun, DurableTasks, PreparedPlan, accept, decision, guard,
    transaction,
};
use crate::{Result, run::Runs};
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
    let mut plan = PreparedPlan {
        source_turn_id: guard::id(&input.source_turn_id, "source_turn_id")?,
        accepted_revision_id: guard::id(&input.accepted_revision_id, "accepted_revision_id")?,
        digest,
        items: Vec::new(),
        authorization: Some((**input).clone()),
    };
    if let Some(accepted) = accept::existing(tasks, run, &plan).await? {
        return Ok(response(accepted));
    }
    let (config, message) = source(tasks, run, &plan).await?;
    let policy = config
        .decision_policy()
        .ok_or(DurableTaskRefusal::DecisionUnavailable)?;
    let objectives = tasks
        .dependencies
        .policy
        .interpretation(input, &message, policy)?;
    let questions = decision::compile::completion(policy)?;
    plan.items = preparation::prepare(tasks, run, input, objectives, &config, &questions).await?;
    Ok(response(accept::bind(tasks, run, &plan).await?))
}

async fn source(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    plan: &PreparedPlan,
) -> Result<(af_fleet_runtime::FleetConfig, String)> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::run(&mut tx, run).await?;
    let source = accept::source::lock(&mut tx, run, &current, plan).await?;
    accept::source::check_human(&source)?;
    if current.revision_id.as_ref() != Some(&plan.accepted_revision_id) {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    let config =
        Runs::stored_config_on(&mut tx, &current.fleet_id, Some(&plan.accepted_revision_id))
            .await?
            .ok_or(DurableTaskRefusal::InvalidTarget)?;
    let message = source.message()?.into_owned();
    tx.commit().await.map_err(crate::error::query(
        "finish native interpretation preflight",
    ))?;
    transaction::recycle(connection);
    Ok((config, message))
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
