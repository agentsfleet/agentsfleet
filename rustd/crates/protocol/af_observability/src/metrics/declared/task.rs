//! Durable task operations at the native lifecycle boundary.

use crate::metrics::family::{CounterKind, Declared, HistogramKind};

/// Completed operations by bounded operation and outcome.
pub const TASK_OPERATIONS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_task_operations_total");

/// Native lifecycle latency, including failed operations and lock waits.
pub const TASK_OPERATION_SECONDS: Declared<HistogramKind> =
    Declared::new("agentsfleet_task_operation_seconds");

/// Completed candidate attempts, including contention and unreadable rows.
pub const DECISION_RECOVERY_ATTEMPTS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_task_decision_recovery_attempts_total");

/// Observed candidate age beyond its original issuance deadline.
pub const DECISION_RECOVERY_OVERDUE_SECONDS: Declared<HistogramKind> =
    Declared::new("agentsfleet_task_decision_recovery_overdue_seconds");
