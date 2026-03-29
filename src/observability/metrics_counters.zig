//! In-process metrics registry exposed in Prometheus text format.

const std = @import("std");
const error_classify = @import("../reliability/error_classify.zig");

pub const DurationBuckets = [_]u64{ 1, 3, 5, 10, 30, 60, 120, 300 };

pub const HistogramSnapshot = struct {
    buckets: [DurationBuckets.len]u64 = [_]u64{0} ** DurationBuckets.len,
    count: u64 = 0,
    sum: u64 = 0,
};

pub const Snapshot = struct {
    runs_created_total: u64,
    runs_completed_total: u64,
    runs_blocked_total: u64,
    run_retries_total: u64,
    external_retries_total: u64,
    external_retries_rate_limited_total: u64,
    external_retries_timeout_total: u64,
    external_retries_context_exhausted_total: u64,
    external_retries_auth_total: u64,
    external_retries_invalid_request_total: u64,
    external_retries_server_error_total: u64,
    external_retries_unknown_total: u64,
    external_failures_total: u64,
    external_failures_rate_limited_total: u64,
    external_failures_timeout_total: u64,
    external_failures_context_exhausted_total: u64,
    external_failures_auth_total: u64,
    external_failures_invalid_request_total: u64,
    external_failures_server_error_total: u64,
    external_failures_unknown_total: u64,
    retry_after_hints_total: u64,
    worker_errors_total: u64,
    sandbox_shell_runs_total: u64,
    sandbox_host_runs_total: u64,
    sandbox_bubblewrap_runs_total: u64,
    sandbox_kill_switch_total: u64,
    sandbox_preflight_failures_total: u64,
    agent_echo_calls_total: u64,
    agent_scout_calls_total: u64,
    agent_warden_calls_total: u64,
    agent_tokens_total: u64,
    backoff_wait_ms_total: u64,
    rate_limit_wait_ms_total: u64,
    side_effect_outbox_enqueued_total: u64,
    side_effect_outbox_delivered_total: u64,
    side_effect_outbox_dead_letter_total: u64,
    api_backpressure_rejections_total: u64,
    api_in_flight_requests: u64,
    worker_in_flight_runs: u64,
    worker_allocator_leaks_total: u64,
    agent_score_computed_total: u64,
    agent_score_computed_unranked: u64,
    agent_score_computed_bronze: u64,
    agent_score_computed_silver: u64,
    agent_score_computed_gold: u64,
    agent_score_computed_elite: u64,
    agent_scoring_failed_total: u64,
    agent_score_latest: u64,
    otel_export_total: u64,
    otel_export_failed_total: u64,
    otel_last_success_at_ms: i64,
    orphan_runs_recovered_total: u64,
    orphan_no_agent_profile_total: u64,
    reconcile_running: u64,
    agent_duration_seconds: HistogramSnapshot,
    run_total_wall_seconds: HistogramSnapshot,
    agent_scoring_duration_ms: HistogramSnapshot,
};

var g_runs_created_total = std.atomic.Value(u64).init(0);
var g_runs_completed_total = std.atomic.Value(u64).init(0);
var g_runs_blocked_total = std.atomic.Value(u64).init(0);
var g_run_retries_total = std.atomic.Value(u64).init(0);
var g_external_retries_total = std.atomic.Value(u64).init(0);
var g_external_retries_rate_limited_total = std.atomic.Value(u64).init(0);
var g_external_retries_timeout_total = std.atomic.Value(u64).init(0);
var g_external_retries_context_exhausted_total = std.atomic.Value(u64).init(0);
var g_external_retries_auth_total = std.atomic.Value(u64).init(0);
var g_external_retries_invalid_request_total = std.atomic.Value(u64).init(0);
var g_external_retries_server_error_total = std.atomic.Value(u64).init(0);
var g_external_retries_unknown_total = std.atomic.Value(u64).init(0);
var g_external_failures_total = std.atomic.Value(u64).init(0);
var g_external_failures_rate_limited_total = std.atomic.Value(u64).init(0);
var g_external_failures_timeout_total = std.atomic.Value(u64).init(0);
var g_external_failures_context_exhausted_total = std.atomic.Value(u64).init(0);
var g_external_failures_auth_total = std.atomic.Value(u64).init(0);
var g_external_failures_invalid_request_total = std.atomic.Value(u64).init(0);
var g_external_failures_server_error_total = std.atomic.Value(u64).init(0);
var g_external_failures_unknown_total = std.atomic.Value(u64).init(0);
var g_retry_after_hints_total = std.atomic.Value(u64).init(0);
var g_worker_errors_total = std.atomic.Value(u64).init(0);
var g_sandbox_shell_runs_total = std.atomic.Value(u64).init(0);
var g_sandbox_host_runs_total = std.atomic.Value(u64).init(0);
var g_sandbox_bubblewrap_runs_total = std.atomic.Value(u64).init(0);
var g_sandbox_kill_switch_total = std.atomic.Value(u64).init(0);
var g_sandbox_preflight_failures_total = std.atomic.Value(u64).init(0);
var g_agent_echo_calls_total = std.atomic.Value(u64).init(0);
var g_agent_scout_calls_total = std.atomic.Value(u64).init(0);
var g_agent_warden_calls_total = std.atomic.Value(u64).init(0);
var g_agent_tokens_total = std.atomic.Value(u64).init(0);
var g_backoff_wait_ms_total = std.atomic.Value(u64).init(0);
var g_rate_limit_wait_ms_total = std.atomic.Value(u64).init(0);
var g_side_effect_outbox_enqueued_total = std.atomic.Value(u64).init(0);
var g_side_effect_outbox_delivered_total = std.atomic.Value(u64).init(0);
var g_side_effect_outbox_dead_letter_total = std.atomic.Value(u64).init(0);
var g_api_backpressure_rejections_total = std.atomic.Value(u64).init(0);
var g_api_in_flight_requests = std.atomic.Value(u64).init(0);
var g_worker_in_flight_runs = std.atomic.Value(u64).init(0);
var g_worker_allocator_leaks_total = std.atomic.Value(u64).init(0);
var g_agent_score_computed_total = std.atomic.Value(u64).init(0);
var g_agent_score_computed_unranked = std.atomic.Value(u64).init(0);
var g_agent_score_computed_bronze = std.atomic.Value(u64).init(0);
var g_agent_score_computed_silver = std.atomic.Value(u64).init(0);
var g_agent_score_computed_gold = std.atomic.Value(u64).init(0);
var g_agent_score_computed_elite = std.atomic.Value(u64).init(0);
var g_agent_scoring_failed_total = std.atomic.Value(u64).init(0);
var g_agent_score_latest = std.atomic.Value(u64).init(0);
var g_otel_export_total = std.atomic.Value(u64).init(0);
var g_otel_export_failed_total = std.atomic.Value(u64).init(0);
var g_otel_last_success_at_ms = std.atomic.Value(i64).init(0);
var g_orphan_runs_recovered_total = std.atomic.Value(u64).init(0);
var g_orphan_no_agent_profile_total = std.atomic.Value(u64).init(0);
var g_reconcile_running = std.atomic.Value(u64).init(0);
var g_histograms_mu: std.Thread.Mutex = .{};
var g_agent_duration_seconds = HistogramSnapshot{};
var g_run_total_wall_seconds = HistogramSnapshot{};
var g_agent_scoring_duration_ms = HistogramSnapshot{};

pub fn incRunsCreated() void {
    _ = g_runs_created_total.fetchAdd(1, .monotonic);
}

pub fn incRunsCompleted() void {
    _ = g_runs_completed_total.fetchAdd(1, .monotonic);
}

pub fn incRunsBlocked() void {
    _ = g_runs_blocked_total.fetchAdd(1, .monotonic);
}

pub fn incRunRetries() void {
    _ = g_run_retries_total.fetchAdd(1, .monotonic);
}

pub fn incExternalRetry(class: error_classify.ErrorClass) void {
    _ = g_external_retries_total.fetchAdd(1, .monotonic);
    incrementClassCounter(
        class,
        &g_external_retries_rate_limited_total,
        &g_external_retries_timeout_total,
        &g_external_retries_context_exhausted_total,
        &g_external_retries_auth_total,
        &g_external_retries_invalid_request_total,
        &g_external_retries_server_error_total,
        &g_external_retries_unknown_total,
    );
}

pub fn incExternalFailure(class: error_classify.ErrorClass) void {
    _ = g_external_failures_total.fetchAdd(1, .monotonic);
    incrementClassCounter(
        class,
        &g_external_failures_rate_limited_total,
        &g_external_failures_timeout_total,
        &g_external_failures_context_exhausted_total,
        &g_external_failures_auth_total,
        &g_external_failures_invalid_request_total,
        &g_external_failures_server_error_total,
        &g_external_failures_unknown_total,
    );
}

fn incrementClassCounter(
    class: error_classify.ErrorClass,
    rate_limited: *std.atomic.Value(u64),
    timeout: *std.atomic.Value(u64),
    context_exhausted: *std.atomic.Value(u64),
    auth: *std.atomic.Value(u64),
    invalid_request: *std.atomic.Value(u64),
    server_error: *std.atomic.Value(u64),
    unknown: *std.atomic.Value(u64),
) void {
    switch (class) {
        .rate_limited => _ = rate_limited.fetchAdd(1, .monotonic),
        .timeout => _ = timeout.fetchAdd(1, .monotonic),
        .context_exhausted => _ = context_exhausted.fetchAdd(1, .monotonic),
        .auth => _ = auth.fetchAdd(1, .monotonic),
        .invalid_request => _ = invalid_request.fetchAdd(1, .monotonic),
        .server_error => _ = server_error.fetchAdd(1, .monotonic),
        .unknown => _ = unknown.fetchAdd(1, .monotonic),
    }
}

pub fn incRetryAfterHintsApplied() void {
    _ = g_retry_after_hints_total.fetchAdd(1, .monotonic);
}

pub fn incWorkerErrors() void {
    _ = g_worker_errors_total.fetchAdd(1, .monotonic);
}

pub fn incSandboxShellRuns() void {
    _ = g_sandbox_shell_runs_total.fetchAdd(1, .monotonic);
}

pub fn incSandboxHostRuns() void {
    _ = g_sandbox_host_runs_total.fetchAdd(1, .monotonic);
}

pub fn incSandboxBubblewrapRuns() void {
    _ = g_sandbox_bubblewrap_runs_total.fetchAdd(1, .monotonic);
}

pub fn incSandboxKillSwitches() void {
    _ = g_sandbox_kill_switch_total.fetchAdd(1, .monotonic);
}

pub fn incSandboxPreflightFailures() void {
    _ = g_sandbox_preflight_failures_total.fetchAdd(1, .monotonic);
}

pub fn incAgentEchoCalls() void {
    _ = g_agent_echo_calls_total.fetchAdd(1, .monotonic);
}

pub fn incAgentScoutCalls() void {
    _ = g_agent_scout_calls_total.fetchAdd(1, .monotonic);
}

pub fn incAgentWardenCalls() void {
    _ = g_agent_warden_calls_total.fetchAdd(1, .monotonic);
}

pub fn addAgentTokens(tokens: u64) void {
    _ = g_agent_tokens_total.fetchAdd(tokens, .monotonic);
}

pub fn addBackoffWaitMs(ms: u64) void {
    _ = g_backoff_wait_ms_total.fetchAdd(ms, .monotonic);
}

pub fn addRateLimitWaitMs(ms: u64) void {
    _ = g_rate_limit_wait_ms_total.fetchAdd(ms, .monotonic);
}

pub fn incOutboxEnqueued() void {
    _ = g_side_effect_outbox_enqueued_total.fetchAdd(1, .monotonic);
}

pub fn incOutboxDelivered() void {
    _ = g_side_effect_outbox_delivered_total.fetchAdd(1, .monotonic);
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

pub fn setWorkerInFlightRuns(v: u32) void {
    g_worker_in_flight_runs.store(@as(u64, @intCast(v)), .release);
}

pub fn incWorkerAllocatorLeaks() void {
    _ = g_worker_allocator_leaks_total.fetchAdd(1, .monotonic);
}

pub fn incAgentScoreComputed(tier: anytype) void {
    _ = g_agent_score_computed_total.fetchAdd(1, .monotonic);
    switch (tier) {
        .unranked => _ = g_agent_score_computed_unranked.fetchAdd(1, .monotonic),
        .bronze => _ = g_agent_score_computed_bronze.fetchAdd(1, .monotonic),
        .silver => _ = g_agent_score_computed_silver.fetchAdd(1, .monotonic),
        .gold => _ = g_agent_score_computed_gold.fetchAdd(1, .monotonic),
        .elite => _ = g_agent_score_computed_elite.fetchAdd(1, .monotonic),
    }
}

pub fn incAgentScoringFailed() void {
    _ = g_agent_scoring_failed_total.fetchAdd(1, .monotonic);
}

pub fn setAgentScoreLatest(score: u8) void {
    g_agent_score_latest.store(@as(u64, score), .release);
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

pub fn incOrphanRunsRecovered() void {
    _ = g_orphan_runs_recovered_total.fetchAdd(1, .monotonic);
}

pub fn incOrphanNoAgentProfile() void {
    _ = g_orphan_no_agent_profile_total.fetchAdd(1, .monotonic);
}

pub fn setReconcileRunning(v: bool) void {
    g_reconcile_running.store(if (v) 1 else 0, .release);
}

pub fn observeAgentScoringDurationMs(ms: u64) void {
    g_histograms_mu.lock();
    defer g_histograms_mu.unlock();
    observeHistogram(&g_agent_scoring_duration_ms, ms);
}

pub fn observeAgentDurationSeconds(seconds: u64) void {
    g_histograms_mu.lock();
    defer g_histograms_mu.unlock();
    observeHistogram(&g_agent_duration_seconds, seconds);
}

pub fn observeRunTotalWallSeconds(seconds: u64) void {
    g_histograms_mu.lock();
    defer g_histograms_mu.unlock();
    observeHistogram(&g_run_total_wall_seconds, seconds);
}

fn observeHistogram(hist: *HistogramSnapshot, value_seconds: u64) void {
    hist.count += 1;
    hist.sum += value_seconds;
    for (DurationBuckets, 0..) |le, i| {
        if (value_seconds <= le) hist.buckets[i] += 1;
    }
}

pub fn snapshot() Snapshot {
    var s = Snapshot{
        .runs_created_total = g_runs_created_total.load(.acquire),
        .runs_completed_total = g_runs_completed_total.load(.acquire),
        .runs_blocked_total = g_runs_blocked_total.load(.acquire),
        .run_retries_total = g_run_retries_total.load(.acquire),
        .external_retries_total = g_external_retries_total.load(.acquire),
        .external_retries_rate_limited_total = g_external_retries_rate_limited_total.load(.acquire),
        .external_retries_timeout_total = g_external_retries_timeout_total.load(.acquire),
        .external_retries_context_exhausted_total = g_external_retries_context_exhausted_total.load(.acquire),
        .external_retries_auth_total = g_external_retries_auth_total.load(.acquire),
        .external_retries_invalid_request_total = g_external_retries_invalid_request_total.load(.acquire),
        .external_retries_server_error_total = g_external_retries_server_error_total.load(.acquire),
        .external_retries_unknown_total = g_external_retries_unknown_total.load(.acquire),
        .external_failures_total = g_external_failures_total.load(.acquire),
        .external_failures_rate_limited_total = g_external_failures_rate_limited_total.load(.acquire),
        .external_failures_timeout_total = g_external_failures_timeout_total.load(.acquire),
        .external_failures_context_exhausted_total = g_external_failures_context_exhausted_total.load(.acquire),
        .external_failures_auth_total = g_external_failures_auth_total.load(.acquire),
        .external_failures_invalid_request_total = g_external_failures_invalid_request_total.load(.acquire),
        .external_failures_server_error_total = g_external_failures_server_error_total.load(.acquire),
        .external_failures_unknown_total = g_external_failures_unknown_total.load(.acquire),
        .retry_after_hints_total = g_retry_after_hints_total.load(.acquire),
        .worker_errors_total = g_worker_errors_total.load(.acquire),
        .sandbox_shell_runs_total = g_sandbox_shell_runs_total.load(.acquire),
        .sandbox_host_runs_total = g_sandbox_host_runs_total.load(.acquire),
        .sandbox_bubblewrap_runs_total = g_sandbox_bubblewrap_runs_total.load(.acquire),
        .sandbox_kill_switch_total = g_sandbox_kill_switch_total.load(.acquire),
        .sandbox_preflight_failures_total = g_sandbox_preflight_failures_total.load(.acquire),
        .agent_echo_calls_total = g_agent_echo_calls_total.load(.acquire),
        .agent_scout_calls_total = g_agent_scout_calls_total.load(.acquire),
        .agent_warden_calls_total = g_agent_warden_calls_total.load(.acquire),
        .agent_tokens_total = g_agent_tokens_total.load(.acquire),
        .backoff_wait_ms_total = g_backoff_wait_ms_total.load(.acquire),
        .rate_limit_wait_ms_total = g_rate_limit_wait_ms_total.load(.acquire),
        .side_effect_outbox_enqueued_total = g_side_effect_outbox_enqueued_total.load(.acquire),
        .side_effect_outbox_delivered_total = g_side_effect_outbox_delivered_total.load(.acquire),
        .side_effect_outbox_dead_letter_total = g_side_effect_outbox_dead_letter_total.load(.acquire),
        .api_backpressure_rejections_total = g_api_backpressure_rejections_total.load(.acquire),
        .api_in_flight_requests = g_api_in_flight_requests.load(.acquire),
        .worker_in_flight_runs = g_worker_in_flight_runs.load(.acquire),
        .worker_allocator_leaks_total = g_worker_allocator_leaks_total.load(.acquire),
        .agent_score_computed_total = g_agent_score_computed_total.load(.acquire),
        .agent_score_computed_unranked = g_agent_score_computed_unranked.load(.acquire),
        .agent_score_computed_bronze = g_agent_score_computed_bronze.load(.acquire),
        .agent_score_computed_silver = g_agent_score_computed_silver.load(.acquire),
        .agent_score_computed_gold = g_agent_score_computed_gold.load(.acquire),
        .agent_score_computed_elite = g_agent_score_computed_elite.load(.acquire),
        .agent_scoring_failed_total = g_agent_scoring_failed_total.load(.acquire),
        .agent_score_latest = g_agent_score_latest.load(.acquire),
        .otel_export_total = g_otel_export_total.load(.acquire),
        .otel_export_failed_total = g_otel_export_failed_total.load(.acquire),
        .otel_last_success_at_ms = g_otel_last_success_at_ms.load(.acquire),
        .orphan_runs_recovered_total = g_orphan_runs_recovered_total.load(.acquire),
        .orphan_no_agent_profile_total = g_orphan_no_agent_profile_total.load(.acquire),
        .reconcile_running = g_reconcile_running.load(.acquire),
        .agent_duration_seconds = .{},
        .run_total_wall_seconds = .{},
        .agent_scoring_duration_ms = .{},
    };
    g_histograms_mu.lock();
    defer g_histograms_mu.unlock();
    s.agent_duration_seconds = g_agent_duration_seconds;
    s.run_total_wall_seconds = g_run_total_wall_seconds;
    s.agent_scoring_duration_ms = g_agent_scoring_duration_ms;
    return s;
}
