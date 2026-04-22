//! Tests for otel_histogram.zig — moved out of the main file to stay under
//! the 350-line gate (RULE FLL).

const std = @import("std");
const otel_histogram = @import("otel_histogram.zig");
const Accumulator = otel_histogram.Accumulator;

test "Accumulator ingests bucket / sum / count lines" {
    const alloc = std.testing.allocator;
    var acc = Accumulator.init(alloc);
    defer acc.deinit();

    try std.testing.expect(try acc.tryIngest("zombie_execution_seconds_bucket{le=\"0.01\"} 0"));
    try std.testing.expect(try acc.tryIngest("zombie_execution_seconds_bucket{le=\"0.1\"} 2"));
    try std.testing.expect(try acc.tryIngest("zombie_execution_seconds_bucket{le=\"+Inf\"} 3"));
    try std.testing.expect(try acc.tryIngest("zombie_execution_seconds_sum 0.150"));
    try std.testing.expect(try acc.tryIngest("zombie_execution_seconds_count 3"));

    const h = acc.histograms.get("zombie_execution_seconds").?;
    try std.testing.expectEqual(@as(usize, 3), h.buckets.items.len);
    try std.testing.expectEqual(@as(u64, 3), h.count);
    try std.testing.expectApproxEqAbs(@as(f64, 0.150), h.sum, 1e-9);
}

test "Accumulator matches trailing suffix even when base name contains _sum/_count" {
    const alloc = std.testing.allocator;
    var acc = Accumulator.init(alloc);
    defer acc.deinit();

    // Base name itself contains `_count` — a naive indexOf would chop the base
    // at the first occurrence and associate the count with a phantom histogram.
    _ = try acc.tryIngest("foo_count_seconds_bucket{le=\"1\"} 2");
    _ = try acc.tryIngest("foo_count_seconds_bucket{le=\"+Inf\"} 2");
    _ = try acc.tryIngest("foo_count_seconds_sum 0.5");
    _ = try acc.tryIngest("foo_count_seconds_count 2");

    try std.testing.expect(acc.histograms.contains("foo_count_seconds"));
    const h = acc.histograms.get("foo_count_seconds").?;
    try std.testing.expectEqual(@as(u64, 2), h.count);
    try std.testing.expectEqual(@as(usize, 2), h.buckets.items.len);
    // And no phantom truncated-name histogram got created.
    try std.testing.expect(!acc.histograms.contains("foo"));
    try std.testing.expect(!acc.histograms.contains("foo_count"));
}

test "Accumulator rejects labeled histogram lines (would conflate series)" {
    const alloc = std.testing.allocator;
    var acc = Accumulator.init(alloc);
    defer acc.deinit();

    // Labeled bucket / sum / count lines must be rejected and NOT create an
    // accumulator entry — otherwise two workspaces' histograms would stack
    // under one OTLP data point.
    try std.testing.expect(!try acc.tryIngest("http_latency_bucket{workspace=\"w1\",le=\"0.1\"} 5"));
    try std.testing.expect(!try acc.tryIngest("http_latency_sum{workspace=\"w1\"} 0.42"));
    try std.testing.expect(!try acc.tryIngest("http_latency_count{workspace=\"w1\"} 5"));
    try std.testing.expect(!acc.histograms.contains("http_latency"));
}

test "Accumulator ignores non-histogram lines" {
    const alloc = std.testing.allocator;
    var acc = Accumulator.init(alloc);
    defer acc.deinit();

    try std.testing.expect(!try acc.tryIngest("zombie_tokens_total 500"));
    try std.testing.expect(!try acc.tryIngest("zombie_worker_running 1"));
}

test "writeOtlp skips non-finite bucket bounds and clamps non-finite sum" {
    const alloc = std.testing.allocator;
    var acc = Accumulator.init(alloc);
    defer acc.deinit();

    // Build a histogram with pathological f64 values that would otherwise
    // serialize as unquoted `nan` / `inf` and break OTLP JSON parsing.
    const h = try acc.getOrCreate("poison");
    try h.buckets.append(alloc, .{ .upper_bound = 0.1, .cumulative_count = 1 });
    try h.buckets.append(alloc, .{ .upper_bound = std.math.nan(f64), .cumulative_count = 1 });
    try h.buckets.append(alloc, .{ .upper_bound = std.math.inf(f64), .cumulative_count = 2 });
    h.count = 2;
    h.sum = std.math.nan(f64);

    var buf: [4096]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    var first = true;
    try acc.writeOtlp(fbs.writer(), 1, 2, &first);
    const out = fbs.getWritten();

    // No literal `nan` or `inf` — both would make the JSON invalid.
    try std.testing.expect(std.mem.indexOf(u8, out, "nan") == null);
    try std.testing.expect(std.mem.indexOf(u8, out, "inf") == null);
    // Non-finite bound was skipped: only the 0.1 bound appears.
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"explicitBounds\":[0.1]"));
    // Non-finite sum was clamped to 0.
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"sum\":0"));
}

test "writeOtlp emits cumulative→delta bucketCounts and explicitBounds" {
    const alloc = std.testing.allocator;
    var acc = Accumulator.init(alloc);
    defer acc.deinit();

    _ = try acc.tryIngest("h_bucket{le=\"0.1\"} 1");
    _ = try acc.tryIngest("h_bucket{le=\"1\"} 3");
    _ = try acc.tryIngest("h_bucket{le=\"+Inf\"} 4");
    _ = try acc.tryIngest("h_sum 0.8");
    _ = try acc.tryIngest("h_count 4");

    var buf: [4096]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    var first = true;
    try acc.writeOtlp(fbs.writer(), 1_600_000_000_000_000_000, 1_700_000_000_000_000_000, &first);
    const out = fbs.getWritten();

    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"name\":\"h\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"aggregationTemporality\":2"));
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"explicitBounds\":[0.1,1]"));
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"bucketCounts\":[\"1\",\"2\",\"1\"]"));
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"count\":\"4\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"startTimeUnixNano\":\"1600000000000000000\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"timeUnixNano\":\"1700000000000000000\""));
}
