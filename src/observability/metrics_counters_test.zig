const std = @import("std");
const mc = @import("metrics_counters.zig");
const error_classify = @import("../reliability/error_classify.zig");

// T9 — inline-for over all ErrorClass variants; catches any new class added without a counter
test "incExternalRetry routes each ErrorClass to its sub-counter" {
    inline for (.{
        .{ error_classify.ErrorClass.rate_limited, "rate_limited" },
        .{ error_classify.ErrorClass.timeout, "timeout" },
        .{ error_classify.ErrorClass.context_exhausted, "context_exhausted" },
        .{ error_classify.ErrorClass.auth, "auth" },
        .{ error_classify.ErrorClass.invalid_request, "invalid_request" },
        .{ error_classify.ErrorClass.server_error, "server_error" },
        .{ error_classify.ErrorClass.unknown, "unknown" },
    }) |pair| {
        const class = pair[0];
        const before = mc.snapshot();
        mc.incExternalRetry(class);
        const after = mc.snapshot();
        try std.testing.expectEqual(before.external_retries_total + 1, after.external_retries_total);
        const delta_sub: u64 = switch (class) {
            .rate_limited => after.external_retries_rate_limited_total - before.external_retries_rate_limited_total,
            .timeout => after.external_retries_timeout_total - before.external_retries_timeout_total,
            .context_exhausted => after.external_retries_context_exhausted_total - before.external_retries_context_exhausted_total,
            .auth => after.external_retries_auth_total - before.external_retries_auth_total,
            .invalid_request => after.external_retries_invalid_request_total - before.external_retries_invalid_request_total,
            .server_error => after.external_retries_server_error_total - before.external_retries_server_error_total,
            .unknown => after.external_retries_unknown_total - before.external_retries_unknown_total,
        };
        try std.testing.expectEqual(@as(u64, 1), delta_sub);
    }
}

// T9 — mirrors above for the failure path; guards against divergence between retry and failure routing
test "incExternalFailure routes each ErrorClass to its sub-counter" {
    inline for (.{
        .{ error_classify.ErrorClass.rate_limited, "rate_limited" },
        .{ error_classify.ErrorClass.timeout, "timeout" },
        .{ error_classify.ErrorClass.context_exhausted, "context_exhausted" },
        .{ error_classify.ErrorClass.auth, "auth" },
        .{ error_classify.ErrorClass.invalid_request, "invalid_request" },
        .{ error_classify.ErrorClass.server_error, "server_error" },
        .{ error_classify.ErrorClass.unknown, "unknown" },
    }) |pair| {
        const class = pair[0];
        const before = mc.snapshot();
        mc.incExternalFailure(class);
        const after = mc.snapshot();
        try std.testing.expectEqual(before.external_failures_total + 1, after.external_failures_total);
        const delta_sub: u64 = switch (class) {
            .rate_limited => after.external_failures_rate_limited_total - before.external_failures_rate_limited_total,
            .timeout => after.external_failures_timeout_total - before.external_failures_timeout_total,
            .context_exhausted => after.external_failures_context_exhausted_total - before.external_failures_context_exhausted_total,
            .auth => after.external_failures_auth_total - before.external_failures_auth_total,
            .invalid_request => after.external_failures_invalid_request_total - before.external_failures_invalid_request_total,
            .server_error => after.external_failures_server_error_total - before.external_failures_server_error_total,
            .unknown => after.external_failures_unknown_total - before.external_failures_unknown_total,
        };
        try std.testing.expectEqual(@as(u64, 1), delta_sub);
    }
}

// T2 — observeHistogram: value exactly at bucket boundary falls in that bucket
test "observeHistogram records value at exact le boundary in the correct bucket" {
    const before = mc.snapshot();
    mc.observeAgentDurationSeconds(1); // DurationBuckets[0] = 1
    const after = mc.snapshot();
    try std.testing.expectEqual(before.agent_duration_seconds.buckets[0] + 1, after.agent_duration_seconds.buckets[0]);
    try std.testing.expectEqual(before.agent_duration_seconds.count + 1, after.agent_duration_seconds.count);
    try std.testing.expectEqual(before.agent_duration_seconds.sum + 1, after.agent_duration_seconds.sum);
}

// T2 — observeHistogram: value above all buckets increments only count/sum, no bucket slot
test "observeHistogram value above all buckets increments count and sum only" {
    const before = mc.snapshot();
    mc.observeAgentDurationSeconds(999); // > 300, the largest bucket le
    const after = mc.snapshot();
    for (before.agent_duration_seconds.buckets, 0..) |bval, i| {
        try std.testing.expectEqual(bval, after.agent_duration_seconds.buckets[i]);
    }
    try std.testing.expectEqual(before.agent_duration_seconds.count + 1, after.agent_duration_seconds.count);
    try std.testing.expectEqual(before.agent_duration_seconds.sum + 999, after.agent_duration_seconds.sum);
}

// T5 — atomic counters must not lose increments under concurrent write pressure
test "atomic counters tolerate concurrent increments without loss" {
    const N = 100;
    const before = mc.snapshot();
    var threads: [N]std.Thread = undefined;
    for (&threads) |*t| {
        t.* = try std.Thread.spawn(.{}, struct {
            fn run() void {
                mc.incRunsCreated();
            }
        }.run, .{});
    }
    for (&threads) |t| t.join();
    const after = mc.snapshot();
    try std.testing.expectEqual(before.runs_created_total + N, after.runs_created_total);
}

// ── M17_001 — run limit counter tests ────────────────────────────────────

// T1: incRunLimitTokenBudgetExceeded increments the dedicated counter.
test "M17: incRunLimitTokenBudgetExceeded increments by 1" {
    const before = mc.snapshot();
    mc.incRunLimitTokenBudgetExceeded();
    const after = mc.snapshot();
    try std.testing.expectEqual(before.run_limit_token_budget_exceeded_total + 1, after.run_limit_token_budget_exceeded_total);
}

// T1: incRunLimitWallTimeExceeded increments the dedicated counter.
test "M17: incRunLimitWallTimeExceeded increments by 1" {
    const before = mc.snapshot();
    mc.incRunLimitWallTimeExceeded();
    const after = mc.snapshot();
    try std.testing.expectEqual(before.run_limit_wall_time_exceeded_total + 1, after.run_limit_wall_time_exceeded_total);
}

// T1: incRunLimitRepairLoopsExhausted increments the dedicated counter.
test "M17: incRunLimitRepairLoopsExhausted increments by 1" {
    const before = mc.snapshot();
    mc.incRunLimitRepairLoopsExhausted();
    const after = mc.snapshot();
    try std.testing.expectEqual(before.run_limit_repair_loops_exhausted_total + 1, after.run_limit_repair_loops_exhausted_total);
}

// T2: the three M17 counters are independent — incrementing one does not
//     change the other two.
test "M17: run limit counters are independent" {
    const before = mc.snapshot();
    mc.incRunLimitTokenBudgetExceeded();
    const after = mc.snapshot();
    try std.testing.expectEqual(before.run_limit_wall_time_exceeded_total, after.run_limit_wall_time_exceeded_total);
    try std.testing.expectEqual(before.run_limit_repair_loops_exhausted_total, after.run_limit_repair_loops_exhausted_total);
}

// T5: all three M17 counters are race-safe under concurrent writes.
test "M17: run limit counters tolerate concurrent increments without loss" {
    const N = 50;
    const before = mc.snapshot();
    var threads: [N]std.Thread = undefined;
    for (&threads) |*t| {
        t.* = try std.Thread.spawn(.{}, struct {
            fn run() void {
                mc.incRunLimitTokenBudgetExceeded();
                mc.incRunLimitWallTimeExceeded();
                mc.incRunLimitRepairLoopsExhausted();
            }
        }.run, .{});
    }
    for (&threads) |t| t.join();
    const after = mc.snapshot();
    try std.testing.expectEqual(before.run_limit_token_budget_exceeded_total + N, after.run_limit_token_budget_exceeded_total);
    try std.testing.expectEqual(before.run_limit_wall_time_exceeded_total + N, after.run_limit_wall_time_exceeded_total);
    try std.testing.expectEqual(before.run_limit_repair_loops_exhausted_total + N, after.run_limit_repair_loops_exhausted_total);
}

// T4: Prometheus output contains all three M17 run_limit reason labels.
test "M17: renderPrometheus includes all three run_limit reason labels" {
    const alloc = std.testing.allocator;
    const render = @import("metrics_render.zig");
    const output = try render.renderPrometheus(alloc, false, null, null);
    defer alloc.free(output);
    try std.testing.expect(std.mem.indexOf(u8, output, "zombied_run_limit_exceeded_total{reason=\"token_budget\"}") != null);
    try std.testing.expect(std.mem.indexOf(u8, output, "zombied_run_limit_exceeded_total{reason=\"wall_time\"}") != null);
    try std.testing.expect(std.mem.indexOf(u8, output, "zombied_run_limit_exceeded_total{reason=\"repair_loops\"}") != null);
}

// T4: Prometheus output for run_limit counters reflects the current snapshot value.
test "M17: renderPrometheus run_limit values match snapshot" {
    const alloc = std.testing.allocator;
    const render = @import("metrics_render.zig");
    mc.incRunLimitTokenBudgetExceeded();
    const snap = mc.snapshot();
    const output = try render.renderPrometheus(alloc, false, null, null);
    defer alloc.free(output);
    // Build the expected line and search for it.
    var buf: [256]u8 = undefined;
    const expected = try std.fmt.bufPrint(&buf, "zombied_run_limit_exceeded_total{{reason=\"token_budget\"}} {d}", .{snap.run_limit_token_budget_exceeded_total});
    try std.testing.expect(std.mem.indexOf(u8, output, expected) != null);
}
