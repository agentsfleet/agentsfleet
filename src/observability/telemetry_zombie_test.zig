//! M15_002 — Zombie event structs (spec §6.0 rows 2.1 / 2.3).
//! Extracted from telemetry_test.zig to keep both under the 350-line gate.

const std = @import("std");
const telemetry = @import("telemetry.zig");

// Spec §6.0 row 2.1 — ZombieTriggered / ZombieCompleted comptime struct shape.
test "M15_002 2.1: ZombieTriggered properties return expected keys" {
    const props = (telemetry.ZombieTriggered{
        .distinct_id = "ws_1",
        .workspace_id = "ws_1",
        .zombie_id = "z_1",
        .event_id = "e_1",
        .source = "webhook",
    }).properties();
    try std.testing.expectEqual(@as(usize, 4), props.len);
    try std.testing.expectEqualStrings("workspace_id", props[0].key);
    try std.testing.expectEqualStrings("zombie_id", props[1].key);
    try std.testing.expectEqualStrings("event_id", props[2].key);
    try std.testing.expectEqualStrings("source", props[3].key);
}

test "M15_002 2.1: ZombieCompleted properties return expected keys and numeric types" {
    const props = (telemetry.ZombieCompleted{
        .distinct_id = "ws_1",
        .workspace_id = "ws_1",
        .zombie_id = "z_1",
        .event_id = "e_1",
        .tokens = 1500,
        .wall_ms = 4200,
        .exit_status = "processed",
    }).properties();
    try std.testing.expectEqual(@as(usize, 6), props.len);
    try std.testing.expectEqualStrings("tokens", props[3].key);
    try std.testing.expectEqual(@as(i64, 1500), props[3].value.integer);
    try std.testing.expectEqualStrings("wall_ms", props[4].key);
    try std.testing.expectEqual(@as(i64, 4200), props[4].value.integer);
    try std.testing.expectEqualStrings("exit_status", props[5].key);
}

// Spec §6.0 row 2.3 — null PostHog client must not panic on Zombie captures.
// ProdBackend is instantiated directly because the comptime-selected Backend
// in test builds is TestBackend.
test "M15_002 2.3: ProdBackend with null client does not panic on Zombie capture" {
    var prod = telemetry.ProdBackend{ .client = null };
    prod.capture(telemetry.ZombieTriggered, .{
        .distinct_id = "ws",
        .workspace_id = "ws",
        .zombie_id = "z",
        .event_id = "e",
        .source = "webhook",
    });
    prod.capture(telemetry.ZombieCompleted, .{
        .distinct_id = "ws",
        .workspace_id = "ws",
        .zombie_id = "z",
        .event_id = "e",
        .tokens = 0,
        .wall_ms = 0,
        .exit_status = "deliver_error",
    });
    // Reaching here without panic is the pass condition.
}
