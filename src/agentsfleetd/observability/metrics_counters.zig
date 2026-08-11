//! Control-plane counter writers: API/SSE guardrails, fleet triggers, the
//! signup funnel, lease-poll cost, the readiness index, and runner
//! maintenance. Storage lives in the generated instrument layer
//! (otel_instruments.zig) — each writer here is the typed call site for one
//! registry family, and `Snapshot`/`snapshot()` are reads over the same cells
//! for the integration suites that assert exact deltas.
//!
//! Family knowledge worth keeping (the retired help prose) rides the registry
//! rows in otel_metrics_families.zig, beside each family's wire identity.

const instruments = @import("otel_instruments.zig");

// ── Family names (RULE UFS: one home per wire string) ───────────────────────
// Global and unlabelled: the lease-poll and maintenance families describe the
// control plane's own discovery cost, not any one fleet, workspace, tenant, or
// runner, so they live here rather than in the per-runner labelled table
// (`metrics_runner.zig`). The two lease-poll totals are only meaningful
// against `lease_polls_total` — rate(candidates)/rate(polls) is mean fan-out,
// rate(roundtrips)/rate(polls) is mean database cost per poll.

pub const LEASE_POLLS_NAME = "agentsfleet_lease_polls_total";
pub const CANDIDATES_SCANNED_NAME = "agentsfleet_lease_poll_candidates_scanned_total";
pub const DB_ROUNDTRIPS_NAME = "agentsfleet_lease_poll_db_roundtrips_total";
pub const READY_DEPTH_NAME = "agentsfleet_fleet_ready_depth";
pub const READY_WRITE_FAILURES_NAME = "agentsfleet_fleet_ready_write_failures_total";
pub const RETENTION_SWEPT_NAME = "agentsfleet_runner_retention_swept_total";
pub const RETENTION_SWEEP_FAILURES_NAME = "agentsfleet_runner_retention_sweep_failures_total";
pub const TEARDOWN_UNREGISTER_FAILURES_NAME = "agentsfleet_account_teardown_unregister_failures_total";

// No field defaults: a field added here without a matching read in snapshot()
// must fail the build, never report a silent zero.
pub const Snapshot = struct {
    api_backpressure_rejections_total: u64,
    api_in_flight_requests: u64,
    sse_backpressure_rejections_total: u64,
    sse_in_flight_streams: u64,
    sse_dropped_frames_total: u64,
    sse_hub_reconnects_total: u64,
    fleet_triggered_total: u64,
    // Lease-poll cost + readiness index.
    lease_polls_total: u64,
    lease_poll_candidates_scanned_total: u64,
    lease_poll_db_roundtrips_total: u64,
    fleet_ready_depth: u64,
    fleet_ready_write_failures_total: u64,
    // Runner maintenance counters.
    runner_retention_swept_total: u64,
    runner_retention_sweep_failures_total: u64,
    account_teardown_unregister_failures_total: u64,
    // Signup funnel counters.
    signup_bootstrapped_total: u64,
    signup_replayed_total: u64,
    signup_failed_bad_sig_total: u64,
    signup_failed_stale_ts_total: u64,
    signup_failed_missing_email_total: u64,
    signup_failed_db_error_total: u64,
    signup_failed_pool_unavailable_total: u64,
    signup_failed_metadata_writeback_total: u64,
};

pub fn incApiBackpressureRejections() void {
    instruments.inc(.api_backpressure_rejections, .{});
}

pub fn setApiInFlightRequests(v: u32) void {
    instruments.set(.api_in_flight_requests, .{}, v);
}

pub fn incSseBackpressureRejections() void {
    instruments.inc(.sse_backpressure_rejections, .{});
}

pub fn setSseInFlightStreams(v: u32) void {
    instruments.set(.sse_in_flight_streams, .{}, v);
}

pub fn incSseDroppedFrames() void {
    instruments.inc(.sse_dropped_frames, .{});
}

pub fn incSseHubReconnects() void {
    instruments.inc(.sse_hub_reconnects, .{});
}

pub fn incFleetsTriggered() void {
    instruments.inc(.fleet_triggered, .{});
}

// Signup funnel counters. Failure reasons enumerated so a single query can
// answer "how many signups failed for reason X over Y?" — the registry
// declares the `reason` dimension off this enum.
pub const SignupFailReason = enum { bad_sig, stale_ts, missing_email, db_error, pool_unavailable, metadata_writeback };

pub fn incSignupBootstrapped() void {
    instruments.inc(.signup_bootstrapped, .{});
}
pub fn incSignupReplayed() void {
    instruments.inc(.signup_replayed, .{});
}
pub fn incSignupFailed(reason: SignupFailReason) void {
    instruments.inc(.signup_failed, .{ .reason = reason });
}

// ── Lease-poll cost + readiness index writers ───────────────────────────────

/// Record what one completed lease poll cost: how many fleets it examined and how
/// many Postgres round-trips it issued. Called once per poll, on every exit path,
/// so an idle poll contributes a candidate count and a round-trip count of zero
/// rather than contributing nothing — an absent sample would leave the idle case
/// invisible, which is the defect this family exists to expose.
pub fn observeLeasePoll(candidates_scanned: u64, db_roundtrips: u64) void {
    instruments.inc(.lease_polls, .{});
    instruments.add(.lease_poll_candidates_scanned, .{}, candidates_scanned);
    instruments.add(.lease_poll_db_roundtrips, .{}, db_roundtrips);
}

/// Overwrite the readiness-depth sample. A SETTER ONLY, deliberately: the index
/// is one hash shared by every replica, so a process-local counter incremented on
/// mark and decremented on clear could not describe it — one replica marks while
/// another clears, a restart zeroes the local delta, and a repeat mark for an
/// already-present fleet changes no field count. The sweeper reads the real field
/// count once per pass and calls this; the flush collect emits the sample,
/// keeping the export path free of both datastores.
pub fn setReadyIndexDepth(fields: u64) void {
    instruments.set(.fleet_ready_depth, .{}, fields);
}

pub fn incReadyWriteFailure() void {
    instruments.inc(.fleet_ready_write_failures, .{});
}

/// One retention cycle deletes rows in batches; the sweeper reports the cycle's
/// combined count once so the series moves in sweep-sized steps.
pub fn addRetentionSwept(rows: u64) void {
    instruments.add(.runner_retention_swept, .{}, rows);
}

/// A cycle that ended in an error. Counted per cycle, not per statement: the
/// operator's question is "is retention still running", and one series that
/// answers it beats one that varies with how far a cycle got.
pub fn incRetentionSweepFailure() void {
    instruments.inc(.runner_retention_sweep_failures, .{});
}

pub fn incTeardownUnregisterFailure() void {
    instruments.inc(.account_teardown_unregister_failures, .{});
}

pub fn snapshot() Snapshot {
    return .{
        .api_backpressure_rejections_total = instruments.snapshotCell(.api_backpressure_rejections, .{}),
        .api_in_flight_requests = instruments.snapshotCell(.api_in_flight_requests, .{}),
        .sse_backpressure_rejections_total = instruments.snapshotCell(.sse_backpressure_rejections, .{}),
        .sse_in_flight_streams = instruments.snapshotCell(.sse_in_flight_streams, .{}),
        .sse_dropped_frames_total = instruments.snapshotCell(.sse_dropped_frames, .{}),
        .sse_hub_reconnects_total = instruments.snapshotCell(.sse_hub_reconnects, .{}),
        .fleet_triggered_total = instruments.snapshotCell(.fleet_triggered, .{}),
        .lease_polls_total = instruments.snapshotCell(.lease_polls, .{}),
        .lease_poll_candidates_scanned_total = instruments.snapshotCell(.lease_poll_candidates_scanned, .{}),
        .lease_poll_db_roundtrips_total = instruments.snapshotCell(.lease_poll_db_roundtrips, .{}),
        .fleet_ready_depth = instruments.snapshotCell(.fleet_ready_depth, .{}),
        .fleet_ready_write_failures_total = instruments.snapshotCell(.fleet_ready_write_failures, .{}),
        .runner_retention_swept_total = instruments.snapshotCell(.runner_retention_swept, .{}),
        .runner_retention_sweep_failures_total = instruments.snapshotCell(.runner_retention_sweep_failures, .{}),
        .account_teardown_unregister_failures_total = instruments.snapshotCell(.account_teardown_unregister_failures, .{}),
        .signup_bootstrapped_total = instruments.snapshotCell(.signup_bootstrapped, .{}),
        .signup_replayed_total = instruments.snapshotCell(.signup_replayed, .{}),
        .signup_failed_bad_sig_total = instruments.snapshotCell(.signup_failed, .{ .reason = .bad_sig }),
        .signup_failed_stale_ts_total = instruments.snapshotCell(.signup_failed, .{ .reason = .stale_ts }),
        .signup_failed_missing_email_total = instruments.snapshotCell(.signup_failed, .{ .reason = .missing_email }),
        .signup_failed_db_error_total = instruments.snapshotCell(.signup_failed, .{ .reason = .db_error }),
        .signup_failed_pool_unavailable_total = instruments.snapshotCell(.signup_failed, .{ .reason = .pool_unavailable }),
        .signup_failed_metadata_writeback_total = instruments.snapshotCell(.signup_failed, .{ .reason = .metadata_writeback }),
    };
}

/// Test-only reset for the lease-poll and readiness families, so a render test
/// starts from a known zero rather than inheriting another test's increments.
pub fn resetLeasePollMetricsForTest() void {
    instruments.resetCellsForTest(&.{
        .lease_polls,
        .lease_poll_candidates_scanned,
        .lease_poll_db_roundtrips,
        .fleet_ready_depth,
        .fleet_ready_write_failures,
    });
}

/// Test-only reset for the runner-maintenance family, same isolation rationale.
pub fn resetRunnerMaintenanceMetricsForTest() void {
    instruments.resetCellsForTest(&.{
        .runner_retention_swept,
        .runner_retention_sweep_failures,
        .account_teardown_unregister_failures,
    });
}
