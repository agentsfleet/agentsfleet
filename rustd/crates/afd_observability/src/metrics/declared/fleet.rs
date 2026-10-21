//! What the runner plane and the background sweepers say about themselves.
//!
//! Queue backlog, lease polls, runner activity, and background maintenance.

use crate::metrics::family::{CounterKind, Declared, GaugeKind, HistogramKind};

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

/// The denominator for the three below.
pub const LEASE_POLLS_TOTAL: Declared<CounterKind> = Declared::new("agentsfleet_lease_polls_total");

/// Rate ÷ polls = fan-out per poll.
pub const LEASE_POLL_CANDIDATES_SCANNED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_lease_poll_candidates_scanned_total");

/// Rate ÷ polls = database cost per poll; an idle poll adds one.
pub const LEASE_POLL_DB_ROUNDTRIPS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_lease_poll_db_roundtrips_total");

/// Rate ÷ polls = Dragonfly cost per poll; an idle poll adds zero.
pub const LEASE_POLL_DRAGONFLY_COMMANDS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_lease_poll_dragonfly_commands_total");

/// Bytes of earlier turns each chat lease carries.
pub const LEASE_HISTORY_BYTES: Declared<HistogramKind> =
    Declared::new("agentsfleet_lease_history_bytes");

/// Chat leases whose earlier turns a cap cut, by the cap.
pub const LEASE_HISTORY_CUTS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_lease_history_cuts_total");

/// Won claims on a fleet whose slot named a sandbox held live at the claim,
/// by who won.
pub const LEASE_HELD_CLAIMS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_lease_held_claims_total");

/// Chat leases issued without their turns because the thread read failed.
pub const LEASE_HISTORY_READ_FAILURES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_lease_history_read_failures_total");

/// Leases granted; fresh ÷ accepted admissions is pickup, a rising reclaimed
/// share is runners dying mid-run.
///
/// Labels: `kind`.
pub const FLEET_RUNS_STARTED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_fleet_runs_started_total");

/// Time through the fleet delivery path, from server and runner clocks.
///
/// Labels: `stage`. Each stage is recorded once for an active lease attempt
/// that reaches it; a reclaim is a separate attempt on the same event.
pub const FLEET_DELIVERY_STAGE_SECONDS: Declared<HistogramKind> =
    Declared::new("agentsfleet_fleet_delivery_stage_seconds");

/// Wake notifications failing or past their deadline, and interrupted listeners.
///
/// A ring past its deadline is counted though the publish may still land; a
/// gap is counted per partition channel, so one redial counts up to sixteen.
///
/// Labels: `kind`.
pub const FLEET_READY_WAKE_FAILURES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_fleet_ready_wake_failures_total");

/// Installed fleets by lifecycle status, as the last census pass counted them.
///
/// Labels: `status`.
pub const FLEETS: Declared<GaugeKind> = Declared::new("agentsfleet_fleets");

/// Retention pruning throughput.
pub const RUNNER_RETENTION_SWEPT_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_retention_swept_total");

/// Retention sweeps failing.
pub const RUNNER_RETENTION_SWEEP_FAILURES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_retention_sweep_failures_total");

/// Accepted and replayed admissions, database refusals, and budget refusals.
///
/// Labels: `outcome`.
pub const ADMISSIONS_TOTAL: Declared<CounterKind> = Declared::new("agentsfleet_admissions_total");

/// Unsettled admissions counted by the last fleet census pass.
pub const ADMISSION_BACKLOG: Declared<GaugeKind> = Declared::new("agentsfleet_admission_backlog");

/// How long the oldest unsettled admission has waited.
pub const ADMISSION_BACKLOG_OLDEST_AGE_SECONDS: Declared<GaugeKind> =
    Declared::new("agentsfleet_admission_backlog_oldest_age_seconds");

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
/// Labels: `runner_id,outcome`, plus `fault` on the failing half.
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
