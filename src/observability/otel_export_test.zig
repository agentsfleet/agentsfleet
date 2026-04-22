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

test "writeAttributes is a no-op for empty label set" {
    var buf: [64]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try otel_export.writeAttributes(fbs.writer(), "");
    try std.testing.expectEqual(@as(usize, 0), fbs.getWritten().len);
}
