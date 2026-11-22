//! Policy checks precede these target reads; allocation follows complete validation.

use af_fleet_runtime::FleetConfig;
use af_wire::decision::NamedQuestion;
use af_wire::durable_task::{
    DurableTaskBindingsInput, DurableTaskInputAction, DurableTaskProposedItem,
    DurableTaskRequestedTarget,
};

use super::super::{
    DurableTaskRefusal, DurableTaskRun, DurableTasks, PreparedDurableTask, PreparedItem, guard,
};
use crate::Result;

pub(super) async fn prepare(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    input: &DurableTaskBindingsInput,
    objectives: Vec<&str>,
    config: &FleetConfig,
    questions: &[NamedQuestion],
) -> Result<Vec<PreparedItem>> {
    let mut items = Vec::with_capacity(input.items.len());
    for (item, objective) in input.items.iter().zip(objectives) {
        let proposed = &item.proposed;
        items.push(if proposed.action == DurableTaskInputAction::NewWork {
            new_durable_task(tasks, run, proposed, objective, config, questions).await?
        } else {
            PreparedItem::Existing {
                source_span: proposed.source_start..proposed.source_end,
                action: proposed.action,
                durable_task_id: proposed
                    .durable_task_id
                    .as_deref()
                    .map(|id| guard::id(id, "durable_task_id"))
                    .transpose()?,
                interpretation: proposed.interpretation.clone(),
            }
        });
    }
    Ok(items)
}

async fn new_durable_task(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    proposed: &DurableTaskProposedItem,
    objective: &str,
    config: &FleetConfig,
    questions: &[NamedQuestion],
) -> Result<PreparedItem> {
    let target = target(
        tasks,
        run,
        proposed
            .target
            .as_ref()
            .ok_or(DurableTaskRefusal::InvalidTarget)?,
        config,
        objective,
    )
    .await?;
    let goal_digest = af_core::etag::sha256_hex(
        &serde_json::to_vec(&(objective, questions)).map_err(af_core::json::Unreadable::from)?,
    );
    Ok(PreparedItem::New {
        task: PreparedDurableTask {
            source_span: proposed.source_start..proposed.source_end,
            title: proposed.title.clone(),
            objective: objective.to_owned(),
            questions: questions.to_vec(),
            goal_digest,
            target_json: target.to_string(),
            limits: tasks.dependencies.policy.limits(),
        },
        interpretation: proposed.interpretation.clone(),
    })
}

async fn target(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    target: &DurableTaskRequestedTarget,
    config: &FleetConfig,
    objective: &str,
) -> Result<serde_json::Value> {
    match target {
        DurableTaskRequestedTarget::ApiOnly => {
            if config.repository_binding().is_some_and(|binding| {
                binding.publication() == af_fleet_runtime::config::RepositoryPublication::ExistingPr
                    && binding.access() == af_fleet_runtime::config::Access::Write
            }) {
                return Err(DurableTaskRefusal::InvalidTarget.into());
            }
            Ok(serde_json::json!({"kind":"api_only"}))
        }
        DurableTaskRequestedTarget::ExistingPr { repository, number } => {
            tasks
                .dependencies
                .storage
                .as_ref()
                .ok_or(DurableTaskRefusal::StorageUnavailable)?
                .verify_create_only()
                .await?;
            let github = tasks
                .dependencies
                .github
                .as_ref()
                .ok_or(DurableTaskRefusal::InvalidTarget)?;
            serde_json::to_value(
                github
                    .prepare(run, config, repository, *number, objective)
                    .await?,
            )
            .map_err(|source| af_core::json::Unreadable::from(source).into())
        }
    }
}
