//! Identities and ordinary turn preparation precede native lifecycle locks.

use super::super::{
    DurableTaskBinding, DurableTaskScope, DurableTasks, PreparedItem, PreparedPlan,
    continuation::Continuation,
};
use crate::Result;
use af_core::clock;

pub(super) struct Wake {
    pub generation: u64,
    pub continuation: Continuation,
}

pub(super) async fn identities(
    tasks: &DurableTasks,
    scope: &DurableTaskScope,
    plan: &PreparedPlan,
) -> Result<(Vec<DurableTaskBinding>, Vec<Option<Wake>>)> {
    let now = clock::now();
    let mut identities = Vec::with_capacity(plan.items.len());
    let mut wakes = Vec::with_capacity(plan.items.len());
    for item in &plan.items {
        let item_id = tasks.runs.entropy().uuid7(now)?;
        let (durable_task_id, wake) = match item {
            PreparedItem::New { .. } => {
                let task = tasks.runs.entropy().uuid7(now)?;
                let continuation = Continuation::new(scope, &task, 1, &plan.source_turn_id);
                (
                    Some(task),
                    Some(Wake {
                        generation: 0,
                        continuation,
                    }),
                )
            }
            PreparedItem::Existing {
                durable_task_id, ..
            } => {
                let duplicate = identities.iter().any(|prior: &DurableTaskBinding| {
                    prior.durable_task_id.as_ref() == durable_task_id.as_ref()
                });
                let wake = if duplicate {
                    None
                } else {
                    super::input::wake(tasks, scope, item).await?
                };
                (durable_task_id.clone(), wake)
            }
        };
        identities.push(DurableTaskBinding {
            item_id,
            durable_task_id,
            durable_task_generation: None,
            action: item.action(),
            applies_here: false,
            input_sequence: None,
        });
        wakes.push(wake);
    }
    Ok((identities, wakes))
}
