//! In-process metrics registry exposed in Prometheus text format.

const std = @import("std");

const agent_metrics = @import("metrics_agent.zig");
pub const incAgentsTriggered = agent_metrics.incAgentsTriggered;

pub const Snapshot = struct {
    api_backpressure_rejections_total: u64,
    api_in_flight_requests: u64,
    sse_backpressure_rejections_total: u64,
    sse_in_flight_streams: u64,
    sse_dropped_frames_total: u64,
    sse_hub_reconnects_total: u64,
    agent_triggered_total: u64 = 0,
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
    s.agent_triggered_total = agent_metrics.snapshotAgentFields().agent_triggered_total;
    s.signup_bootstrapped_total = loadStat(&g_signup_bootstrapped_total);
    s.signup_replayed_total = loadStat(&g_signup_replayed_total);
    s.signup_failed_bad_sig_total = loadStat(&g_signup_failed_bad_sig_total);
    s.signup_failed_stale_ts_total = loadStat(&g_signup_failed_stale_ts_total);
    s.signup_failed_missing_email_total = loadStat(&g_signup_failed_missing_email_total);
    s.signup_failed_db_error_total = loadStat(&g_signup_failed_db_error_total);
    s.signup_failed_pool_unavailable_total = loadStat(&g_signup_failed_pool_unavailable_total);
    s.signup_failed_metadata_writeback_total = loadStat(&g_signup_failed_metadata_writeback_total);
    return s;
}
