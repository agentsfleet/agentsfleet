//! Fleet event structs.
//! Extracted from telemetry_test.zig to keep both under the 350-line gate.

const std = @import("std");
const telemetry = @import("telemetry.zig");
const common = @import("common");
const posthog = @import("posthog");

/// Batch knobs set far above anything the capture ladder below emits, so the
/// single event it enqueues is never batched out and the network stays
/// untouched. Named rather than spelled inline: both are "effectively never",
/// not a tuned value, and the reason belongs with the number.
const NEVER_FLUSH_INTERVAL_MS = 3_600_000;
const NEVER_FLUSH_AT = 1_000;

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
    const event = telemetry.FleetCompleted.init(.{
        .distinct_id = "ws_1",
        .workspace_id = "ws_1",
        .fleet_id = "z_1",
        .event_id = "e_1",
        .tokens = 1500,
        .wall_ms = 4200,
        .exit_status = "processed",
        .time_to_first_token_ms = 0,
    });
    const props = event.properties();
    try std.testing.expectEqual(@as(usize, 8), props.len);
    try std.testing.expectEqualStrings("tokens", props[3].key);
    try std.testing.expectEqual(@as(i64, 1500), props[3].value.integer);
    try std.testing.expectEqualStrings("wall_ms", props[4].key);
    try std.testing.expectEqual(@as(i64, 4200), props[4].value.integer);
    try std.testing.expectEqualStrings("exit_status", props[5].key);
    try std.testing.expectEqualStrings("time_to_first_token_ms", props[6].key);
    try std.testing.expectEqual(@as(i64, 0), props[6].value.integer);
    try std.testing.expectEqualStrings("$insert_id", props[7].key);
    try std.testing.expectEqualStrings(
        "29fb04e3980c341d5dd2f564506ca1b157dee37b5d8499721f06bff885f03529",
        props[7].value.string,
    );
}

test "test_fleet_completed_saturates_runner_u64_properties" {
    const event = telemetry.FleetCompleted.init(.{
        .distinct_id = "ws",
        .workspace_id = "ws",
        .fleet_id = "z",
        .event_id = "e",
        .tokens = std.math.maxInt(u64),
        .wall_ms = std.math.maxInt(u64),
        .exit_status = "processed",
        .time_to_first_token_ms = std.math.maxInt(u64),
    });
    const props = event.properties();
    try std.testing.expectEqual(std.math.maxInt(i64), props[3].value.integer);
    try std.testing.expectEqual(std.math.maxInt(i64), props[4].value.integer);
    try std.testing.expectEqual(std.math.maxInt(i64), props[6].value.integer);
}

// The end-to-end "one accepted report → one capture" proof lives in the fleet
// round-trip integration suite, which drives the real handler. This one only
// pins that a captured FleetCompleted lands under its own event kind.
test "FleetCompleted captures under its own event kind" {
    var tel = telemetry.Telemetry.initTest();
    const event = telemetry.FleetCompleted.init(.{
        .distinct_id = "ws",
        .workspace_id = "ws",
        .fleet_id = "z",
        .event_id = "e",
        .tokens = 1,
        .wall_ms = 2,
        .exit_status = "processed",
        .time_to_first_token_ms = 3,
    });
    tel.capture(telemetry.FleetCompleted, event);
    try telemetry.TestBackend.assertCount(1);
    try telemetry.TestBackend.assertLastEventIs(.fleet_completed);
}

test "test_completion_analytics_remains_optional" {
    var prod = telemetry.ProdBackend{ .client = null };
    prod.capture(telemetry.FleetTriggered, .{
        .distinct_id = "ws",
        .workspace_id = "ws",
        .fleet_id = "z",
        .event_id = "e",
        .source = "webhook",
    });
    prod.capture(telemetry.FleetCompleted, telemetry.FleetCompleted.init(.{
        .distinct_id = "ws",
        .workspace_id = "ws",
        .fleet_id = "z",
        .event_id = "e",
        .tokens = 0,
        .wall_ms = 0,
        .exit_status = "deliver_error",
        .time_to_first_token_ms = 0,
    }));
    // Reaching here without panic is the pass condition.
}

test "a capture the client cannot serialize is swallowed, never propagated" {
    // `ProdBackend.capture` returns void: the caller emitting an analytics
    // event has no way to handle a telemetry failure and must not be made to.
    // The existing sibling test covers the null-client early return; this one
    // reaches the arm past it, where a real client's `capture` fails.
    //
    // The client is built on a failing allocator whose budget runs out during
    // `serializeEvent`. `has_induced_failure` separates the two outcomes: it
    // is false while the ladder is still failing `init` (which returns early),
    // and true only once init succeeded and the capture itself was starved —
    // which is the arm under test, asserted rather than merely driven.
    const alloc = std.testing.allocator;
    const event = telemetry.FleetTriggered{
        .distinct_id = "ws_capture",
        .workspace_id = "ws_capture",
        .fleet_id = "z_capture",
        .event_id = "e_capture",
        .source = "webhook",
    };

    var proved = false;
    for (0..64) |fail_index| {
        var failing = std.testing.FailingAllocator.init(alloc, .{ .fail_index = fail_index });
        const client = posthog.init(failing.allocator(), common.globalIo(), .{
            .api_key = "phc_capture_ladder",
            .host = "https://us.i.posthog.com",
            .flush_interval_ms = NEVER_FLUSH_INTERVAL_MS,
            .flush_at = NEVER_FLUSH_AT,
            .max_retries = 1,
        }) catch continue;
        defer client.deinit();

        var prod = telemetry.ProdBackend{ .client = client };
        prod.capture(telemetry.FleetTriggered, event);

        if (failing.has_induced_failure) {
            proved = true;
            break;
        }
    }
    try std.testing.expect(proved);
}
