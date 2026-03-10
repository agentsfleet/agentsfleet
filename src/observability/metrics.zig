//! In-process metrics registry exposed in Prometheus text format.

const std = @import("std");
const error_classify = @import("../reliability/error_classify.zig");

const DurationBuckets = [_]u64{ 1, 3, 5, 10, 30, 60, 120, 300 };

const HistogramSnapshot = struct {
    buckets: [DurationBuckets.len]u64 = [_]u64{0} ** DurationBuckets.len,
    count: u64 = 0,
    sum: u64 = 0,
};

const Snapshot = struct {
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
    agent_duration_seconds: HistogramSnapshot,
    run_total_wall_seconds: HistogramSnapshot,
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
var g_histograms_mu: std.Thread.Mutex = .{};
var g_agent_duration_seconds = HistogramSnapshot{};
var g_run_total_wall_seconds = HistogramSnapshot{};

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

fn snapshot() Snapshot {
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
        .agent_duration_seconds = .{},
        .run_total_wall_seconds = .{},
    };
    g_histograms_mu.lock();
    defer g_histograms_mu.unlock();
    s.agent_duration_seconds = g_agent_duration_seconds;
    s.run_total_wall_seconds = g_run_total_wall_seconds;
    return s;
}

fn appendDurationHistogram(
    writer: anytype,
    name: []const u8,
    help: []const u8,
    hist: HistogramSnapshot,
) !void {
    try writer.print("# HELP {s} {s}\n", .{ name, help });
    try writer.print("# TYPE {s} histogram\n", .{name});
    for (DurationBuckets, 0..) |le, i| {
        try writer.print("{s}_bucket{{le=\"{d}\"}} {d}\n", .{
            name,
            le,
            hist.buckets[i],
        });
    }
    try writer.print("{s}_bucket{{le=\"+Inf\"}} {d}\n", .{ name, hist.count });
    try writer.print("{s}_sum {d}\n", .{ name, hist.sum });
    try writer.print("{s}_count {d}\n", .{ name, hist.count });
}

fn appendMetric(
    writer: anytype,
    name: []const u8,
    metric_type: []const u8,
    help: []const u8,
    value: anytype,
) !void {
    try writer.print("# HELP {s} {s}\n", .{ name, help });
    try writer.print("# TYPE {s} {s}\n", .{ name, metric_type });
    try writer.print("{s} {d}\n", .{ name, value });
}

pub fn renderPrometheus(
    alloc: std.mem.Allocator,
    worker_running: bool,
    queue_depth: ?i64,
    oldest_queued_age_ms: ?i64,
) ![]u8 {
    const s = snapshot();
    const worker_running_gauge: u8 = if (worker_running) 1 else 0;
    const queue_depth_gauge: i64 = queue_depth orelse 0;
    const oldest_age_gauge: i64 = oldest_queued_age_ms orelse 0;

    var out: std.ArrayList(u8) = .{};
    errdefer out.deinit(alloc);
    const writer = out.writer(alloc);

    try appendMetric(writer, "zombie_runs_created_total", "counter", "Total runs accepted by API.", s.runs_created_total);
    try appendMetric(writer, "zombie_runs_completed_total", "counter", "Total runs completed successfully.", s.runs_completed_total);
    try appendMetric(writer, "zombie_runs_blocked_total", "counter", "Total runs that ended blocked.", s.runs_blocked_total);
    try appendMetric(writer, "zombie_run_retries_total", "counter", "Total retry attempts across runs.", s.run_retries_total);
    try appendMetric(writer, "zombie_external_retries_total", "counter", "Total retry attempts inside external side-effect wrappers.", s.external_retries_total);
    try appendMetric(writer, "zombie_external_retries_rate_limited_total", "counter", "External retries classified as rate-limited.", s.external_retries_rate_limited_total);
    try appendMetric(writer, "zombie_external_retries_timeout_total", "counter", "External retries classified as timeout.", s.external_retries_timeout_total);
    try appendMetric(writer, "zombie_external_retries_context_exhausted_total", "counter", "External retries classified as context exhausted.", s.external_retries_context_exhausted_total);
    try appendMetric(writer, "zombie_external_retries_auth_total", "counter", "External retries classified as auth.", s.external_retries_auth_total);
    try appendMetric(writer, "zombie_external_retries_invalid_request_total", "counter", "External retries classified as invalid request.", s.external_retries_invalid_request_total);
    try appendMetric(writer, "zombie_external_retries_server_error_total", "counter", "External retries classified as server error.", s.external_retries_server_error_total);
    try appendMetric(writer, "zombie_external_retries_unknown_total", "counter", "External retries classified as unknown.", s.external_retries_unknown_total);
    try appendMetric(writer, "zombie_external_failures_total", "counter", "External calls that exited wrappers as classified failures.", s.external_failures_total);
    try appendMetric(writer, "zombie_external_failures_rate_limited_total", "counter", "External failures classified as rate-limited.", s.external_failures_rate_limited_total);
    try appendMetric(writer, "zombie_external_failures_timeout_total", "counter", "External failures classified as timeout.", s.external_failures_timeout_total);
    try appendMetric(writer, "zombie_external_failures_context_exhausted_total", "counter", "External failures classified as context exhausted.", s.external_failures_context_exhausted_total);
    try appendMetric(writer, "zombie_external_failures_auth_total", "counter", "External failures classified as auth.", s.external_failures_auth_total);
    try appendMetric(writer, "zombie_external_failures_invalid_request_total", "counter", "External failures classified as invalid request.", s.external_failures_invalid_request_total);
    try appendMetric(writer, "zombie_external_failures_server_error_total", "counter", "External failures classified as server error.", s.external_failures_server_error_total);
    try appendMetric(writer, "zombie_external_failures_unknown_total", "counter", "External failures classified as unknown.", s.external_failures_unknown_total);
    try appendMetric(writer, "zombie_retry_after_hints_total", "counter", "Retry attempts that used Retry-After guidance.", s.retry_after_hints_total);
    try appendMetric(writer, "zombie_worker_errors_total", "counter", "Total worker loop errors.", s.worker_errors_total);
    try appendMetric(writer, "zombie_agent_echo_calls_total", "counter", "Total Echo agent invocations.", s.agent_echo_calls_total);
    try appendMetric(writer, "zombie_agent_scout_calls_total", "counter", "Total Scout agent invocations.", s.agent_scout_calls_total);
    try appendMetric(writer, "zombie_agent_warden_calls_total", "counter", "Total Warden agent invocations.", s.agent_warden_calls_total);
    try appendMetric(writer, "zombie_agent_tokens_total", "counter", "Total tokens consumed by agent calls.", s.agent_tokens_total);
    try appendMetric(writer, "zombie_backoff_wait_ms_total", "counter", "Total backoff wait time in milliseconds.", s.backoff_wait_ms_total);
    try appendMetric(writer, "zombie_rate_limit_wait_ms_total", "counter", "Total wait time due to rate limiting in milliseconds.", s.rate_limit_wait_ms_total);
    try appendMetric(writer, "zombie_side_effect_outbox_enqueued_total", "counter", "Total side-effect outbox entries enqueued.", s.side_effect_outbox_enqueued_total);
    try appendMetric(writer, "zombie_side_effect_outbox_delivered_total", "counter", "Total side-effect outbox entries marked delivered.", s.side_effect_outbox_delivered_total);
    try appendMetric(writer, "zombie_side_effect_outbox_dead_letter_total", "counter", "Total side-effect outbox entries dead-lettered by reconciliation.", s.side_effect_outbox_dead_letter_total);
    try appendMetric(writer, "zombie_api_backpressure_rejections_total", "counter", "Total API requests rejected by in-flight backpressure guard.", s.api_backpressure_rejections_total);
    try appendMetric(writer, "zombie_api_in_flight_requests", "gauge", "Current in-flight API requests protected by backpressure guard.", s.api_in_flight_requests);
    try appendMetric(writer, "zombie_worker_in_flight_runs", "gauge", "Current in-flight runs across worker threads.", s.worker_in_flight_runs);
    try appendMetric(writer, "zombie_worker_allocator_leaks_total", "counter", "Total worker allocator leak detections on teardown.", s.worker_allocator_leaks_total);
    try appendMetric(writer, "zombie_worker_running", "gauge", "Worker liveness gauge (1 running, 0 stopped).", worker_running_gauge);
    try appendMetric(writer, "zombie_queue_depth", "gauge", "Current queued runs in SPEC_QUEUED.", queue_depth_gauge);
    try appendMetric(writer, "zombie_oldest_queued_age_ms", "gauge", "Oldest queued run age in milliseconds.", oldest_age_gauge);

    try appendDurationHistogram(
        writer,
        "zombie_agent_duration_seconds",
        "Duration of individual agent calls in seconds.",
        s.agent_duration_seconds,
    );
    try appendDurationHistogram(
        writer,
        "zombie_run_total_wall_seconds",
        "End-to-end run wall-clock duration in seconds.",
        s.run_total_wall_seconds,
    );
    try writer.writeAll("\n");

    return out.toOwnedSlice(alloc);
}

test "prometheus render includes key metrics" {
    const alloc = std.testing.allocator;
    const body = try renderPrometheus(alloc, true, 3, 1200);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_runs_created_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_worker_running 1"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_queue_depth 3"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_external_retries_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_external_failures_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_rate_limit_wait_ms_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_side_effect_outbox_enqueued_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_side_effect_outbox_dead_letter_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_backpressure_rejections_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_in_flight_requests"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_agent_duration_seconds_bucket"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_run_total_wall_seconds_bucket"));
}

test "integration: outbox metric counters are exposed in prometheus output" {
    const alloc = std.testing.allocator;
    incOutboxEnqueued();
    incOutboxDelivered();
    incOutboxDeadLetter();

    const body = try renderPrometheus(alloc, false, 0, 0);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_side_effect_outbox_enqueued_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_side_effect_outbox_delivered_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_side_effect_outbox_dead_letter_total"));
}

test "integration: worker guardrail metrics are exposed in prometheus output" {
    const alloc = std.testing.allocator;
    setWorkerInFlightRuns(2);
    incWorkerAllocatorLeaks();

    const body = try renderPrometheus(alloc, true, 0, 0);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_worker_in_flight_runs 2"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_worker_allocator_leaks_total"));
}

test "integration: api throughput guardrail metrics are exposed in prometheus output" {
    const alloc = std.testing.allocator;
    setApiInFlightRequests(3);
    incApiBackpressureRejections();

    const body = try renderPrometheus(alloc, true, 0, 0);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_in_flight_requests 3"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_backpressure_rejections_total"));
}
