//! In-process metrics registry exposed in Prometheus text format.

const std = @import("std");
const me = @import("metrics_external.zig");

pub const DurationBuckets = [_]u64{ 1, 3, 5, 10, 30, 60, 120, 300 };
pub const HistogramSnapshot = struct {
    buckets: [DurationBuckets.len]u64 = [_]u64{0} ** DurationBuckets.len,
    count: u64 = 0,
    sum: u64 = 0,
};

const zombie_metrics = @import("metrics_zombie.zig");
pub const ZombieDurationBucketsMs = zombie_metrics.ZombieDurationBucketsMs;
pub const ZombieHistogramSnapshot = zombie_metrics.ZombieHistogramSnapshot;
pub const incZombiesTriggered = zombie_metrics.incZombiesTriggered;
pub const incZombiesCompleted = zombie_metrics.incZombiesCompleted;
pub const incZombiesFailed = zombie_metrics.incZombiesFailed;
pub const addZombieTokens = zombie_metrics.addZombieTokens;
pub const observeZombieExecutionSeconds = zombie_metrics.observeZombieExecutionSeconds;

pub const Snapshot = struct {
    // External retry/failure fields — defaults to 0, filled by me.snapshotExternalFields().
    external_retries_total: u64 = 0,
    external_retries_rate_limited_total: u64 = 0,
    external_retries_timeout_total: u64 = 0,
    external_retries_context_exhausted_total: u64 = 0,
    external_retries_auth_total: u64 = 0,
    external_retries_invalid_request_total: u64 = 0,
    external_retries_server_error_total: u64 = 0,
    external_retries_unknown_total: u64 = 0,
    external_failures_total: u64 = 0,
    external_failures_rate_limited_total: u64 = 0,
    external_failures_timeout_total: u64 = 0,
    external_failures_context_exhausted_total: u64 = 0,
    external_failures_auth_total: u64 = 0,
    external_failures_invalid_request_total: u64 = 0,
    external_failures_server_error_total: u64 = 0,
    external_failures_unknown_total: u64 = 0,
    retry_after_hints_total: u64 = 0,
    agent_tokens_total: u64,
    backoff_wait_ms_total: u64,
    side_effect_outbox_dead_letter_total: u64,
    api_backpressure_rejections_total: u64,
    api_in_flight_requests: u64,
    gate_repair_exhausted_total: u64,
    // M17_001 §1.3: per-limit-type termination counters
    run_limit_token_budget_exceeded_total: u64,
    run_limit_wall_time_exceeded_total: u64,
    run_limit_repair_loops_exhausted_total: u64,
    otel_export_total: u64,
    otel_export_failed_total: u64,
    otel_last_success_at_ms: i64,
    reconcile_running: u64,
    agent_duration_seconds: HistogramSnapshot,
    // M15_002 §1.0 — zombie counters + wall-time histogram
    zombie_triggered_total: u64 = 0,
    zombie_completed_total: u64 = 0,
    zombie_failed_total: u64 = 0,
    zombie_tokens_total: u64 = 0,
    zombie_execution_seconds: ZombieHistogramSnapshot = .{},
    // Signup funnel counters.
    signup_bootstrapped_total: u64 = 0,
    signup_replayed_total: u64 = 0,
    signup_failed_bad_sig_total: u64 = 0,
    signup_failed_stale_ts_total: u64 = 0,
    signup_failed_missing_email_total: u64 = 0,
    signup_failed_db_error_total: u64 = 0,
    signup_failed_pool_unavailable_total: u64 = 0,
};

var g_agent_tokens_total = std.atomic.Value(u64).init(0);
var g_backoff_wait_ms_total = std.atomic.Value(u64).init(0);
var g_side_effect_outbox_dead_letter_total = std.atomic.Value(u64).init(0);
var g_api_backpressure_rejections_total = std.atomic.Value(u64).init(0);
var g_api_in_flight_requests = std.atomic.Value(u64).init(0);
var g_gate_repair_exhausted_total = std.atomic.Value(u64).init(0);
var g_run_limit_token_budget_exceeded_total = std.atomic.Value(u64).init(0);
var g_run_limit_wall_time_exceeded_total = std.atomic.Value(u64).init(0);
var g_run_limit_repair_loops_exhausted_total = std.atomic.Value(u64).init(0);
var g_otel_export_total = std.atomic.Value(u64).init(0);
var g_otel_export_failed_total = std.atomic.Value(u64).init(0);
var g_otel_last_success_at_ms = std.atomic.Value(i64).init(0);
var g_reconcile_running = std.atomic.Value(u64).init(0);
var g_signup_bootstrapped_total = std.atomic.Value(u64).init(0);
var g_signup_replayed_total = std.atomic.Value(u64).init(0);
var g_signup_failed_bad_sig_total = std.atomic.Value(u64).init(0);
var g_signup_failed_stale_ts_total = std.atomic.Value(u64).init(0);
var g_signup_failed_missing_email_total = std.atomic.Value(u64).init(0);
var g_signup_failed_db_error_total = std.atomic.Value(u64).init(0);
var g_signup_failed_pool_unavailable_total = std.atomic.Value(u64).init(0);
const mh = @import("metrics_histograms.zig");
pub const incExternalRetry = me.incExternalRetry;
pub const incExternalFailure = me.incExternalFailure;
pub const incRetryAfterHintsApplied = me.incRetryAfterHintsApplied;

pub fn addAgentTokens(tokens: u64) void {
    _ = g_agent_tokens_total.fetchAdd(tokens, .monotonic);
}

pub fn addBackoffWaitMs(ms: u64) void {
    _ = g_backoff_wait_ms_total.fetchAdd(ms, .monotonic);
}

pub fn incOutboxDeadLetter() void {
    _ = g_side_effect_outbox_dead_letter_total.fetchAdd(1, .monotonic);
}

pub fn incApiBackpressureRejections() void {
    _ = g_api_backpressure_rejections_total.fetchAdd(1, .monotonic);
}

pub fn setApiInFlightRequests(v: u32) void {
    g_api_in_flight_requests.store(@as(u64, @intCast(v)), .release);
}

pub fn incGateRepairExhausted() void {
    _ = g_gate_repair_exhausted_total.fetchAdd(1, .monotonic);
}
pub fn incRunLimitTokenBudgetExceeded() void {
    _ = g_run_limit_token_budget_exceeded_total.fetchAdd(1, .monotonic);
}
pub fn incRunLimitWallTimeExceeded() void {
    _ = g_run_limit_wall_time_exceeded_total.fetchAdd(1, .monotonic);
}
pub fn incRunLimitRepairLoopsExhausted() void {
    _ = g_run_limit_repair_loops_exhausted_total.fetchAdd(1, .monotonic);
}
pub fn incOtelExportTotal() void {
    _ = g_otel_export_total.fetchAdd(1, .monotonic);
}
pub fn incOtelExportFailed() void {
    _ = g_otel_export_failed_total.fetchAdd(1, .monotonic);
}
pub fn setOtelLastSuccessAtMs(ms: i64) void {
    g_otel_last_success_at_ms.store(ms, .release);
}

pub fn setReconcileRunning(v: bool) void {
    g_reconcile_running.store(if (v) 1 else 0, .release);
}

// Signup funnel counters. Failure reasons enumerated so a single Prometheus
// query can answer "how many signups failed for reason X over Y?"
pub const SignupFailReason = enum { bad_sig, stale_ts, missing_email, db_error, pool_unavailable };

pub fn incSignupBootstrapped() void {
    _ = g_signup_bootstrapped_total.fetchAdd(1, .monotonic);
}
pub fn incSignupReplayed() void {
    _ = g_signup_replayed_total.fetchAdd(1, .monotonic);
}
pub fn incSignupFailed(reason: SignupFailReason) void {
    const slot = switch (reason) {
        .bad_sig => &g_signup_failed_bad_sig_total,
        .stale_ts => &g_signup_failed_stale_ts_total,
        .missing_email => &g_signup_failed_missing_email_total,
        .db_error => &g_signup_failed_db_error_total,
        .pool_unavailable => &g_signup_failed_pool_unavailable_total,
    };
    _ = slot.fetchAdd(1, .monotonic);
}

pub const observeAgentDurationSeconds = mh.observeAgentDurationSeconds;

pub fn snapshot() Snapshot {
    var s = Snapshot{
        .agent_tokens_total = g_agent_tokens_total.load(.acquire),
        .backoff_wait_ms_total = g_backoff_wait_ms_total.load(.acquire),
        .side_effect_outbox_dead_letter_total = g_side_effect_outbox_dead_letter_total.load(.acquire),
        .api_backpressure_rejections_total = g_api_backpressure_rejections_total.load(.acquire),
        .api_in_flight_requests = g_api_in_flight_requests.load(.acquire),
        .gate_repair_exhausted_total = g_gate_repair_exhausted_total.load(.acquire),
        .run_limit_token_budget_exceeded_total = g_run_limit_token_budget_exceeded_total.load(.acquire),
        .run_limit_wall_time_exceeded_total = g_run_limit_wall_time_exceeded_total.load(.acquire),
        .run_limit_repair_loops_exhausted_total = g_run_limit_repair_loops_exhausted_total.load(.acquire),
        .otel_export_total = g_otel_export_total.load(.acquire),
        .otel_export_failed_total = g_otel_export_failed_total.load(.acquire),
        .otel_last_success_at_ms = g_otel_last_success_at_ms.load(.acquire),
        .reconcile_running = g_reconcile_running.load(.acquire),
        .agent_duration_seconds = .{},
    };
    const ext = me.snapshotExternalFields();
    s.external_retries_total = ext.external_retries_total;
    s.external_retries_rate_limited_total = ext.external_retries_rate_limited_total;
    s.external_retries_timeout_total = ext.external_retries_timeout_total;
    s.external_retries_context_exhausted_total = ext.external_retries_context_exhausted_total;
    s.external_retries_auth_total = ext.external_retries_auth_total;
    s.external_retries_invalid_request_total = ext.external_retries_invalid_request_total;
    s.external_retries_server_error_total = ext.external_retries_server_error_total;
    s.external_retries_unknown_total = ext.external_retries_unknown_total;
    s.external_failures_total = ext.external_failures_total;
    s.external_failures_rate_limited_total = ext.external_failures_rate_limited_total;
    s.external_failures_timeout_total = ext.external_failures_timeout_total;
    s.external_failures_context_exhausted_total = ext.external_failures_context_exhausted_total;
    s.external_failures_auth_total = ext.external_failures_auth_total;
    s.external_failures_invalid_request_total = ext.external_failures_invalid_request_total;
    s.external_failures_server_error_total = ext.external_failures_server_error_total;
    s.external_failures_unknown_total = ext.external_failures_unknown_total;
    s.retry_after_hints_total = ext.retry_after_hints_total;
    mh.snapshotHistograms(&s);
    const zf = zombie_metrics.snapshotZombieFields();
    s.zombie_triggered_total = zf.zombie_triggered_total;
    s.zombie_completed_total = zf.zombie_completed_total;
    s.zombie_failed_total = zf.zombie_failed_total;
    s.zombie_tokens_total = zf.zombie_tokens_total;
    s.zombie_execution_seconds = zf.zombie_execution_seconds;
    s.signup_bootstrapped_total = g_signup_bootstrapped_total.load(.acquire);
    s.signup_replayed_total = g_signup_replayed_total.load(.acquire);
    s.signup_failed_bad_sig_total = g_signup_failed_bad_sig_total.load(.acquire);
    s.signup_failed_stale_ts_total = g_signup_failed_stale_ts_total.load(.acquire);
    s.signup_failed_missing_email_total = g_signup_failed_missing_email_total.load(.acquire);
    s.signup_failed_db_error_total = g_signup_failed_db_error_total.load(.acquire);
    s.signup_failed_pool_unavailable_total = g_signup_failed_pool_unavailable_total.load(.acquire);
    return s;
}
