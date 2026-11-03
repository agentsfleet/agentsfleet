//! Durable objective ownership within the existing admission and lease path.
//!
//! The store owns native rows and transitions. Its pool remains the lease
//! store's private pool, and task work never introduces a competing queue.

mod admit;
mod continuation;
mod control;
mod guard;
mod observe;
mod read;
mod refusal;
mod row;
mod sql;
mod types;

use std::pin::Pin;

use afd_core::id::Uuid7;
use afd_observability::metrics::label::task::{TaskOperation, TaskOutcome};
use tracing::Instrument as _;

use crate::{Result, lease::Leases};

pub use refusal::TaskRefusal;
pub use types::{
    PreparedPlan, PreparedTask, TaskBinding, TaskBindings, TaskControl, TaskLease, TaskScope,
    TaskSnapshot,
};

/// Object-safe native operation using the owning crate's error shell.
pub type TaskFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Behavior of the authoritative task owner; all writes are fenced natively.
pub trait TaskLifecycle: Send + Sync + std::fmt::Debug {
    /// Bind a complete prepared interpretation without allocating on replay.
    fn bind<'a>(
        &'a self,
        lease: TaskLease<'a>,
        plan: &'a PreparedPlan,
    ) -> TaskFuture<'a, TaskBindings>;
    /// Read a task only inside the caller's authorized workspace/fleet scope.
    fn read<'a>(&'a self, scope: &'a TaskScope, task_id: &'a Uuid7)
    -> TaskFuture<'a, TaskSnapshot>;
    /// Apply a versioned human lifecycle control without resetting budgets.
    fn control<'a>(
        &'a self,
        scope: &'a TaskScope,
        task_id: &'a Uuid7,
        expected_version: u64,
        control: TaskControl,
    ) -> TaskFuture<'a, TaskSnapshot>;
}

/// Task data and native dependencies; behavior is implemented by the trait.
#[derive(Debug, Clone)]
pub struct Tasks {
    leases: Leases,
    admissions: afd_admission::Admissions,
}

impl Tasks {
    /// Uses the same database, entropy and lease authority as the fleet plane.
    #[must_use]
    pub const fn new(leases: Leases, admissions: afd_admission::Admissions) -> Self {
        Self { leases, admissions }
    }
}

impl TaskLifecycle for Tasks {
    fn bind<'a>(
        &'a self,
        lease: TaskLease<'a>,
        plan: &'a PreparedPlan,
    ) -> TaskFuture<'a, TaskBindings> {
        let span = tracing::info_span!("task_bind", lease_id = lease.lease_id.as_str());
        Box::pin(
            observe::measure(
                TaskOperation::Bind,
                admit::bind(self, lease, plan),
                |bindings| {
                    if bindings.replayed {
                        TaskOutcome::Replayed
                    } else {
                        TaskOutcome::Accepted
                    }
                },
            )
            .instrument(span),
        )
    }

    fn read<'a>(
        &'a self,
        scope: &'a TaskScope,
        task_id: &'a Uuid7,
    ) -> TaskFuture<'a, TaskSnapshot> {
        let span = tracing::info_span!("task_read", task_id = task_id.as_str());
        Box::pin(
            observe::measure(
                TaskOperation::Read,
                read::read(self, scope, task_id),
                |_| TaskOutcome::Accepted,
            )
            .instrument(span),
        )
    }

    fn control<'a>(
        &'a self,
        scope: &'a TaskScope,
        task_id: &'a Uuid7,
        expected_version: u64,
        control: TaskControl,
    ) -> TaskFuture<'a, TaskSnapshot> {
        let operation = match control {
            TaskControl::Pause => TaskOperation::Pause,
            TaskControl::Resume => TaskOperation::Resume,
            TaskControl::Cancel => TaskOperation::Cancel,
        };
        let span = tracing::info_span!("task_control", task_id = task_id.as_str());
        Box::pin(
            observe::measure(
                operation,
                control::apply(self, scope, task_id, expected_version, control),
                |_| TaskOutcome::Accepted,
            )
            .instrument(span),
        )
    }
}
