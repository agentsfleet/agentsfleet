//! Prometheus text rendering for metrics_counters state.

const std = @import("std");
const mc = @import("metrics_counters.zig");
const mw = @import("metrics_workspace.zig");
const em = @import("../executor/executor_metrics.zig");

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
) ![]u8 {
    const s = mc.snapshot();
    const worker_running_gauge: u8 = if (worker_running) 1 else 0;

    var out: std.ArrayList(u8) = .{};
    errdefer out.deinit(alloc);
    const writer = out.writer(alloc);

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
    try appendMetric(writer, "zombie_sandbox_shell_runs_total", "counter", "Total shell tool executions routed through sandbox wrapper.", s.sandbox_shell_runs_total);
    try appendMetric(writer, "zombie_sandbox_host_runs_total", "counter", "Total sandbox shell executions using host backend.", s.sandbox_host_runs_total);
    try appendMetric(writer, "zombie_sandbox_bubblewrap_runs_total", "counter", "Total sandbox shell executions using bubblewrap backend.", s.sandbox_bubblewrap_runs_total);
    try appendMetric(writer, "zombie_sandbox_kill_switch_total", "counter", "Total shell executions interrupted by sandbox kill switch.", s.sandbox_kill_switch_total);
    try appendMetric(writer, "zombie_sandbox_preflight_failures_total", "counter", "Total sandbox backend preflight failures.", s.sandbox_preflight_failures_total);
    try appendMetric(writer, "zombie_agent_echo_calls_total", "counter", "Total Echo agent invocations.", s.agent_echo_calls_total);
    try appendMetric(writer, "zombie_agent_scout_calls_total", "counter", "Total Scout agent invocations.", s.agent_scout_calls_total);
    try appendMetric(writer, "zombie_agent_warden_calls_total", "counter", "Total Warden agent invocations.", s.agent_warden_calls_total);
    try appendMetric(writer, "zombie_agent_tokens_total", "counter", "Total tokens consumed by agent calls.", s.agent_tokens_total);
    try appendMetric(writer, "zombie_agent_echo_tokens_total", "counter", "Tokens consumed by Echo (plan) agent calls.", s.agent_echo_tokens_total);
    try appendMetric(writer, "zombie_agent_scout_tokens_total", "counter", "Tokens consumed by Scout (implement) agent calls.", s.agent_scout_tokens_total);
    try appendMetric(writer, "zombie_agent_warden_tokens_total", "counter", "Tokens consumed by Warden (validate) agent calls.", s.agent_warden_tokens_total);
    try appendMetric(writer, "zombie_agent_orchestrator_tokens_total", "counter", "Tokens consumed by Orchestrator agent calls.", s.agent_orchestrator_tokens_total);
    try appendMetric(writer, "zombie_backoff_wait_ms_total", "counter", "Total backoff wait time in milliseconds.", s.backoff_wait_ms_total);
    try appendMetric(writer, "zombie_rate_limit_wait_ms_total", "counter", "Total wait time due to rate limiting in milliseconds.", s.rate_limit_wait_ms_total);
    try appendMetric(writer, "zombie_side_effect_outbox_enqueued_total", "counter", "Total side-effect outbox entries enqueued.", s.side_effect_outbox_enqueued_total);
    try appendMetric(writer, "zombie_side_effect_outbox_delivered_total", "counter", "Total side-effect outbox entries marked delivered.", s.side_effect_outbox_delivered_total);
    try appendMetric(writer, "zombie_side_effect_outbox_dead_letter_total", "counter", "Total side-effect outbox entries dead-lettered by reconciliation.", s.side_effect_outbox_dead_letter_total);
    try appendMetric(writer, "zombie_api_backpressure_rejections_total", "counter", "Total API requests rejected by in-flight backpressure guard.", s.api_backpressure_rejections_total);
    try appendMetric(writer, "zombie_api_in_flight_requests", "gauge", "Current in-flight API requests protected by backpressure guard.", s.api_in_flight_requests);
    try appendMetric(writer, "zombie_worker_allocator_leaks_total", "counter", "Total worker allocator leak detections on teardown.", s.worker_allocator_leaks_total);
    try appendMetric(writer, "zombie_worker_running", "gauge", "Worker liveness gauge (1 running, 0 stopped).", worker_running_gauge);

    try appendMetric(writer, "zombie_gate_repair_loops_total", "counter", "Total gate repair loop iterations.", s.gate_repair_loops_total);
    try appendMetric(writer, "zombie_gate_repair_exhausted_total", "counter", "Total gate repair exhaustions (max loops reached).", s.gate_repair_exhausted_total);
    // M17_001 §1.3: per-limit-type counters
    try appendMetric(writer, "zombied_run_limit_exceeded_total{reason=\"token_budget\"}", "counter", "Runs terminated by token budget limit.", s.run_limit_token_budget_exceeded_total);
    try appendMetric(writer, "zombied_run_limit_exceeded_total{reason=\"wall_time\"}", "counter", "Runs terminated by wall-time limit.", s.run_limit_wall_time_exceeded_total);
    try appendMetric(writer, "zombied_run_limit_exceeded_total{reason=\"repair_loops\"}", "counter", "Runs terminated by repair-loop exhaustion.", s.run_limit_repair_loops_exhausted_total);
    try appendMetric(writer, "zombie_otel_export_total", "counter", "Total OTEL metric export attempts.", s.otel_export_total);
    try appendMetric(writer, "zombie_otel_export_failed_total", "counter", "Total OTEL metric export failures.", s.otel_export_failed_total);
    try appendMetric(writer, "zombie_otel_last_success_at_ms", "gauge", "Timestamp of last successful OTEL export in epoch ms.", s.otel_last_success_at_ms);

    // Orphan recovery metrics (M14_001).
    try appendMetric(writer, "zombie_reconcile_orphan_runs_recovered_total", "counter", "Total orphaned runs recovered by reconciler.", s.orphan_runs_recovered_total);
    try appendMetric(writer, "zombie_reconcile_orphan_no_agent_profile_total", "counter", "Orphan recoveries where workspace had no active agent profile.", s.orphan_no_agent_profile_total);
    try appendMetric(writer, "zombie_reconcile_running", "gauge", "Reconcile daemon liveness gauge (1 running, 0 stopped).", s.reconcile_running);

    try appendDurationHistogram(
        writer,
        "zombie_agent_duration_seconds",
        "Duration of individual agent calls in seconds.",
        s.agent_duration_seconds,
    );
    // M28_001 §4.1: gate repair loops per run histogram (custom buckets).
    {
        const glh = s.gate_repair_loops_per_run;
        try writer.print("# HELP zombie_gate_repair_loops_per_run Distribution of gate repair loops per run.\n", .{});
        try writer.print("# TYPE zombie_gate_repair_loops_per_run histogram\n", .{});
        for (mc.GateLoopBuckets, 0..) |le, i| {
            try writer.print("zombie_gate_repair_loops_per_run_bucket{{le=\"{d}\"}} {d}\n", .{ le, glh.buckets[i] });
        }
        try writer.print("zombie_gate_repair_loops_per_run_bucket{{le=\"+Inf\"}} {d}\n", .{glh.count});
        try writer.print("zombie_gate_repair_loops_per_run_sum {d}\n", .{glh.sum});
        try writer.print("zombie_gate_repair_loops_per_run_count {d}\n", .{glh.count});
    }

    // M21_002 §4.3: interrupt delivery latency histogram (custom buckets).
    {
        const ilh = s.interrupt_delivery_latency_ms;
        try writer.print("# HELP zombie_interrupt_delivery_latency_ms Time from gate start to interrupt detection in milliseconds.\n", .{});
        try writer.print("# TYPE zombie_interrupt_delivery_latency_ms histogram\n", .{});
        for (mc.InterruptLatencyBuckets, 0..) |le, i| {
            try writer.print("zombie_interrupt_delivery_latency_ms_bucket{{le=\"{d}\"}} {d}\n", .{ le, ilh.buckets[i] });
        }
        try writer.print("zombie_interrupt_delivery_latency_ms_bucket{{le=\"+Inf\"}} {d}\n", .{ilh.count});
        try writer.print("zombie_interrupt_delivery_latency_ms_sum {d}\n", .{ilh.sum});
        try writer.print("zombie_interrupt_delivery_latency_ms_count {d}\n", .{ilh.count});
    }

    // Executor metrics (§5.2).
    const es = em.executorSnapshot();
    try appendMetric(writer, "zombie_executor_sessions_created_total", "counter", "Total executor sessions created.", es.sessions_created_total);
    try appendMetric(writer, "zombie_executor_sessions_active", "gauge", "Current active executor sessions.", es.sessions_active);
    try appendMetric(writer, "zombie_executor_failures_total", "counter", "Total executor RPC failures.", es.failures_total);
    try appendMetric(writer, "zombie_executor_oom_kills_total", "counter", "Total OOM kills by executor cgroup.", es.oom_kills_total);
    try appendMetric(writer, "zombie_executor_timeout_kills_total", "counter", "Total timeout kills through executor boundary.", es.timeout_kills_total);
    try appendMetric(writer, "zombie_executor_landlock_denials_total", "counter", "Total Landlock filesystem denials.", es.landlock_denials_total);
    try appendMetric(writer, "zombie_executor_resource_kills_total", "counter", "Total resource limit kills (CPU/disk).", es.resource_kills_total);
    try appendMetric(writer, "zombie_executor_lease_expired_total", "counter", "Total executor leases expired (orphan cleanup).", es.lease_expired_total);
    try appendMetric(writer, "zombie_executor_cancellations_total", "counter", "Total executor session cancellations.", es.cancellations_total);
    try appendMetric(writer, "zombie_executor_cpu_throttled_ms_total", "counter", "Total CPU throttling time in milliseconds.", es.cpu_throttled_ms_total);
    try appendMetric(writer, "zombie_executor_memory_peak_bytes", "gauge", "Peak memory usage across executor sessions.", es.memory_peak_bytes);

    // Executor stage metrics (M12_003 §5.3).
    try appendMetric(writer, "zombie_executor_stages_started_total", "counter", "Total NullClaw stage invocations started.", es.stages_started_total);
    try appendMetric(writer, "zombie_executor_stages_completed_total", "counter", "Total NullClaw stage invocations completed successfully.", es.stages_completed_total);
    try appendMetric(writer, "zombie_executor_stages_failed_total", "counter", "Total NullClaw stage invocations failed.", es.stages_failed_total);
    try appendMetric(writer, "zombie_executor_agent_tokens_total", "counter", "Total tokens consumed by executor agent calls.", es.agent_tokens_total);

    // Executor agent duration histogram (M12_003 §5.3).
    try writer.print("# HELP zombie_executor_agent_duration_seconds Duration of executor agent calls in seconds.\n", .{});
    try writer.print("# TYPE zombie_executor_agent_duration_seconds histogram\n", .{});
    for (em.DURATION_BUCKETS, 0..) |le, i| {
        try writer.print("zombie_executor_agent_duration_seconds_bucket{{le=\"{d}\"}} {d}\n", .{ le, es.duration_buckets[i] });
    }
    try writer.print("zombie_executor_agent_duration_seconds_bucket{{le=\"+Inf\"}} {d}\n", .{es.duration_count});
    try writer.print("zombie_executor_agent_duration_seconds_sum {d}\n", .{es.duration_sum});
    try writer.print("zombie_executor_agent_duration_seconds_count {d}\n", .{es.duration_count});

    // M28_001 §2.5: per-workspace metrics.
    try mw.renderPrometheus(writer);

    try writer.writeAll("\n");

    return out.toOwnedSlice(alloc);
}
