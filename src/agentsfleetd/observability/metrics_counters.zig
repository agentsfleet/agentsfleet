//! In-process metrics registry exposed in Prometheus text format.

const std = @import("std");

const fleet_metrics = @import("metrics_fleet.zig");
pub const incFleetsTriggered = fleet_metrics.incFleetsTriggered;

// ── Lease-poll cost + readiness index ───────────────────────────────────────
// Global and unlabelled: these describe the control plane's own discovery cost,
// not any one fleet, workspace, tenant, or runner, so they live here rather than
// in the per-runner labelled table (`metrics_runner.zig`).
//
// The two totals are only meaningful against `lease_polls_total` — an operator
// reads `rate(candidates_scanned) / rate(polls)` for mean fan-out per poll and
// `rate(db_roundtrips) / rate(polls)` for mean database cost per poll. Shipping
// the numerators without that denominator would make a traffic increase
// indistinguishable from a fan-out regression, which is the exact confusion
// these families exist to remove.

pub const LEASE_POLLS_NAME = "agentsfleet_lease_polls_total";
pub const LEASE_POLLS_HELP = "Lease polls served, the denominator for the per-poll cost families below.";
pub const CANDIDATES_SCANNED_NAME = "agentsfleet_lease_poll_candidates_scanned_total";
pub const CANDIDATES_SCANNED_HELP = "Fleets examined across all lease polls; divide by lease polls for mean fan-out.";
pub const DB_ROUNDTRIPS_NAME = "agentsfleet_lease_poll_db_roundtrips_total";
pub const DB_ROUNDTRIPS_HELP = "Postgres round-trips issued on the lease path; an idle poll must contribute zero.";
pub const READY_DEPTH_NAME = "agentsfleet_fleet_ready_depth";
pub const READY_DEPTH_HELP = "Fleets in the shared readiness index, sampled by the reclaim sweeper. NOT summable across replicas — every replica samples the same index, so use any single series.";
pub const READY_WRITE_FAILURES_NAME = "agentsfleet_fleet_ready_write_failures_total";
pub const READY_WRITE_FAILURES_HELP = "Readiness index writes (mark or clear) that failed against Redis. Unlabelled: which of the two failed does not change the operator's response, and the log line carries it.";

// ── Runner maintenance ──────────────────────────────────────────────────────
// Unlabelled like the lease-poll families: both describe control-plane
// housekeeping, not any one tenant's work; the log lines carry the per-table
// and per-tenant breakdowns.

pub const RETENTION_SWEPT_NAME = "agentsfleet_runner_retention_swept_total";
pub const RETENTION_SWEPT_HELP = "Terminal runner lease/event rows deleted by the retention sweep. A flat line on a busy control plane means the sweeper is not running OR is failing every cycle — read it beside the failures counter, which tells the two apart.";
pub const RETENTION_SWEEP_FAILURES_NAME = "agentsfleet_runner_retention_sweep_failures_total";
pub const RETENTION_SWEEP_FAILURES_HELP = "Retention sweep cycles that ended in an error. Rising means history is no longer being pruned; the log line carries the error.";
pub const TEARDOWN_UNREGISTER_FAILURES_NAME = "agentsfleet_account_teardown_unregister_failures_total";
pub const TEARDOWN_UNREGISTER_FAILURES_HELP = "Schedule unregister calls that failed during an account purge. Non-zero means an erased tenant may still have a firing timer upstream; the log line names the tenant.";

pub const Snapshot = struct {
    api_backpressure_rejections_total: u64,
    api_in_flight_requests: u64,
    sse_backpressure_rejections_total: u64,
    sse_in_flight_streams: u64,
    sse_dropped_frames_total: u64,
    sse_hub_reconnects_total: u64,
    fleet_triggered_total: u64 = 0,
    // Lease-poll cost + readiness index.
    lease_polls_total: u64 = 0,
    lease_poll_candidates_scanned_total: u64 = 0,
    lease_poll_db_roundtrips_total: u64 = 0,
    fleet_ready_depth: u64 = 0,
    fleet_ready_write_failures_total: u64 = 0,
    // Runner maintenance counters.
    runner_retention_swept_total: u64 = 0,
    runner_retention_sweep_failures_total: u64 = 0,
    account_teardown_unregister_failures_total: u64 = 0,
    // Signup funnel counters.
    signup_bootstrapped_total: u64 = 0,
    signup_replayed_total: u64 = 0,
    signup_failed_bad_sig_total: u64 = 0,
    signup_failed_stale_ts_total: u64 = 0,
    signup_failed_missing_email_total: u64 = 0,
    signup_failed_db_error_total: u64 = 0,
    signup_failed_pool_unavailable_total: u64 = 0,
    signup_failed_metadata_writeback_total: u64 = 0,
};

var g_api_backpressure_rejections_total = std.atomic.Value(u64).init(0);
var g_api_in_flight_requests = std.atomic.Value(u64).init(0);
var g_sse_backpressure_rejections_total = std.atomic.Value(u64).init(0);
var g_sse_in_flight_streams = std.atomic.Value(u64).init(0);
var g_sse_dropped_frames_total = std.atomic.Value(u64).init(0);
var g_sse_hub_reconnects_total = std.atomic.Value(u64).init(0);
var g_signup_bootstrapped_total = std.atomic.Value(u64).init(0);
var g_signup_replayed_total = std.atomic.Value(u64).init(0);
var g_signup_failed_bad_sig_total = std.atomic.Value(u64).init(0);
var g_signup_failed_stale_ts_total = std.atomic.Value(u64).init(0);
var g_signup_failed_missing_email_total = std.atomic.Value(u64).init(0);
var g_signup_failed_db_error_total = std.atomic.Value(u64).init(0);
var g_signup_failed_pool_unavailable_total = std.atomic.Value(u64).init(0);
var g_signup_failed_metadata_writeback_total = std.atomic.Value(u64).init(0);
var g_lease_polls_total = std.atomic.Value(u64).init(0);
var g_lease_poll_candidates_scanned_total = std.atomic.Value(u64).init(0);
var g_lease_poll_db_roundtrips_total = std.atomic.Value(u64).init(0);
var g_fleet_ready_depth = std.atomic.Value(u64).init(0);
var g_fleet_ready_write_failures_total = std.atomic.Value(u64).init(0);
var g_runner_retention_swept_total = std.atomic.Value(u64).init(0);
var g_runner_retention_sweep_failures_total = std.atomic.Value(u64).init(0);
var g_account_teardown_unregister_failures_total = std.atomic.Value(u64).init(0);

// safe because: every store/load below is an independent stat counter or
// gauge — readers (the /metrics scrape) tolerate staleness, and no other
// memory is published through these atomics.

pub fn incApiBackpressureRejections() void {
    _ = g_api_backpressure_rejections_total.fetchAdd(1, .monotonic); // safe because: see module note above
}

pub fn setApiInFlightRequests(v: u32) void {
    g_api_in_flight_requests.store(@as(u64, @intCast(v)), .release); // safe because: see module note above
}

pub fn incSseBackpressureRejections() void {
    _ = g_sse_backpressure_rejections_total.fetchAdd(1, .monotonic); // safe because: see module note above
}

pub fn setSseInFlightStreams(v: u32) void {
    g_sse_in_flight_streams.store(@as(u64, @intCast(v)), .release); // safe because: see module note above
}

pub fn incSseDroppedFrames() void {
    _ = g_sse_dropped_frames_total.fetchAdd(1, .monotonic); // safe because: see module note above
}

pub fn incSseHubReconnects() void {
    _ = g_sse_hub_reconnects_total.fetchAdd(1, .monotonic); // safe because: see module note above
}

// Signup funnel counters. Failure reasons enumerated so a single Prometheus
// query can answer "how many signups failed for reason X over Y?"
pub const SignupFailReason = enum { bad_sig, stale_ts, missing_email, db_error, pool_unavailable, metadata_writeback };

/// Wire label values for the signup-failure `reason` dimension, one per
/// SignupFailReason field, in declaration order — the exporter iterates this.
pub const SIGNUP_FAIL_REASON_LABELS = blk: {
    const fields = @typeInfo(SignupFailReason).@"enum".fields;
    var labels: [fields.len][]const u8 = undefined;
    for (fields, 0..) |f, i| labels[i] = f.name;
    break :blk labels;
};

pub fn incSignupBootstrapped() void {
    _ = g_signup_bootstrapped_total.fetchAdd(1, .monotonic); // safe because: see module note above
}
pub fn incSignupReplayed() void {
    _ = g_signup_replayed_total.fetchAdd(1, .monotonic); // safe because: see module note above
}
pub fn incSignupFailed(reason: SignupFailReason) void {
    const slot = switch (reason) {
        .bad_sig => &g_signup_failed_bad_sig_total,
        .stale_ts => &g_signup_failed_stale_ts_total,
        .missing_email => &g_signup_failed_missing_email_total,
        .db_error => &g_signup_failed_db_error_total,
        .pool_unavailable => &g_signup_failed_pool_unavailable_total,
        .metadata_writeback => &g_signup_failed_metadata_writeback_total,
    };
    _ = slot.fetchAdd(1, .monotonic); // safe because: see module note above
}

// ── Lease-poll cost + readiness index writers ───────────────────────────────

/// Record what one completed lease poll cost: how many fleets it examined and how
/// many Postgres round-trips it issued. Called once per poll, on every exit path,
/// so an idle poll contributes a candidate count and a round-trip count of zero
/// rather than contributing nothing — an absent sample would leave the idle case
/// invisible, which is the defect this family exists to expose.
pub fn observeLeasePoll(candidates_scanned: u64, db_roundtrips: u64) void {
    _ = g_lease_polls_total.fetchAdd(1, .monotonic); // safe because: see module note above
    _ = g_lease_poll_candidates_scanned_total.fetchAdd(candidates_scanned, .monotonic); // safe because: see module note above
    _ = g_lease_poll_db_roundtrips_total.fetchAdd(db_roundtrips, .monotonic); // safe because: see module note above
}

/// Overwrite the readiness-depth sample. A SETTER ONLY, deliberately: the index
/// is one hash shared by every replica, so a process-local counter incremented on
/// mark and decremented on clear could not describe it — one replica marks while
/// another clears, a restart zeroes the local delta, and a repeat mark for an
/// already-present fleet changes no field count. The sweeper reads the real field
/// count once per pass and calls this; `/metrics` renders the sample, keeping the
/// scrape path free of both datastores.
pub fn setReadyIndexDepth(fields: u64) void {
    g_fleet_ready_depth.store(fields, .release); // safe because: see module note above
}

pub fn incReadyWriteFailure() void {
    _ = g_fleet_ready_write_failures_total.fetchAdd(1, .monotonic); // safe because: see module note above
}

/// One retention cycle deletes rows in batches; the sweeper reports the cycle's
/// combined count once so the series moves in sweep-sized steps.
pub fn addRetentionSwept(rows: u64) void {
    _ = g_runner_retention_swept_total.fetchAdd(rows, .monotonic); // safe because: see module note above
}

/// A cycle that ended in an error. Counted per cycle, not per statement: the
/// operator's question is "is retention still running", and one series that
/// answers it beats one that varies with how far a cycle got.
pub fn incRetentionSweepFailure() void {
    _ = g_runner_retention_sweep_failures_total.fetchAdd(1, .monotonic); // safe because: see module note above
}

pub fn incTeardownUnregisterFailure() void {
    _ = g_account_teardown_unregister_failures_total.fetchAdd(1, .monotonic); // safe because: see module note above
}

fn loadStat(counter: *std.atomic.Value(u64)) u64 {
    return counter.load(.acquire); // safe because: scrape-time read of an independent stat counter; see module note
}

pub fn snapshot() Snapshot {
    var s = Snapshot{
        .api_backpressure_rejections_total = loadStat(&g_api_backpressure_rejections_total),
        .api_in_flight_requests = loadStat(&g_api_in_flight_requests),
        .sse_backpressure_rejections_total = loadStat(&g_sse_backpressure_rejections_total),
        .sse_in_flight_streams = loadStat(&g_sse_in_flight_streams),
        .sse_dropped_frames_total = loadStat(&g_sse_dropped_frames_total),
        .sse_hub_reconnects_total = loadStat(&g_sse_hub_reconnects_total),
    };
    s.fleet_triggered_total = fleet_metrics.snapshotFleetFields().fleet_triggered_total;
    s.signup_bootstrapped_total = loadStat(&g_signup_bootstrapped_total);
    s.signup_replayed_total = loadStat(&g_signup_replayed_total);
    s.signup_failed_bad_sig_total = loadStat(&g_signup_failed_bad_sig_total);
    s.signup_failed_stale_ts_total = loadStat(&g_signup_failed_stale_ts_total);
    s.signup_failed_missing_email_total = loadStat(&g_signup_failed_missing_email_total);
    s.signup_failed_db_error_total = loadStat(&g_signup_failed_db_error_total);
    s.signup_failed_pool_unavailable_total = loadStat(&g_signup_failed_pool_unavailable_total);
    s.signup_failed_metadata_writeback_total = loadStat(&g_signup_failed_metadata_writeback_total);
    s.lease_polls_total = loadStat(&g_lease_polls_total);
    s.lease_poll_candidates_scanned_total = loadStat(&g_lease_poll_candidates_scanned_total);
    s.lease_poll_db_roundtrips_total = loadStat(&g_lease_poll_db_roundtrips_total);
    s.fleet_ready_depth = loadStat(&g_fleet_ready_depth);
    s.fleet_ready_write_failures_total = loadStat(&g_fleet_ready_write_failures_total);
    s.runner_retention_swept_total = loadStat(&g_runner_retention_swept_total);
    s.runner_retention_sweep_failures_total = loadStat(&g_runner_retention_sweep_failures_total);
    s.account_teardown_unregister_failures_total = loadStat(&g_account_teardown_unregister_failures_total);
    return s;
}

/// Test-only reset for the lease-poll and readiness families, so a render test
/// starts from a known zero rather than inheriting another test's increments.
pub fn resetLeasePollMetricsForTest() void {
    g_lease_polls_total.store(0, .release); // safe because: single-threaded test reset
    g_lease_poll_candidates_scanned_total.store(0, .release);
    g_lease_poll_db_roundtrips_total.store(0, .release);
    g_fleet_ready_depth.store(0, .release);
    g_fleet_ready_write_failures_total.store(0, .release);
}

/// Test-only reset for the runner-maintenance family, same isolation rationale.
pub fn resetRunnerMaintenanceMetricsForTest() void {
    g_runner_retention_swept_total.store(0, .release); // safe because: single-threaded test reset
    g_runner_retention_sweep_failures_total.store(0, .release);
    g_account_teardown_unregister_failures_total.store(0, .release);
}
