const std = @import("std");
const metrics = @import("metrics_sensitive_memory.zig");

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

test "sensitive memory metrics render current RSS and unlabeled aggregate counters" {
    var buffer: [2048]u8 = undefined;
    var writer = std.Io.Writer.fixed(&buffer);
    try metrics.renderPrometheus(&writer);
    const output = writer.buffered();

    try expectMetric(output, metrics.METRIC_PROCESS_RESIDENT_MEMORY);
    try expectMetric(output, metrics.METRIC_REQUEST_ERASED_BYTES);
    try expectMetric(output, metrics.METRIC_RESPONSE_ERASED_BYTES);
    try expectMetric(output, metrics.METRIC_RESPONSE_WRITE_FAILURES);
    try std.testing.expect(std.mem.indexOfScalar(u8, output, '{') == null);
}

fn expectMetric(output: []const u8, name: []const u8) !void {
    try std.testing.expect(std.mem.indexOf(u8, output, name) != null);
}

fn recordConcurrent() void {
    for (0..INCREMENTS_PER_WRITER) |_| {
        metrics.recordRequestErased(1);
        metrics.recordResponseErased(1);
        metrics.incResponseWriteFailure();
    }
}
