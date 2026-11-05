//! Identities and ordinary admission preparation precede native lifecycle locks.

use afd_core::clock;
use super::super::{PreparedItem, PreparedPlan, TaskBinding, TaskScope, Tasks, continuation::Continuation};
use crate::Result;

pub(super) struct Wake {
    pub generation: u64,
    pub continuation: Continuation,
}

pub(super) async fn identities(tasks: &Tasks, scope: &TaskScope, plan: &PreparedPlan) -> Result<(Vec<TaskBinding>, Vec<Option<Wake>>)> {
    let now = clock::now();
    let mut identities = Vec::with_capacity(plan.items.len());
    let mut wakes = Vec::with_capacity(plan.items.len());
    for item in &plan.items {
        let item_id = tasks.leases.entropy().uuid7(now)?;
        let (task_id, wake) = match item {
            PreparedItem::New { .. } => {
                let task = tasks.leases.entropy().uuid7(now)?;
                let continuation = Continuation::new(scope, &task, 1, &plan.source_admission_id);
                (Some(task), Some(Wake { generation: 0, continuation }))
            }
            PreparedItem::Existing { task_id, .. } => {
                let duplicate = identities.iter().any(|prior: &TaskBinding| prior.task_id.as_ref() == task_id.as_ref());
                let wake = if duplicate { None } else { super::input::wake(tasks, scope, item).await? };
                (task_id.clone(), wake)
            }
        };
        identities.push(TaskBinding { item_id, task_id, task_generation: None, action: item.action(), applies_here: false, input_sequence: None });
        wakes.push(wake);
    }
    Ok((identities, wakes))
}
