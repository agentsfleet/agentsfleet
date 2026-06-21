const std = @import("std");
const mc = @import("metrics_counters.zig");
comptime {
    _ = @import("metrics_fleet.zig");
}

// ── SSE hub counters (dropped frames + reconnects) ──────────────────────

// The hub/subscription behaviour tests assert their local mirrors (dropCount,
// delivery recovery); these pin the operator-facing counters themselves so the
// inc call sites, the snapshot fields, and the render lines cannot silently
// go dead while the behaviour suite stays green.

test "incSseDroppedFrames increments its dedicated counter by 1" {
    const before = mc.snapshot();
    mc.incSseDroppedFrames();
    const after = mc.snapshot();
    try std.testing.expectEqual(before.sse_dropped_frames_total + 1, after.sse_dropped_frames_total);
}

test "incSseHubReconnects increments its dedicated counter by 1" {
    const before = mc.snapshot();
    mc.incSseHubReconnects();
    const after = mc.snapshot();
    try std.testing.expectEqual(before.sse_hub_reconnects_total + 1, after.sse_hub_reconnects_total);
}

test "renderPrometheus carries the SSE hub counter lines with snapshot values" {
    const alloc = std.testing.allocator;
    const render = @import("metrics_render.zig");
    mc.incSseDroppedFrames();
    mc.incSseHubReconnects();
    const snap = mc.snapshot();
    const output = try render.renderPrometheus(alloc, false);
    defer alloc.free(output);
    var dropped_buf: [128]u8 = undefined;
    const dropped = try std.fmt.bufPrint(&dropped_buf, "fleet_sse_dropped_frames_total {d}", .{snap.sse_dropped_frames_total});
    try std.testing.expect(std.mem.indexOf(u8, output, dropped) != null);
    var reconnects_buf: [128]u8 = undefined;
    const reconnects = try std.fmt.bufPrint(&reconnects_buf, "fleet_sse_hub_reconnects_total {d}", .{snap.sse_hub_reconnects_total});
    try std.testing.expect(std.mem.indexOf(u8, output, reconnects) != null);
}

// ── Backpressure counters + gauges ──────────────────────────────────────

test "incApiBackpressureRejections increments its dedicated counter by 1" {
    const before = mc.snapshot();
    mc.incApiBackpressureRejections();
    const after = mc.snapshot();
    try std.testing.expectEqual(before.api_backpressure_rejections_total + 1, after.api_backpressure_rejections_total);
}

test "in-flight gauges reflect the last stored value" {
    mc.setApiInFlightRequests(7);
    mc.setSseInFlightStreams(4);
    const snap = mc.snapshot();
    try std.testing.expectEqual(@as(u64, 7), snap.api_in_flight_requests);
    try std.testing.expectEqual(@as(u64, 4), snap.sse_in_flight_streams);
}
