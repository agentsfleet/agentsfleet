//! Write one complete interpretation through the already locked source and fleet thread.

use super::super::{
    DurableTaskBinding, DurableTaskRun, DurableTasks, PreparedItem, PreparedPlan, guard::RunRow,
};
use super::{input, insert, prepare::Wake, source::Source};
use crate::Result;
use af_core::id::Uuid7;
use afd_turns::PreparedTurn;

pub(super) struct Writer<'a> {
    pub tasks: &'a DurableTasks,
    pub current: &'a RunRow,
    pub source: &'a Source,
    pub plan: &'a PreparedPlan,
    pub run: DurableTaskRun<'a>,
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
        let mut bound = self.current.durable_task_id.clone();
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
                PreparedItem::New { task, .. } => {
                    self.new_durable_task(connection, task, binding, &mut bound)
                        .await?
                }
                PreparedItem::Existing { .. } => {
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
        Ok(Written { bound, queued })
    }
    async fn new_durable_task(
        &self,
        connection: &mut sqlx::PgConnection,
        task: &super::super::PreparedDurableTask,
        binding: &mut DurableTaskBinding,
        bound: &mut Option<Uuid7>,
    ) -> Result<bool> {
        let active = bound.is_none();
        insert::task(
            connection,
            self.current,
            self.plan,
            task,
            binding,
            insert::Acceptance {
                source: self.source,
                active,
                unfinished_max: self.tasks.dependencies.policy.unfinished(),
            },
        )
        .await?;
        binding.applies_here = active;
        binding.durable_task_generation = Some(1);
        if active {
            insert::run(
                connection,
                self.run,
                binding,
                self.tasks.times().slice_seconds(),
            )
            .await?;
            insert::bind_turn(connection, &self.plan.source_turn_id, binding).await?;
            bound.clone_from(&binding.durable_task_id);
        }
        Ok(!active)
    }
}
