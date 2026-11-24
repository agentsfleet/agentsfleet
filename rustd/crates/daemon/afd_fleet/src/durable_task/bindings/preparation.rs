//! One binding item's checks and target reads, before the bind transaction.

use af_fleet_runtime::config::{Access, RepositoryBinding, RepositoryPublication};
use af_wire::durable_task::{DurableTaskBindingItem, DurableTaskRequestedTarget};

use super::super::{
    DurableTaskRefusal, DurableTaskRun, DurableTasks, PreparedDurableTask, PreparedItem, decision,
    guard,
};
use crate::Result;

/// `item` as the bind writes it: a new task whose objective is its span of
/// `message`, or a steer of the task it names.
///
/// # Errors
/// A span outside the message, statements the parser refuses, a target the
/// fleet's approved access does not reach, and a malformed task identifier.
pub(super) async fn prepare(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    item: &DurableTaskBindingItem,
    message: &str,
) -> Result<PreparedItem> {
    let (start, end) = item.span();
    let objective = span(message, start, end)?;
    match item {
        DurableTaskBindingItem::NewWork {
            title,
            target,
            statements,
            ..
        } => {
            let parsed = decision::compile::statements(statements)?;
            tasks
                .dependencies
                .policy
                .new_work(title, objective, &parsed)?;
            let questions = decision::compile::questions(&parsed)?;
            let binding = approved_binding(tasks, run).await?;
            let target = self::target(tasks, run, target, binding.as_ref(), objective).await?;
            let goal_digest = af_core::etag::sha256_hex(
                &serde_json::to_vec(&(objective, &questions))
                    .map_err(af_core::json::Unreadable::from)?,
            );
            Ok(PreparedItem::New {
                task: PreparedDurableTask {
                    source_span: start..end,
                    title: title.clone(),
                    objective: objective.to_owned(),
                    questions,
                    goal_digest,
                    target_json: target.to_string(),
                    limits: tasks.dependencies.policy.limits(),
                },
            })
        }
        DurableTaskBindingItem::Steer {
            durable_task_id, ..
        } => Ok(PreparedItem::Steer {
            source_span: start..end,
            durable_task_id: guard::id(durable_task_id, "durable_task_id")?,
        }),
    }
}

/// The part of `message` a span names; one that does not fall on its
/// characters is not the person's words.
fn span(message: &str, start: i32, end: i32) -> Result<&str> {
    let range = usize::try_from(start).ok().zip(usize::try_from(end).ok());
    range
        .and_then(|(start, end)| message.get(start..end))
        .ok_or_else(|| DurableTaskRefusal::ReplayConflict.into())
}

/// The GitHub scope a person approved for the run's fleet, as it stands now.
async fn approved_binding(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
) -> Result<Option<RepositoryBinding>> {
    let scope = guard::run_scope(tasks.runs.pool(), run).await?;
    let mut connection = tasks.runs.pool().acquire().await?;
    crate::run::Runs::approved_binding_on(&mut connection, &scope.fleet_id).await
}

async fn target(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    target: &DurableTaskRequestedTarget,
    binding: Option<&RepositoryBinding>,
    objective: &str,
) -> Result<serde_json::Value> {
    match target {
        DurableTaskRequestedTarget::ApiOnly => {
            // An approved existing-PR write scope reserves its repositories for
            // tasks that publish to them, so API-only work cannot hold it.
            if binding.is_some_and(|binding| {
                binding.publication() == RepositoryPublication::ExistingPr
                    && binding.access() == Access::Write
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
                    .prepare(run, binding, repository, *number, objective)
                    .await?,
            )
            .map_err(|source| af_core::json::Unreadable::from(source).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::span;

    #[test]
    fn a_span_is_taken_only_on_the_messages_characters() {
        let message = "fix the cart — until PR 88 is green";

        assert_eq!(span(message, 0, 12).ok(), Some("fix the cart"));
        for (start, end) in [(0, 200), (13, 14), (-1, 3), (5, 2)] {
            assert!(span(message, start, end).is_err(), "{start}..{end}");
        }
    }
}
