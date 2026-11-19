//! Write one complete interpretation through the already locked source and fleet slot.

use super::super::{PreparedItem, PreparedPlan, TaskBinding, TaskLease, Tasks, guard::LeaseRow};
use super::{input, insert, prepare::Wake, source::Source};
use crate::Result;
use af_core::id::Uuid7;
use afd_admission::PreparedAdmission;

pub(super) struct Writer<'a> {
    pub tasks: &'a Tasks,
    pub current: &'a LeaseRow,
    pub source: &'a Source,
    pub plan: &'a PreparedPlan,
    pub lease: TaskLease<'a>,
}

pub(super) struct Written {
    pub bound: Option<Uuid7>,
    pub queued: Vec<usize>,
}

impl Writer<'_> {
    pub(super) async fn apply(
        &self,
        connection: &mut sqlx::PgConnection,
        identities: &mut [TaskBinding],
        continuations: &[Option<Wake>],
        prepared: Vec<Option<PreparedAdmission<'_>>>,
    ) -> Result<Written> {
        let mut queued = Vec::new();
        let mut bound = self.current.task_id.clone();
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
                    self.new_task(connection, task, binding, &mut bound).await?
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
                    .admissions
                    .stage(
                        connection,
                        continuation.ok_or(super::super::TaskRefusal::StaleState)?,
                    )
                    .await?;
                insert::admission(connection, &admitted.stored.id, binding).await?;
                queued.push(ordinal);
            }
        }
        Ok(Written { bound, queued })
    }
    async fn new_task(
        &self,
        connection: &mut sqlx::PgConnection,
        task: &super::super::PreparedTask,
        binding: &mut TaskBinding,
        bound: &mut Option<Uuid7>,
    ) -> Result<bool> {
        let active = bound.is_none();
        insert::task(
            connection,
            self.current,
            self.plan,
            task,
            binding,
            insert::Admission {
                source: self.source,
                active,
                unfinished_max: self.tasks.dependencies.policy.unfinished(),
            },
        )
        .await?;
        binding.applies_here = active;
        binding.task_generation = Some(1);
        if active {
            insert::lease(connection, self.lease, binding).await?;
            insert::admission(connection, &self.plan.source_admission_id, binding).await?;
            bound.clone_from(&binding.task_id);
        }
        Ok(!active)
    }
}
