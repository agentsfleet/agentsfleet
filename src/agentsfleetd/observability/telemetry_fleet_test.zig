//! Fleet event structs.
//! Extracted from telemetry_test.zig to keep both under the 350-line gate.

const std = @import("std");
const telemetry = @import("telemetry.zig");

// FleetTriggered / FleetCompleted comptime struct shape.
test "FleetTriggered properties return expected keys" {
    const props = (telemetry.FleetTriggered{
        .distinct_id = "ws_1",
        .workspace_id = "ws_1",
        .fleet_id = "z_1",
        .event_id = "e_1",
        .source = "webhook",
    }).properties();
    try std.testing.expectEqual(@as(usize, 4), props.len);
    try std.testing.expectEqualStrings("workspace_id", props[0].key);
    try std.testing.expectEqualStrings("fleet_id", props[1].key);
    try std.testing.expectEqualStrings("event_id", props[2].key);
    try std.testing.expectEqualStrings("source", props[3].key);
}

test "FleetCompleted properties return expected keys and numeric types" {
    const props = (telemetry.FleetCompleted{
        .distinct_id = "ws_1",
        .workspace_id = "ws_1",
        .fleet_id = "z_1",
        .event_id = "e_1",
        .tokens = 1500,
        .wall_ms = 4200,
        .exit_status = "processed",
    }).properties();
    try std.testing.expectEqual(@as(usize, 7), props.len);
    try std.testing.expectEqualStrings("tokens", props[3].key);
    try std.testing.expectEqual(@as(i64, 1500), props[3].value.integer);
    try std.testing.expectEqualStrings("wall_ms", props[4].key);
    try std.testing.expectEqual(@as(i64, 4200), props[4].value.integer);
    try std.testing.expectEqualStrings("exit_status", props[5].key);
    try std.testing.expectEqualStrings("time_to_first_token_ms", props[6].key);
    try std.testing.expectEqual(@as(i64, 0), props[6].value.integer);
}

// null PostHog client must not panic on Fleet captures.
// ProdBackend is instantiated directly because the comptime-selected Backend
// in test builds is TestBackend.
test "ProdBackend with null client does not panic on Fleet capture" {
    var prod = telemetry.ProdBackend{ .client = null };
    prod.capture(telemetry.FleetTriggered, .{
        .distinct_id = "ws",
        .workspace_id = "ws",
        .fleet_id = "z",
        .event_id = "e",
        .source = "webhook",
    });
    prod.capture(telemetry.FleetCompleted, .{
        .distinct_id = "ws",
        .workspace_id = "ws",
        .fleet_id = "z",
        .event_id = "e",
        .tokens = 0,
        .wall_ms = 0,
        .exit_status = "deliver_error",
    });
    // Reaching here without panic is the pass condition.
}
