const std = @import("std");
const common = @import("common");
const metrics = @import("metrics_sensitive_memory.zig");
const window = @import("otel_metrics_window_test.zig");

const CONCURRENT_WRITERS: usize = 100;
const INCREMENTS_PER_WRITER: usize = 100;

test "sensitive memory counters record aggregate bytes and write failures" {
    const before = metrics.snapshot();
    metrics.recordRequestErased(31);
    metrics.recordResponseErased(47);
    metrics.incResponseWriteFailure();
    const after = metrics.snapshot();

    try std.testing.expectEqual(before.request_erased_bytes_total + 31, after.request_erased_bytes_total);
    try std.testing.expectEqual(before.response_erased_bytes_total + 47, after.response_erased_bytes_total);
    try std.testing.expectEqual(before.response_write_failures_total + 1, after.response_write_failures_total);
}

test "sensitive memory counters preserve increments from 100 concurrent writers" {
    const before = metrics.snapshot();
    var threads: [CONCURRENT_WRITERS]std.Thread = undefined;
    for (&threads) |*thread| thread.* = try std.Thread.spawn(.{}, recordConcurrent, .{});
    for (threads) |thread| thread.join();
    const after = metrics.snapshot();
    const expected: u64 = @intCast(CONCURRENT_WRITERS * INCREMENTS_PER_WRITER);

    try std.testing.expectEqual(before.request_erased_bytes_total + expected, after.request_erased_bytes_total);
    try std.testing.expectEqual(before.response_erased_bytes_total + expected, after.response_erased_bytes_total);
    try std.testing.expectEqual(before.response_write_failures_total + expected, after.response_write_failures_total);
}

test "sensitive memory metrics export current RSS and unlabeled aggregate counters" {
    const alloc = std.testing.allocator;
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);

    // Each aggregate counter exports wholly unlabelled — no tenant, workspace,
    // fleet, route, or secret identity ever rides a sensitive-memory series.
    try window.expectFamilyWith(body, metrics.METRIC_REQUEST_ERASED_BYTES, &.{window.NO_ATTRIBUTES});
    try window.expectFamilyWith(body, metrics.METRIC_RESPONSE_ERASED_BYTES, &.{window.NO_ATTRIBUTES});
    try window.expectFamilyWith(body, metrics.METRIC_RESPONSE_WRITE_FAILURES, &.{window.NO_ATTRIBUTES});

    // The resident-set probe is platform-dependent; when the platform reports
    // it, the gauge is present (unlabelled) — when it cannot, the family is
    // absent rather than a fake zero.
    if (common.rss.currentBytes() != null) {
        try window.expectFamilyWith(body, metrics.METRIC_PROCESS_RESIDENT_MEMORY, &.{window.NO_ATTRIBUTES});
    } else {
        try window.expectNoFamilyWith(body, metrics.METRIC_PROCESS_RESIDENT_MEMORY, &.{});
    }
}

fn recordConcurrent() void {
    for (0..INCREMENTS_PER_WRITER) |_| {
        metrics.recordRequestErased(1);
        metrics.recordResponseErased(1);
        metrics.incResponseWriteFailure();
    }
}
