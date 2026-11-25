//! Write one plan through the already locked source and fleet thread.
//!
//! A binding a tool made never attaches its task to the calling run: a new
//! task is written `ready` with its continuation turn staged, and the
//! continuation is assigned as the task's first run, prepared before its
//! sandbox. The calling run stays an ordinary run.

use super::super::{DurableTaskBinding, DurableTasks, PreparedItem, PreparedPlan, guard::RunRow};
use super::{input, insert, prepare::Wake, source::Source};
use crate::Result;
use af_core::id::Uuid7;
use afd_turns::PreparedTurn;

pub(super) struct Writer<'a> {
    pub tasks: &'a DurableTasks,
    pub current: &'a RunRow,
    pub source: &'a Source,
    pub plan: &'a PreparedPlan,
}

pub(super) struct Written {
    pub bound: Option<Uuid7>,
    pub queued: Vec<usize>,
}

impl Writer<'_> {
    pub(super) async fn apply(
        &self,
        connection: &mut sqlx::PgConnection,
        identities: &mut [DurableTaskBinding],
        continuations: &[Option<Wake>],
        prepared: Vec<Option<PreparedTurn<'_>>>,
    ) -> Result<Written> {
        let mut queued = Vec::new();
        for (ordinal, ((item, binding), continuation)) in self
            .plan
            .items
            .iter()
            .zip(identities.iter_mut())
            .zip(prepared)
            .enumerate()
        {
            insert::item(connection, self.current, self.plan, item, binding, ordinal).await?;
            let should_queue = match item {
                PreparedItem::New { task } => {
                    self.new_durable_task(connection, task, binding).await?;
                    true
                }
                PreparedItem::Steer { .. } => {
                    input::accept(
                        connection,
                        self.current,
                        item,
                        binding,
                        self.source,
                        continuations.get(ordinal).and_then(Option::as_ref),
                    )
                    .await?
                }
            };
            if should_queue {
                let admitted = self
                    .tasks
                    .turns
                    .stage(
                        connection,
                        continuation.ok_or(super::super::DurableTaskRefusal::StaleState)?,
                    )
                    .await?;
                insert::bind_turn(connection, &admitted.stored.id, binding).await?;
                queued.push(ordinal);
            }
        }
        Ok(Written {
            bound: self.current.durable_task_id.clone(),
            queued,
        })
    }

    /// Writes a new task `ready`, its first run left to the staged
    /// continuation.
    async fn new_durable_task(
        &self,
        connection: &mut sqlx::PgConnection,
        task: &super::super::PreparedDurableTask,
        binding: &mut DurableTaskBinding,
    ) -> Result<()> {
        insert::task(
            connection,
            self.current,
            task,
            binding,
            insert::Acceptance {
                source: self.source,
                unfinished_max: self.tasks.dependencies.policy.unfinished(),
            },
        )
        .await?;
        binding.applies_here = false;
        binding.durable_task_generation = Some(1);
        Ok(())
    }
}
