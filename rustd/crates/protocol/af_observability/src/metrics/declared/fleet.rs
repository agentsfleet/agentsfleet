//! What the runner plane and the background sweepers say about themselves.
//!
//! Queue backlog, run polls, runner activity, and background maintenance.

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

/// The denominator for the candidates count below.
pub const RUN_POLLS_TOTAL: Declared<CounterKind> = Declared::new("agentsfleet_run_polls_total");

/// Rate ÷ polls = fan-out per poll.
pub const RUN_POLL_CANDIDATES_SCANNED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_run_poll_candidates_scanned_total");

/// Bytes of earlier turns each chat run carries.
pub const RUN_HISTORY_BYTES: Declared<HistogramKind> =
    Declared::new("agentsfleet_run_history_bytes");

/// Chat runs whose earlier turns a cap cut, by the cap.
pub const RUN_HISTORY_CUTS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_run_history_cuts_total");

/// Won assignments on a fleet whose thread named a sandbox held live at the
/// assignment, by who won.
pub const RUN_HELD_ASSIGNMENTS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_run_held_assignments_total");

/// Accepted steers, by where they went.
pub const STEER_DISPOSITIONS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_steer_dispositions_total");

/// Run-path cache lookups; misses over lookups is the read rate a cache
/// leaves on its path.
///
/// Labels: `cache`, `outcome`.
pub const RUN_CACHE_LOOKUPS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_run_cache_lookups_total");

/// Messages live runs took from their fleets, assigned and settled.
pub const RUN_FOLDS_TOTAL: Declared<CounterKind> = Declared::new("agentsfleet_run_folds_total");

/// Chat runs issued without their turns because the thread read failed.
pub const RUN_HISTORY_READ_FAILURES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_run_history_read_failures_total");

/// Runs granted; fresh ÷ accepted turns is pickup, a rising reassigned
/// share is runners dying mid-run.
///
/// Labels: `kind`.
pub const FLEET_RUNS_STARTED_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_fleet_runs_started_total");

/// Time through the fleet delivery path, from server and runner clocks.
///
/// Labels: `stage`. Each stage is recorded once for an active run attempt
/// that reaches it; a reassignment is a separate attempt on the same event.
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

/// Accepted and replayed turns, database refusals, and budget refusals.
///
/// Labels: `outcome`.
pub const TURNS_TOTAL: Declared<CounterKind> = Declared::new("agentsfleet_turns_total");

/// Unsettled fleet turns counted by the last fleet census pass.
pub const TURN_BACKLOG: Declared<GaugeKind> = Declared::new("agentsfleet_turn_backlog");

/// How long the oldest unsettled fleet turn has waited.
pub const TURN_BACKLOG_OLDEST_AGE_SECONDS: Declared<GaugeKind> =
    Declared::new("agentsfleet_turn_backlog_oldest_age_seconds");

/// Failure rate per reason.
///
/// Labels: `runner_id,reason`.
pub const RUNNER_FAILURES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_failures_total");

/// Increments only past 4096 runner slots.
pub const RUNNER_FAILURES_OVERFLOW_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_failures_overflow_total");

/// Runs each runner acquired, and how each one it held ended.
///
/// Labels: `runner_id,outcome`.
pub const RUNNER_RUNS_TOTAL: Declared<CounterKind> = Declared::new("agentsfleet_runner_runs_total");

/// A runner going quiet.
///
/// Labels: `runner_id`.
pub const RUNNER_LAST_SEEN_SECONDS: Declared<GaugeKind> =
    Declared::new("agentsfleet_runner_last_seen_seconds");

/// Best-effort; self-heals on restart.
///
/// Labels: `runner_id`.
pub const RUNNER_ACTIVE_RUNS: Declared<GaugeKind> = Declared::new("agentsfleet_runner_active_runs");

/// Successful report answers, including idempotent retries.
pub const RUNNER_REPORT_ANSWERS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_report_answers_total");

/// Every native connection attempt, including failed HTTP/3 before fallback.
pub const RUNNER_STREAM_CONNECT_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_stream_connect_total");

/// Native TLS decisions; transport, peer role, group and outcome have closed labels.
pub const RUNNER_STREAM_TLS_HANDSHAKES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_stream_tls_handshakes_total");

/// Complete native connection attempt duration, including name resolution.
pub const RUNNER_STREAM_CONNECT_SECONDS: Declared<HistogramKind> =
    Declared::new("agentsfleet_runner_stream_connect_seconds");

/// Bounded operations, including memory waits and cancelled futures.
pub const RUNNER_STREAM_OPERATION_SECONDS: Declared<HistogramKind> =
    Declared::new("agentsfleet_runner_stream_operation_seconds");

/// Transport failures that caused an HTTP/2 attempt.
pub const RUNNER_STREAM_FALLBACK_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_stream_fallback_total");

/// Expired absolute deadlines, separated by the operation that stalled.
pub const RUNNER_STREAM_TIMEOUTS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_stream_timeouts_total");

/// Recent-ready notifications admitted, deduplicated or evicted by a replica.
pub const FLEET_READY_WAKE_QUEUE_EVENTS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_fleet_ready_wake_queue_events_total");

/// Native path round-trip measurements; unsupported measurements stay absent.
pub const RUNNER_STREAM_NATIVE_RTT_SECONDS: Declared<HistogramKind> =
    Declared::new("agentsfleet_runner_stream_native_rtt_seconds");
/// QUIC packet counter deltas, independent of application request success.
pub const RUNNER_STREAM_NATIVE_PACKETS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_stream_native_packets_total");
/// QUIC probe timeout deltas across all paths on established connections.
pub const RUNNER_STREAM_NATIVE_PTO_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_stream_native_pto_total");
/// Retransmitted stream bytes; distinct from the packet counter's unit.
pub const RUNNER_STREAM_NATIVE_RETRANSMITTED_BYTES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_stream_native_retransmitted_bytes_total");

/// Committed automatic quarantine and lift transitions, labelled only by change.
pub const RUNNER_CORDON_CHANGES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_runner_cordon_changes_total");
