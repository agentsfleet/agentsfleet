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
pub const CANDIDATES_MAX_NAME = "agentsfleet_lease_poll_candidates_max";
pub const CANDIDATES_MAX_HELP = "Most fleets any single lease poll examined (high-water mark; bounded by the per-poll ceiling).";
pub const DB_ROUNDTRIPS_NAME = "agentsfleet_lease_poll_db_roundtrips_total";
pub const DB_ROUNDTRIPS_HELP = "Postgres round-trips issued on the lease path; an idle poll must contribute zero.";
pub const READY_DEPTH_NAME = "agentsfleet_fleet_ready_depth";
pub const READY_DEPTH_HELP = "Fleets in the shared readiness index, sampled by the reclaim sweeper. NOT summable across replicas — every replica samples the same index, so use any single series.";
pub const READY_WRITE_FAILURES_NAME = "agentsfleet_fleet_ready_write_failures_total";
pub const READY_WRITE_FAILURES_HELP = "Readiness index writes that failed against Redis, labelled by which write.";
pub const READY_SWEEP_RECOVERIES_NAME = "agentsfleet_fleet_ready_sweep_recoveries_total";
pub const READY_SWEEP_RECOVERIES_HELP = "Fleets the reclaim sweeper re-marked as ready after the index lost them.";

/// Which readiness write failed. Owned here rather than in `queue/fleet_ready.zig`
/// because this module owns the metric's label axis, and because the queue module
/// already imports this one — defining it there would close an import cycle.
pub const ReadyWrite = enum {
    mark,
    clear,

    pub fn label(self: ReadyWrite) []const u8 {
        return @tagName(self);
    }
};

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
    lease_poll_candidates_max: u64 = 0,
    lease_poll_db_roundtrips_total: u64 = 0,
    fleet_ready_depth: u64 = 0,
    fleet_ready_write_failures_mark_total: u64 = 0,
    fleet_ready_write_failures_clear_total: u64 = 0,
    fleet_ready_sweep_recoveries_total: u64 = 0,
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
var g_lease_poll_candidates_max = std.atomic.Value(u64).init(0);
var g_lease_poll_db_roundtrips_total = std.atomic.Value(u64).init(0);
var g_fleet_ready_depth = std.atomic.Value(u64).init(0);
var g_fleet_ready_write_failures_mark_total = std.atomic.Value(u64).init(0);
var g_fleet_ready_write_failures_clear_total = std.atomic.Value(u64).init(0);
var g_fleet_ready_sweep_recoveries_total = std.atomic.Value(u64).init(0);

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
const SignupFailReason = enum { bad_sig, stale_ts, missing_email, db_error, pool_unavailable, metadata_writeback };

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
    // High-water mark via compare-and-swap: a plain store would let a small
    // concurrent poll overwrite a larger peak and hide the tail.
    var seen = g_lease_poll_candidates_max.load(.monotonic); // safe because: pure maximum tracking; a lost race retries below
    while (candidates_scanned > seen) {
        seen = g_lease_poll_candidates_max.cmpxchgWeak(seen, candidates_scanned, .monotonic, .monotonic) orelse break; // safe because: independent gauge, retry loop converges
    }
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

pub fn incReadyWriteFailure(which: ReadyWrite) void {
    const slot = switch (which) {
        .mark => &g_fleet_ready_write_failures_mark_total,
        .clear => &g_fleet_ready_write_failures_clear_total,
    };
    _ = slot.fetchAdd(1, .monotonic); // safe because: see module note above
}

pub fn incReadySweepRecovery() void {
    _ = g_fleet_ready_sweep_recoveries_total.fetchAdd(1, .monotonic); // safe because: see module note above
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
    s.lease_poll_candidates_max = loadStat(&g_lease_poll_candidates_max);
    s.lease_poll_db_roundtrips_total = loadStat(&g_lease_poll_db_roundtrips_total);
    s.fleet_ready_depth = loadStat(&g_fleet_ready_depth);
    s.fleet_ready_write_failures_mark_total = loadStat(&g_fleet_ready_write_failures_mark_total);
    s.fleet_ready_write_failures_clear_total = loadStat(&g_fleet_ready_write_failures_clear_total);
    s.fleet_ready_sweep_recoveries_total = loadStat(&g_fleet_ready_sweep_recoveries_total);
    return s;
}

/// Test-only reset for the lease-poll and readiness families, so a render test
/// starts from a known zero rather than inheriting another test's increments.
pub fn resetLeasePollMetricsForTest() void {
    g_lease_polls_total.store(0, .release); // safe because: single-threaded test reset
    g_lease_poll_candidates_scanned_total.store(0, .release);
    g_lease_poll_candidates_max.store(0, .release);
    g_lease_poll_db_roundtrips_total.store(0, .release);
    g_fleet_ready_depth.store(0, .release);
    g_fleet_ready_write_failures_mark_total.store(0, .release);
    g_fleet_ready_write_failures_clear_total.store(0, .release);
    g_fleet_ready_sweep_recoveries_total.store(0, .release);
}
