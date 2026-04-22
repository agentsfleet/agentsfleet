//! Tests for otel_export.zig public surface — moved out of the main file to
//! stay under the 350-line gate (RULE FLL).

const std = @import("std");
const otel_export = @import("otel_export.zig");

test "renderOtlpJson splits Prometheus labels into OTLP attributes (name field is bare)" {
    const alloc = std.testing.allocator;
    const mc = @import("metrics_counters.zig");
    mc.incSignupFailed(.bad_sig);

    const body = try otel_export.renderOtlpJson(alloc, "zombied", true);
    defer alloc.free(body);

    try std.testing.expect(std.mem.indexOf(u8, body, "zombie_signup_failed_total{") == null);
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "\"name\":\"zombie_signup_failed_total\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "\"attributes\":["));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "\"key\":\"reason\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "\"stringValue\":\"bad_sig\""));
}

test "writeAttributes emits multiple label pairs in order" {
    var buf: [512]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try otel_export.writeAttributes(fbs.writer(), "workspace_id=\"ws-1\",zombie_id=\"z-2\"");
    const out = fbs.getWritten();
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"key\":\"workspace_id\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"stringValue\":\"ws-1\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"key\":\"zombie_id\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"stringValue\":\"z-2\""));
}

test "renderOtlpJson stamps startTimeUnixNano on counter and gauge data points" {
    const alloc = std.testing.allocator;
    const body = try otel_export.renderOtlpJson(alloc, "zombied", true);
    defer alloc.free(body);

    // Every data point must carry startTimeUnixNano so CUMULATIVE backends can
    // anchor the aggregation window. Without it, the first scrape looks like a
    // zero-duration interval and rates compute to zero.
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "\"startTimeUnixNano\":\""));
    // The counter emitter puts aggregationTemporality + isMonotonic at the sum
    // level (per OTLP proto), not inside the data point.
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "\"sum\":{\"aggregationTemporality\":2,\"isMonotonic\":true"));
}

test "writeAttributes JSON-escapes plain special characters (no Prom escapes)" {
    var buf: [512]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    // Label value literally contains a newline (no Prometheus escape). That
    // must be JSON-encoded as `\n`, and the pipe character passes through.
    try otel_export.writeAttributes(fbs.writer(), "raw=\"a\nb|c\"");
    const out = fbs.getWritten();
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"stringValue\":\"a\\nb|c\""));
}

test "renderOtlpJson escapes service name with special characters" {
    const alloc = std.testing.allocator;
    const body = try otel_export.renderOtlpJson(alloc, "svc\"with\\special", true);
    defer alloc.free(body);

    // Raw `"` or `\` characters in the name would have broken JSON. After
    // escaping, the sequence `\"` and `\\` must appear in place.
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "svc\\\"with\\\\special"));
    try std.testing.expect(std.mem.indexOf(u8, body, "\"svc\"with") == null);
}

test "writeAttributes preserves Prometheus-escaped quote inside a label value" {
    var buf: [512]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    // Source label value in Prometheus exposition: `a\"b` → literal `a"b`.
    // Output JSON must contain `a\"b` (JSON-escaped quote), not truncate at
    // the embedded quote as the pre-fix parser did.
    try otel_export.writeAttributes(fbs.writer(), "workspace=\"a\\\"b\"");
    const out = fbs.getWritten();
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "\"stringValue\":\"a\\\"b\""));
    // And the attribute object must close properly (not mid-emit).
    try std.testing.expect(std.mem.endsWith(u8, out, "]"));
}

test "writeAttributes is a no-op for empty label set" {
    var buf: [64]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try otel_export.writeAttributes(fbs.writer(), "");
    try std.testing.expectEqual(@as(usize, 0), fbs.getWritten().len);
}
