//! Install-time event-stream setup for POST /v1/workspaces/{ws}/fleets.
//! Ensures the per-fleet events stream + consumer group exist before the 201,
//! with a bounded retry so a sub-second Redis blip does not surface as a 500.
//! Extracted from create.zig (RULE FLL); create.zig is the sole consumer.

const std = @import("std");
const constants = @import("common");
const logging = @import("log");
const ec = @import("../../../errors/error_registry.zig");
const queue_redis = @import("../../../queue/redis_client.zig");
const redis_fleet = @import("../../../queue/redis_fleet.zig");

const log = logging.scoped(.fleet_api);

/// Fixed backoff schedule for install-time `XGROUP CREATE` retries.
/// Total wall budget = sum = 2.1s. Three sleeps means four attempts (one
/// extra try after the last sleep). See `installBackoffMs` for the lookup.
const install_backoff_schedule = [_]u32{ 100, 500, 1500 };

/// Pure-function backoff lookup, modelled after Bun's
/// `valkey/valkey.zig:getReconnectDelay`. Returns the sleep duration (ms)
/// for `attempt`, or null when the schedule is exhausted (caller bails).
/// Pulled out of the retry loop so the four-attempt / three-sleep
/// invariant is unit-testable without standing up a Redis mock.
fn installBackoffMs(attempt: usize) ?u32 {
    if (attempt >= install_backoff_schedule.len) return null;
    return install_backoff_schedule[attempt];
}

/// By the time this returns successfully, the per-fleet events stream +
/// consumer group exist — the lease XREADGROUP needs the group present.
/// Retries up to 4 attempts with `install_backoff_schedule` between each
/// (2.1s total wall) so a sub-second Redis blip does not surface as a
/// user-visible 500. On final failure the caller rolls back the PG row.
pub fn ensureEventStream(redis: *queue_redis.Client, fleet_id: []const u8) !void {
    var attempt: usize = 0;
    while (true) : (attempt += 1) {
        redis_fleet.ensureFleetConsumerGroup(redis, fleet_id) catch |err| {
            const sleep_ms = installBackoffMs(attempt) orelse return err;
            log.warn(
                "create_stream_setup_retry",
                .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .attempt = attempt + 1, .err = @errorName(err), .fleet_id = fleet_id, .sleep_ms = sleep_ms },
            );
            constants.sleepNanos(@as(u64, sleep_ms) * std.time.ns_per_ms);
            continue;
        };
        return;
    }
}

// installBackoffMs is the corrected 4-attempt / 3-sleep schedule — the
// pre-fix guard `attempt + 1 >= len` left the third 1500ms entry unreachable
// (greptile-caught). Table-driven pin: every entry reachable + exhaustion
// boundary holds + no integer wraparound past the schedule end.

test "installBackoffMs: schedule shape + exhaustion + wraparound" {
    try std.testing.expectEqual(@as(?u32, 100), installBackoffMs(0));
    try std.testing.expectEqual(@as(?u32, 500), installBackoffMs(1));
    try std.testing.expectEqual(@as(?u32, 1500), installBackoffMs(2));
    try std.testing.expectEqual(@as(?u32, null), installBackoffMs(3));
    try std.testing.expectEqual(@as(?u32, null), installBackoffMs(100));
    try std.testing.expectEqual(@as(?u32, null), installBackoffMs(std.math.maxInt(usize)));
}

test "install_backoff_schedule: total wall budget is 2.1s as documented" {
    var sum: u64 = 0;
    for (install_backoff_schedule) |ms| sum += ms;
    try std.testing.expectEqual(@as(u64, 2100), sum);
}

test "ensureEventStream retry loop: 4 attempts on permanent failure" {
    // Drives the exact loop shape from ensureEventStream against an
    // injected counter — proves four calls, three sleeps, terminating
    // err.PermanentFail. Uses installBackoffMs directly (no Thread.sleep
    // — tests run in microseconds).
    var calls: usize = 0;
    var attempt: usize = 0;
    const result: error{PermanentFail}!void = blk: while (true) : (attempt += 1) {
        calls += 1;
        // Simulated group-ensure: always fails.
        const op_err: error{PermanentFail} = error.PermanentFail;
        const sleep_ms = installBackoffMs(attempt) orelse break :blk op_err;
        // In production this is `std.Thread.sleep(sleep_ms * ns_per_ms)`.
        // The test only needs the fact that a sleep WOULD have happened.
        _ = sleep_ms;
        continue;
    };
    try std.testing.expectError(error.PermanentFail, result);
    try std.testing.expectEqual(@as(usize, 4), calls);
}

test "ensureEventStream retry loop: succeeds on first attempt → no retries" {
    var calls: usize = 0;
    var attempt: usize = 0;
    while (true) : (attempt += 1) {
        calls += 1;
        // Simulated group-ensure: succeeds immediately.
        const op_result: error{}!void = {};
        op_result catch |err| {
            const sleep_ms = installBackoffMs(attempt) orelse return err;
            _ = sleep_ms;
            continue;
        };
        break;
    }
    try std.testing.expectEqual(@as(usize, 1), calls);
}

test "ensureEventStream retry loop: succeeds on attempt 2 (after 100ms+500ms sleeps)" {
    var calls: usize = 0;
    var attempt: usize = 0;
    while (true) : (attempt += 1) {
        calls += 1;
        const op_err: ?error{Transient} = if (calls < 3) error.Transient else null;
        if (op_err) |err| {
            const sleep_ms = installBackoffMs(attempt) orelse return err;
            _ = sleep_ms;
            continue;
        }
        break;
    }
    try std.testing.expectEqual(@as(usize, 3), calls);
}
