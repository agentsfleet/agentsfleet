//! Production interpretation prepares remote targets outside the native bind transaction.

use super::{
    PreparedItem, PreparedPlan, PreparedTask, TaskLease, TaskRefusal, Tasks, admit, decision,
    guard, transaction,
};
use crate::{Result, lease::Leases};
use afd_wire::task::{
    TaskBindingsInput, TaskBindingsResponse, TaskInputAction, TaskItemBinding, TaskRequestedTarget,
};

pub(super) async fn accept(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    input: &garde::Valid<TaskBindingsInput>,
) -> Result<TaskBindingsResponse<'static>> {
    let digest = afd_core::etag::sha256_hex(
        &serde_json::to_vec(&input.interpretation()).map_err(afd_core::json::Unreadable::from)?,
    );
    if digest != input.plan_digest {
        return Err(TaskRefusal::ReplayConflict.into());
    }
    let mut plan = PreparedPlan {
        source_admission_id: guard::id(&input.source_admission_id, "source_admission_id")?,
        accepted_revision_id: guard::id(&input.accepted_revision_id, "accepted_revision_id")?,
        digest,
        items: Vec::new(),
        authorization: Some((**input).clone()),
    };
    if let Some(accepted) = admit::existing(tasks, lease, &plan).await? {
        return Ok(response(accepted));
    }
    let (config, message) = source(tasks, lease, &plan).await?;
    let questions = decision::compile::completion(
        config
            .decision_policy()
            .ok_or(TaskRefusal::DecisionUnavailable)?,
    )?;
    let mut end = 0_i32;
    for item in &input.items {
        let proposed = &item.proposed;
        if proposed.source_start < end || proposed.source_end <= proposed.source_start {
            return Err(TaskRefusal::InputAmbiguous.into());
        }
        let span = proposed.source_start.unsigned_abs() as usize
            ..proposed.source_end.unsigned_abs() as usize;
        let objective = message
            .get(span)
            .ok_or(TaskRefusal::InputAmbiguous)?
            .to_owned();
        end = proposed.source_end;
        if proposed.action == TaskInputAction::NewWork {
            let target = match proposed.target.as_ref().ok_or(TaskRefusal::InvalidTarget)? {
                TaskRequestedTarget::ApiOnly => {
                    if config.repository_binding().is_some_and(|binding| {
                        binding.publication()
                            == afd_fleet_runtime::config::RepositoryPublication::ExistingPr
                            && binding.access() == afd_fleet_runtime::config::Access::Write
                    }) {
                        return Err(TaskRefusal::InvalidTarget.into());
                    }
                    serde_json::json!({"kind":"api_only"})
                }
                TaskRequestedTarget::ExistingPr { repository, number } => {
                    tasks
                        .dependencies
                        .storage
                        .as_ref()
                        .ok_or(TaskRefusal::StorageUnavailable)?
                        .verify_create_only()
                        .await?;
                    let github = tasks
                        .dependencies
                        .github
                        .as_ref()
                        .ok_or(TaskRefusal::InvalidTarget)?;
                    serde_json::to_value(
                        github
                            .prepare(lease, &config, repository, *number, &objective)
                            .await?,
                    )
                    .map_err(afd_core::json::Unreadable::from)?
                }
            };
            let goal_digest = afd_core::etag::sha256_hex(
                &serde_json::to_vec(&(&objective, &questions))
                    .map_err(afd_core::json::Unreadable::from)?,
            );
            plan.items.push(PreparedItem::New {
                task: PreparedTask {
                    source_span: proposed.source_start..proposed.source_end,
                    title: proposed.title.clone(),
                    objective,
                    questions: questions.clone(),
                    goal_digest,
                    target_json: target.to_string(),
                    limits: afd_wire::task::TaskLimits::default(),
                },
                interpretation: proposed.interpretation.clone(),
            });
        } else {
            plan.items.push(PreparedItem::Existing {
                source_span: proposed.source_start..proposed.source_end,
                action: proposed.action,
                task_id: proposed
                    .task_id
                    .as_deref()
                    .map(|id| guard::id(id, "task_id"))
                    .transpose()?,
                interpretation: proposed.interpretation.clone(),
            });
        }
    }
    Ok(response(admit::bind(tasks, lease, &plan).await?))
}

async fn source(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    plan: &PreparedPlan,
) -> Result<(afd_fleet_runtime::FleetConfig, String)> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::lease(&mut tx, lease).await?;
    let source = admit::source::lock(&mut tx, lease, &current, plan).await?;
    admit::source::check_human(&source)?;
    if current.revision_id.as_deref() != Some(plan.accepted_revision_id.as_str()) {
        return Err(TaskRefusal::StaleState.into());
    }
    let config = Leases::stored_config_on(
        &mut tx,
        &current.fleet_id,
        Some(plan.accepted_revision_id.as_str()),
    )
    .await?
    .ok_or(TaskRefusal::InvalidTarget)?;
    let message = source.message()?.into_owned();
    tx.commit().await.map_err(crate::error::query(
        "finish native interpretation preflight",
    ))?;
    transaction::recycle(connection);
    Ok((config, message))
}

fn response(accepted: super::TaskBindings) -> TaskBindingsResponse<'static> {
    TaskBindingsResponse {
        replayed: accepted.replayed,
        execution_deadline: Some(accepted.execution_deadline),
        current_task: accepted.current_task,
        items: accepted
            .items
            .into_iter()
            .map(|item| TaskItemBinding {
                item_id: item.item_id.to_string(),
                task_id: item.task_id.map(|task| task.to_string()),
                task_generation: item.task_generation,
                action: item.action,
                applies_here: item.applies_here,
                input_sequence: item.input_sequence,
            })
            .collect(),
    }
}
