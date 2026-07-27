//! Prometheus text rendering for metrics_counters state.

const std = @import("std");
const mc = @import("metrics_counters.zig");
const mr = @import("metrics_runner.zig");
const mrp = @import("metrics_redis_pool.zig");
const msm = @import("metrics_sensitive_memory.zig");
const mt = @import("metrics_trace.zig");
const mot = @import("metrics_otel.zig");

const S_TYPE_S_S_N = "# TYPE {s} {s}\n";
/// One exposition line carrying a single label: `name{label="value"} 42`.
const S_ONE_LABEL_SAMPLE = "{s}{{{s}=\"{s}\"}} {d}\n";
/// The same, with two labels: `name{a="x",b="y"} 42`.
const S_TWO_LABEL_SAMPLE = "{s}{{{s}=\"{s}\",{s}=\"{s}\"}} {d}\n";
const S_REASON = "reason";
const S_SIGNAL = "signal";
const S_ATTRIBUTE = "attribute";
const S_COUNTER = "counter";
const S_HELP_S_S_N = "# HELP {s} {s}\n";
const S_GAUGE = "gauge";

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
        try writer.print(S_ONE_LABEL_SAMPLE, .{ name, label_name, sample.label_value, sample.value });
    }
}

/// The attribute-omission family. Its label VALUES are the wire attribute keys
/// (`gen_ai.request.model`), so an operator reads the same string the OTLP
/// payload would have carried had the attribute been representable.
fn appendAttributeOmissions(writer: anytype, current: mot.Snapshot) !void {
    try writer.print(S_HELP_S_S_N, .{ mot.ATTRIBUTE_OMITTED_NAME, mot.ATTRIBUTE_OMITTED_HELP });
    try writer.print(S_TYPE_S_S_N, .{ mot.ATTRIBUTE_OMITTED_NAME, S_COUNTER });
    for (mot.OMITTED_ATTRIBUTES, 0..) |attribute, attribute_index| {
        for (mot.OMISSION_REASONS, 0..) |reason, reason_index| {
            try writer.print(
                S_TWO_LABEL_SAMPLE,
                .{
                    mot.ATTRIBUTE_OMITTED_NAME,
                    S_ATTRIBUTE,
                    attribute.label(),
                    S_REASON,
                    reason.label(),
                    current.attribute_omitted[attribute_index][reason_index],
                },
            );
        }
    }
}

fn appendOtlpHealth(writer: anytype) !void {
    const current = mot.snapshot();
    try writer.print(S_HELP_S_S_N, .{ mot.QUEUE_DEPTH_NAME, mot.QUEUE_DEPTH_HELP });
    try writer.print(S_TYPE_S_S_N, .{ mot.QUEUE_DEPTH_NAME, S_GAUGE });
    for (mot.SIGNALS, 0..) |signal, signal_index| {
        try writer.print(
            S_ONE_LABEL_SAMPLE,
            .{ mot.QUEUE_DEPTH_NAME, S_SIGNAL, @tagName(signal), current.queue_depth[signal_index] },
        );
    }

    try writer.print(S_HELP_S_S_N, .{ mot.DISCARDED_NAME, mot.DISCARDED_HELP });
    try writer.print(S_TYPE_S_S_N, .{ mot.DISCARDED_NAME, S_COUNTER });
    for (mot.SIGNALS, 0..) |signal, signal_index| {
        for (mot.DISCARD_REASONS, 0..) |reason, reason_index| {
            try writer.print(
                S_TWO_LABEL_SAMPLE,
                .{
                    mot.DISCARDED_NAME,
                    S_SIGNAL,
                    @tagName(signal),
                    S_REASON,
                    @tagName(reason),
                    current.discarded[signal_index][reason_index],
                },
            );
        }
    }

    try appendAttributeOmissions(writer, current);
}

fn appendGuardrailFamilies(writer: anytype, s: mc.Snapshot, worker_running_gauge: u8) !void {
    try appendMetric(writer, "agentsfleet_api_backpressure_rejections_total", S_COUNTER, "Total API requests rejected by in-flight backpressure guard.", s.api_backpressure_rejections_total);
    try appendMetric(writer, "agentsfleet_api_in_flight_requests", S_GAUGE, "Current in-flight API requests protected by backpressure guard.", s.api_in_flight_requests);
    try appendMetric(writer, "agentsfleet_sse_backpressure_rejections_total", S_COUNTER, "Total SSE stream requests rejected at the stream cap.", s.sse_backpressure_rejections_total);
    try appendMetric(writer, "agentsfleet_sse_in_flight_streams", S_GAUGE, "Current live SSE event streams held below the stream cap.", s.sse_in_flight_streams);
    try appendMetric(writer, "agentsfleet_sse_dropped_frames_total", S_COUNTER, "Total SSE frames dropped against slow consumers (bounded per-stream queues).", s.sse_dropped_frames_total);
    try appendMetric(writer, "agentsfleet_sse_hub_reconnects_total", S_COUNTER, "Total successful redials of the shared SSE pub/sub connection.", s.sse_hub_reconnects_total);
    try appendMetric(writer, "agentsfleet_worker_running", S_GAUGE, "Worker liveness gauge (1 running, 0 stopped).", worker_running_gauge);
}

fn appendTraceSuppression(writer: anytype) !void {
    const trace = mt.snapshot();
    try appendLabeledFamily(writer, mt.SUPPRESSED_NAME, S_COUNTER, mt.SUPPRESSED_HELP, S_REASON, &.{
        .{ .label_value = "noisy_route", .value = trace.noisy_route_total },
        .{ .label_value = "runner_rejection_budget", .value = trace.runner_rejection_budget_total },
        .{ .label_value = "server_error_budget", .value = trace.server_error_budget_total },
        .{ .label_value = "sampled_success_budget", .value = trace.sampled_success_budget_total },
        .{ .label_value = "sample_miss", .value = trace.sample_miss_total },
    });
}

fn appendSignupFamilies(writer: anytype, s: mc.Snapshot) !void {
    try appendMetric(writer, "agentsfleet_signup_bootstrapped_total", S_COUNTER, "Clerk webhooks that provisioned a fresh personal account.", s.signup_bootstrapped_total);
    try appendMetric(writer, "agentsfleet_signup_replayed_total", S_COUNTER, "Clerk webhooks that matched an existing account (idempotent replay).", s.signup_replayed_total);
    try appendLabeledFamily(
        writer,
        "agentsfleet_signup_failed_total",
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
}

/// Lease-poll cost and readiness-index health. Every family here is global and
/// wholly unlabelled, so nothing in this block can create a series per fleet,
/// workspace, tenant, or runner. Every value comes from the in-memory snapshot —
/// the readiness depth is the sweeper's sample, never a scrape-time Redis read.
fn appendLeasePollFamilies(writer: anytype, s: mc.Snapshot) !void {
    try appendMetric(writer, mc.LEASE_POLLS_NAME, S_COUNTER, mc.LEASE_POLLS_HELP, s.lease_polls_total);
    try appendMetric(writer, mc.CANDIDATES_SCANNED_NAME, S_COUNTER, mc.CANDIDATES_SCANNED_HELP, s.lease_poll_candidates_scanned_total);
    try appendMetric(writer, mc.DB_ROUNDTRIPS_NAME, S_COUNTER, mc.DB_ROUNDTRIPS_HELP, s.lease_poll_db_roundtrips_total);
    try appendMetric(writer, mc.READY_DEPTH_NAME, S_GAUGE, mc.READY_DEPTH_HELP, s.fleet_ready_depth);
    try appendMetric(writer, mc.READY_WRITE_FAILURES_NAME, S_COUNTER, mc.READY_WRITE_FAILURES_HELP, s.fleet_ready_write_failures_total);
}

/// Redis request-path pool — emitted only when a Pool has been registered
/// (early-boot scrapes pre-registration emit no lines; downstream scrapers
/// treat absent series as zero, matching the no-pool-yet reality).
fn appendRedisPoolFamilies(writer: anytype) !void {
    const rps = mrp.snapshot() orelse return;
    try appendMetric(writer, "agentsfleet_redis_pool_active", S_GAUGE, "Pooled Redis connections currently leased to a caller.", rps.active);
    try appendMetric(writer, "agentsfleet_redis_pool_idle", S_GAUGE, "Pooled Redis connections sitting idle, ready to lease.", rps.idle);
    try appendMetric(writer, "agentsfleet_redis_pool_dials_total", S_COUNTER, "Total successful TCP dials performed by the Redis pool.", rps.dials_total);
    try appendMetric(writer, "agentsfleet_redis_pool_overflow_dials_total", S_COUNTER, "Dials that occurred while active connections were at or over max_idle (transient burst).", rps.overflow_dials_total);
    try appendMetric(writer, "agentsfleet_redis_pool_poisoned_connections_total", S_COUNTER, "Connections released after entering the .poisoned state (transport error in flight).", rps.poisoned_connections_total);
    try appendMetric(writer, "agentsfleet_redis_pool_reconnects_total", S_COUNTER, "Fresh dials performed by the Client retry layer after a transport-level failure.", rps.reconnects_total);
    try appendMetric(writer, "agentsfleet_redis_pool_forced_closes_total", S_COUNTER, "Connections closed by release because the idle list was already at max_idle (over-cap overflow).", rps.forced_closes_total);
    try appendMetric(writer, "agentsfleet_redis_pool_acquire_timeouts_total", S_COUNTER, "Acquire calls that timed out waiting for a slot (currently always 0 — Pool acquires never block).", rps.acquire_timeouts_total);
}

/// Render the whole exposition. Family order is the scrape's stable shape, so
/// the calls below stay in emission order; every family carries the one
/// `agentsfleet_` namespace (`semantic_schema_test.zig` proves it).
pub fn renderPrometheus(
    alloc: std.mem.Allocator,
    worker_running: bool,
) ![]u8 {
    const s = mc.snapshot();

    var aw: std.Io.Writer.Allocating = .init(alloc);
    errdefer aw.deinit();
    const writer = &aw.writer;

    try appendGuardrailFamilies(writer, s, if (worker_running) 1 else 0);
    try appendTraceSuppression(writer);
    try appendOtlpHealth(writer);
    try appendSignupFamilies(writer, s);

    // Per-execution series were emitted here until execution moved to the
    // runner. The runner exposes its own engine metrics; agentsfleetd no longer
    // renders an execution block.
    try appendMetric(writer, "agentsfleet_fleet_triggered_total", S_COUNTER, "Total fleet webhook triggers accepted.", s.fleet_triggered_total);

    try appendLeasePollFamilies(writer, s);
    try appendRedisPoolFamilies(writer);
    try msm.renderPrometheus(writer);
    // Per-runner failure metrics (pushed in on each runner report).
    try mr.renderPrometheus(writer);

    try writer.writeAll("\n");

    return aw.toOwnedSlice();
}
