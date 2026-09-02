//! What the runner plane and the background sweepers say about themselves.
//!
//! The widest group, and it stays one module because an operator reading a
//! stalled fleet reads across all of it: whether work was found (the ready
//! index), whether a runner took it (the lease poll), what the runner did with
//! it (the per-runner counters), and whether the sweepers behind them kept up.

use crate::metrics::family::{CounterKind, Declared, GaugeKind, HistogramKind};

/// Provider completion to proof-qualified verifier queueing.
pub const REPAIR_PRODUCTION_TO_QUEUE_SECONDS: Declared<HistogramKind> =
    Declared::new("agentsfleet_repair_production_to_queue_seconds");

/// Proof-qualified verifier queueing to completed Fleet report.
pub const REPAIR_QUEUE_TO_COMPLETION_SECONDS: Declared<HistogramKind> =
    Declared::new("agentsfleet_repair_queue_to_completion_seconds");

/// Trigger volume.
pub const FLEET_TRIGGERED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_fleet_triggered_total");

/// Signup funnel: fresh accounts.
pub const SIGNUP_BOOTSTRAPPED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_signup_bootstrapped_total");

/// Signup funnel: idempotent replays.
pub const SIGNUP_REPLAYED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_signup_replayed_total");

/// Rejected signups per cause.
///
/// Labels: `reason`.
pub const SIGNUP_FAILED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_signup_failed_total");

/// The denominator for the two below.
pub const LEASE_POLLS_TOTAL: Declared<CounterKind> = Declared::new("agentsfleet_lease_polls_total");

/// Rate ÷ polls = fan-out per poll.
pub const LEASE_POLL_CANDIDATES_SCANNED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_lease_poll_candidates_scanned_total");

/// Rate ÷ polls = DB cost per poll; idle polls must add zero.
pub const LEASE_POLL_DB_ROUNDTRIPS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_lease_poll_db_roundtrips_total");

/// Readiness backlog (not summable across replicas).
pub const FLEET_READY_DEPTH: Declared<GaugeKind> = Declared::new("agentsfleet_fleet_ready_depth");

/// Redis index writes failing.
pub const FLEET_READY_WRITE_FAILURES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_fleet_ready_write_failures_total");

/// Retention pruning throughput.
pub const RUNNER_RETENTION_SWEPT_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_retention_swept_total");

/// Retention sweeps failing.
pub const RUNNER_RETENTION_SWEEP_FAILURES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_retention_sweep_failures_total");

/// Teardown purges failing to unregister.
pub const ACCOUNT_TEARDOWN_UNREGISTER_FAILURES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_account_teardown_unregister_failures_total");

/// Accepted, replayed, or refused production evidence.
///
/// Labels: `outcome`.
pub const REPAIR_PROVIDER_RESULTS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_repair_provider_results_total");

/// Exact, missed, and ambiguous repair correlations.
///
/// Labels: `outcome`.
pub const REPAIR_CORRELATIONS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_repair_correlations_total");

/// Durable verifier intents created after exact correlation.
pub const REPAIR_VERIFICATION_INTENTS_CREATED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_repair_verification_intents_created_total");

/// Failed verifier dispatch attempts awaiting retry.
pub const REPAIR_DISPATCH_RETRIED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_repair_dispatch_retried_total");

/// Emitted and idempotently replayed proof-qualified events.
///
/// Labels: `outcome`.
pub const REPAIR_SYNTHETIC_EVENTS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_repair_synthetic_events_total");

/// Queued and completed verifier Fleet runs.
///
/// Labels: `outcome`.
pub const REPAIR_VERIFIER_RUNS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_repair_verifier_runs_total");

/// Due verifier sample capped at the dispatcher batch limit.
pub const REPAIR_DISPATCH_DUE_BATCH: Declared<GaugeKind> =
    Declared::new("agentsfleet_repair_dispatch_due_batch");

/// Age of the oldest due verifier intent.
pub const REPAIR_DISPATCH_OLDEST_AGE_SECONDS: Declared<GaugeKind> =
    Declared::new("agentsfleet_repair_dispatch_oldest_age_seconds");

/// Failure rate per reason.
///
/// Labels: `runner_id,reason`.
pub const RUNNER_FAILURES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_failures_total");

/// Increments only past 4096 runner slots.
pub const RUNNER_FAILURES_OVERFLOW_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_failures_overflow_total");

/// Run volume per runner.
///
/// Labels: `runner_id,outcome`.
pub const RUNNER_EXECUTIONS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_executions_total");

/// A runner going quiet.
///
/// Labels: `runner_id`.
pub const RUNNER_LAST_SEEN_SECONDS: Declared<GaugeKind> =
    Declared::new("agentsfleet_runner_last_seen_seconds");

/// Best-effort; self-heals on restart.
///
/// Labels: `runner_id`.
pub const RUNNER_ACTIVE_LEASES: Declared<GaugeKind> =
    Declared::new("agentsfleet_runner_active_leases");
