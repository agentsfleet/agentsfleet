//! Prometheus text rendering for metrics_counters state.

const std = @import("std");
const mc = @import("metrics_counters.zig");

fn appendDurationHistogram(
    writer: anytype,
    name: []const u8,
    help: []const u8,
    hist: mc.HistogramSnapshot,
) !void {
    try writer.print("# HELP {s} {s}\n", .{ name, help });
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
    const s = mc.snapshot();
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

    try appendMetric(writer, "zombie_agent_score_computed_total", "counter", "Total scored runs across all tiers.", s.agent_score_computed_total);
    try appendMetric(writer, "zombie_agent_score_computed_unranked_total", "counter", "Scored runs with UNRANKED tier.", s.agent_score_computed_unranked);
    try appendMetric(writer, "zombie_agent_score_computed_bronze_total", "counter", "Scored runs with BRONZE tier.", s.agent_score_computed_bronze);
    try appendMetric(writer, "zombie_agent_score_computed_silver_total", "counter", "Scored runs with SILVER tier.", s.agent_score_computed_silver);
    try appendMetric(writer, "zombie_agent_score_computed_gold_total", "counter", "Scored runs with GOLD tier.", s.agent_score_computed_gold);
    try appendMetric(writer, "zombie_agent_score_computed_elite_total", "counter", "Scored runs with ELITE tier.", s.agent_score_computed_elite);
    try appendMetric(writer, "zombie_agent_scoring_failed_total", "counter", "Total scoring failures caught by fail-safe.", s.agent_scoring_failed_total);
    try appendMetric(writer, "zombie_agent_score_latest", "gauge", "Most recently computed agent score.", s.agent_score_latest);
    try appendMetric(writer, "zombie_otel_export_total", "counter", "Total OTEL metric export attempts.", s.otel_export_total);
    try appendMetric(writer, "zombie_otel_export_failed_total", "counter", "Total OTEL metric export failures.", s.otel_export_failed_total);
    try appendMetric(writer, "zombie_otel_last_success_at_ms", "gauge", "Timestamp of last successful OTEL export in epoch ms.", s.otel_last_success_at_ms);

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
    try appendDurationHistogram(
        writer,
        "zombie_agent_scoring_duration_ms",
        "Time spent in scoreRun in milliseconds.",
        s.agent_scoring_duration_ms,
    );
    try writer.writeAll("\n");

    return out.toOwnedSlice(alloc);
}
