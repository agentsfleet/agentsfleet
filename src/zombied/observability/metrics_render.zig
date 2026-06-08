//! Prometheus text rendering for metrics_counters state.

const std = @import("std");
const mc = @import("metrics_counters.zig");
const mw = @import("metrics_workspace.zig");
const mr = @import("metrics_runner.zig");
const mrp = @import("metrics_redis_pool.zig");
const MS_PER_SECOND = 1000.0;

const S_TYPE_S_S_N = "# TYPE {s} {s}\n";
const S_REASON = "reason";
const S_COUNTER = "counter";
const S_HELP_S_S_N = "# HELP {s} {s}\n";
const S_GAUGE = "gauge";

fn appendDurationHistogram(
    writer: anytype,
    name: []const u8,
    help: []const u8,
    hist: mc.HistogramSnapshot,
) !void {
    try writer.print(S_HELP_S_S_N, .{ name, help });
    try writer.print("# TYPE {s} histogram\n", .{name});
    for (mc.DurationBuckets, 0..) |le, i| {
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
    try writer.print(S_HELP_S_S_N, .{ name, help });
    try writer.print(S_TYPE_S_S_N, .{ name, metric_type });
    try writer.print("{s} {d}\n", .{ name, value });
}

const LabeledSample = struct {
    label_value: []const u8,
    value: u64,
};

/// Emit one metric family with multiple label series: a single `# HELP` +
/// `# TYPE` block (bare metric name, per Prometheus exposition spec) followed
/// by one value line per series. Use for counters/gauges that vary by a
/// single label (e.g. `reason="bad_sig"`) — embedding the label in the HELP
/// or TYPE lines breaks strict scrapers like `promtool check metrics`.
fn appendLabeledFamily(
    writer: anytype,
    name: []const u8,
    metric_type: []const u8,
    help: []const u8,
    label_name: []const u8,
    samples: []const LabeledSample,
) !void {
    try writer.print(S_HELP_S_S_N, .{ name, help });
    try writer.print(S_TYPE_S_S_N, .{ name, metric_type });
    for (samples) |sample| {
        try writer.print("{s}{{{s}=\"{s}\"}} {d}\n", .{ name, label_name, sample.label_value, sample.value });
    }
}

pub fn renderPrometheus(
    alloc: std.mem.Allocator,
    worker_running: bool,
) ![]u8 {
    const s = mc.snapshot();
    const worker_running_gauge: u8 = if (worker_running) 1 else 0;

    var aw: std.Io.Writer.Allocating = .init(alloc);
    errdefer aw.deinit();
    const writer = &aw.writer;

    try appendMetric(writer, "zombie_external_retries_total", S_COUNTER, "Total retry attempts inside external side-effect wrappers.", s.external_retries_total);
    try appendMetric(writer, "zombie_external_retries_rate_limited_total", S_COUNTER, "External retries classified as rate-limited.", s.external_retries_rate_limited_total);
    try appendMetric(writer, "zombie_external_retries_timeout_total", S_COUNTER, "External retries classified as timeout.", s.external_retries_timeout_total);
    try appendMetric(writer, "zombie_external_retries_context_exhausted_total", S_COUNTER, "External retries classified as context exhausted.", s.external_retries_context_exhausted_total);
    try appendMetric(writer, "zombie_external_retries_auth_total", S_COUNTER, "External retries classified as auth.", s.external_retries_auth_total);
    try appendMetric(writer, "zombie_external_retries_invalid_request_total", S_COUNTER, "External retries classified as invalid request.", s.external_retries_invalid_request_total);
    try appendMetric(writer, "zombie_external_retries_server_error_total", S_COUNTER, "External retries classified as server error.", s.external_retries_server_error_total);
    try appendMetric(writer, "zombie_external_retries_unknown_total", S_COUNTER, "External retries classified as unknown.", s.external_retries_unknown_total);
    try appendMetric(writer, "zombie_external_failures_total", S_COUNTER, "External calls that exited wrappers as classified failures.", s.external_failures_total);
    try appendMetric(writer, "zombie_external_failures_rate_limited_total", S_COUNTER, "External failures classified as rate-limited.", s.external_failures_rate_limited_total);
    try appendMetric(writer, "zombie_external_failures_timeout_total", S_COUNTER, "External failures classified as timeout.", s.external_failures_timeout_total);
    try appendMetric(writer, "zombie_external_failures_context_exhausted_total", S_COUNTER, "External failures classified as context exhausted.", s.external_failures_context_exhausted_total);
    try appendMetric(writer, "zombie_external_failures_auth_total", S_COUNTER, "External failures classified as auth.", s.external_failures_auth_total);
    try appendMetric(writer, "zombie_external_failures_invalid_request_total", S_COUNTER, "External failures classified as invalid request.", s.external_failures_invalid_request_total);
    try appendMetric(writer, "zombie_external_failures_server_error_total", S_COUNTER, "External failures classified as server error.", s.external_failures_server_error_total);
    try appendMetric(writer, "zombie_external_failures_unknown_total", S_COUNTER, "External failures classified as unknown.", s.external_failures_unknown_total);
    try appendMetric(writer, "zombie_retry_after_hints_total", S_COUNTER, "Retry attempts that used Retry-After guidance.", s.retry_after_hints_total);
    try appendMetric(writer, "zombie_agent_tokens_total", S_COUNTER, "Total tokens consumed by agent calls.", s.agent_tokens_total);
    try appendMetric(writer, "zombie_backoff_wait_ms_total", S_COUNTER, "Total backoff wait time in milliseconds.", s.backoff_wait_ms_total);
    try appendMetric(writer, "zombie_api_backpressure_rejections_total", S_COUNTER, "Total API requests rejected by in-flight backpressure guard.", s.api_backpressure_rejections_total);
    try appendMetric(writer, "zombie_api_in_flight_requests", S_GAUGE, "Current in-flight API requests protected by backpressure guard.", s.api_in_flight_requests);
    try appendMetric(writer, "zombie_worker_running", S_GAUGE, "Worker liveness gauge (1 running, 0 stopped).", worker_running_gauge);

    try appendMetric(writer, "zombie_gate_repair_exhausted_total", S_COUNTER, "Total gate repair exhaustions (max loops reached).", s.gate_repair_exhausted_total);
    // Per-limit-type counters as a single labelled family (was three
    // per-series HELP/TYPE blocks, which strict scrapers reject).
    try appendLabeledFamily(
        writer,
        "zombied_run_limit_exceeded_total",
        S_COUNTER,
        "Runs terminated by a resource limit, labelled by which limit tripped.",
        S_REASON,
        &.{
            .{ .label_value = "token_budget", .value = s.run_limit_token_budget_exceeded_total },
            .{ .label_value = "wall_time", .value = s.run_limit_wall_time_exceeded_total },
            .{ .label_value = "repair_loops", .value = s.run_limit_repair_loops_exhausted_total },
        },
    );
    // Signup funnel counters.
    try appendMetric(writer, "zombie_signup_bootstrapped_total", S_COUNTER, "Clerk webhooks that provisioned a fresh personal account.", s.signup_bootstrapped_total);
    try appendMetric(writer, "zombie_signup_replayed_total", S_COUNTER, "Clerk webhooks that matched an existing account (idempotent replay).", s.signup_replayed_total);
    try appendLabeledFamily(
        writer,
        "zombie_signup_failed_total",
        S_COUNTER,
        "Signup webhooks that were rejected, labelled by rejection reason.",
        S_REASON,
        &.{
            .{ .label_value = "bad_sig", .value = s.signup_failed_bad_sig_total },
            .{ .label_value = "stale_ts", .value = s.signup_failed_stale_ts_total },
            .{ .label_value = "missing_email", .value = s.signup_failed_missing_email_total },
            .{ .label_value = "db_error", .value = s.signup_failed_db_error_total },
            .{ .label_value = "pool_unavailable", .value = s.signup_failed_pool_unavailable_total },
            .{ .label_value = "metadata_writeback", .value = s.signup_failed_metadata_writeback_total },
        },
    );

    try appendDurationHistogram(
        writer,
        "zombie_agent_duration_seconds",
        "Duration of individual agent calls in seconds.",
        s.agent_duration_seconds,
    );

    // Per-execution series were emitted here until the M80 cutover moved
    // execution to the runner. The runner exposes its own engine metrics;
    // zombied no longer renders an execution block.

    // M15_002: zombie execution counters + wall-time histogram.
    try appendMetric(writer, "zombie_triggered_total", S_COUNTER, "Total zombie webhook triggers accepted.", s.zombie_triggered_total);
    try appendMetric(writer, "zombie_completed_total", S_COUNTER, "Total zombie events delivered successfully.", s.zombie_completed_total);
    try appendMetric(writer, "zombie_failed_total", S_COUNTER, "Total zombie event delivery failures.", s.zombie_failed_total);
    try appendMetric(writer, "zombie_tokens_total", S_COUNTER, "Total tokens consumed across zombie deliveries.", s.zombie_tokens_total);
    {
        const zh = s.zombie_execution_seconds;
        try writer.print("# HELP zombie_execution_seconds Zombie event end-to-end wall-time in seconds.\n", .{});
        try writer.print("# TYPE zombie_execution_seconds histogram\n", .{});
        // Buckets stored as ms; emit le and sum as fractional seconds (Prometheus base unit).
        for (mc.ZombieDurationBucketsMs, 0..) |le_ms, i| {
            try writer.print("zombie_execution_seconds_bucket{{le=\"{d:.3}\"}} {d}\n", .{ @as(f64, @floatFromInt(le_ms)) / MS_PER_SECOND, zh.buckets[i] });
        }
        try writer.print("zombie_execution_seconds_bucket{{le=\"+Inf\"}} {d}\n", .{zh.count});
        try writer.print("zombie_execution_seconds_sum {d:.3}\n", .{@as(f64, @floatFromInt(zh.sum)) / MS_PER_SECOND});
        try writer.print("zombie_execution_seconds_count {d}\n", .{zh.count});
    }

    // Redis request-path pool — emitted only when a Pool has been registered
    // (early-boot scrapes pre-registration emit no lines; downstream scrapers
    // treat absent series as zero, matching the no-pool-yet reality).
    if (mrp.snapshot()) |rps| {
        try appendMetric(writer, "zombie_redis_pool_active", S_GAUGE, "Pooled Redis connections currently leased to a caller.", rps.active);
        try appendMetric(writer, "zombie_redis_pool_idle", S_GAUGE, "Pooled Redis connections sitting idle, ready to lease.", rps.idle);
        try appendMetric(writer, "zombie_redis_pool_dials_total", S_COUNTER, "Total successful TCP dials performed by the Redis pool.", rps.dials_total);
        try appendMetric(writer, "zombie_redis_pool_overflow_dials_total", S_COUNTER, "Dials that occurred while active connections were at or over max_idle (transient burst).", rps.overflow_dials_total);
        try appendMetric(writer, "zombie_redis_pool_poisoned_connections_total", S_COUNTER, "Connections released after entering the .poisoned state (transport error in flight).", rps.poisoned_connections_total);
        try appendMetric(writer, "zombie_redis_pool_reconnects_total", S_COUNTER, "Fresh dials performed by the Client retry layer after a transport-level failure.", rps.reconnects_total);
        try appendMetric(writer, "zombie_redis_pool_forced_closes_total", S_COUNTER, "Connections closed by release because the idle list was already at max_idle (over-cap overflow).", rps.forced_closes_total);
        try appendMetric(writer, "zombie_redis_pool_acquire_timeouts_total", S_COUNTER, "Acquire calls that timed out waiting for a slot (currently always 0 — Pool acquires never block).", rps.acquire_timeouts_total);
    }

    // Per-workspace metrics.
    try mw.renderPrometheus(writer);

    // Per-runner failure metrics (pushed in on each runner report).
    try mr.renderPrometheus(writer);

    try writer.writeAll("\n");

    return aw.toOwnedSlice();
}
