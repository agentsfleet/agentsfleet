// Shared test helpers for harness-binary-driven integration tests.
//
// Both the progress-callbacks test and the heartbeat test connect a Redis
// pub/sub subscriber to `zombie:{id}:activity`, drive the worker through
// writepath.run, and collect the published frames. The helpers below own
// the small bits common to that pattern so each test focuses on its
// scripted scenario + assertions.

const std = @import("std");
const Allocator = std.mem.Allocator;

const redis_pubsub = @import("../queue/redis_pubsub.zig");
const activity_publisher = @import("activity_publisher.zig");

/// Env-var read at the top of every harness test that lets a developer
/// disable the suite without removing the binary from disk.
pub const SKIP_ENV_VAR = "EXECUTOR_HARNESS_SKIP";

/// Env-var that integration tests use to point at a TLS-protected Redis;
/// the queue's default RedisRole reads REDIS_URL which CI doesn't set.
pub const REDIS_TLS_URL_ENV_VAR = "TEST_REDIS_TLS_URL";

/// Single source of truth for the seeded zombie name. Both the parsed
/// config JSON and the seedZombie call must agree on it.
pub const ZOMBIE_NAME = "harness-bot";
pub const ZOMBIE_CONFIG_JSON = "{\"name\":\"" ++ ZOMBIE_NAME ++ "\",\"x-usezombie\":{\"trigger\":{\"type\":\"webhook\",\"source\":\"agentmail\"},\"tools\":[\"agentmail\"],\"budget\":{\"daily_dollars\":5.0}}}";
pub const ZOMBIE_SOURCE_MD = "---\nname: " ++ ZOMBIE_NAME ++ "\n---\n\nYou are a harness bot.\n";

/// Quiet window after the last received frame before the drain returns.
/// Calibrated against the SSE handler's heartbeat cadence on idle channels.
pub const DRAIN_QUIET_MS: u64 = 250;

/// Sleep after Subscriber.subscribe() before assuming SUBSCRIBE landed and
/// the channel is wired up server-side. Mirrors redis_pubsub_test.zig.
pub const SUBSCRIBE_SETTLE_MS: u64 = 200;

/// One frame received from the activity channel, tagged with the wall-clock
/// timestamp at which the test thread observed it. Tests use the timestamps
/// to validate inter-frame intervals (heartbeat cadence, latency).
pub const TimedFrame = struct { kind: []u8, ts_ms: i64 };

/// BUFFER GATE: ArrayList for collected frames — appendable as messages
/// arrive + indexable + sliceable for ordered/timed assertions.
pub const FrameList = std.ArrayList(TimedFrame);

/// Connect a dedicated SUBSCRIBE-only Redis client, or return null when the
/// integration env var is unset (test should skip).
pub fn connectSubscriber(alloc: Allocator) !?redis_pubsub.Subscriber {
    const tls_url = std.process.getEnvVarOwned(alloc, REDIS_TLS_URL_ENV_VAR) catch return null;
    defer alloc.free(tls_url);
    return redis_pubsub.Subscriber.connectFromUrl(alloc, tls_url) catch return null;
}

/// Drain frames from the subscriber until either:
///   - an `event_complete` frame lands (worker reached the terminal marker),
///   - DRAIN_QUIET_MS elapses with no new frames, or
///   - the overall budget expires.
/// Each frame is dup-allocated; caller owns the list (use freeFrames).
pub fn drainFrames(sub: *redis_pubsub.Subscriber, alloc: Allocator, list: *FrameList, budget_ms: u64) !void {
    const overall_deadline = std.time.milliTimestamp() + @as(i64, @intCast(budget_ms));
    var last_msg_at = std.time.milliTimestamp();
    while (std.time.milliTimestamp() < overall_deadline) {
        const msg_opt = try sub.readMessage();
        if (msg_opt) |m| {
            var msg = m;
            defer msg.deinit();
            const kind = try extractKind(alloc, msg.data);
            const now_ms = std.time.milliTimestamp();
            try list.append(alloc, .{ .kind = kind, .ts_ms = now_ms });
            last_msg_at = now_ms;
            if (std.mem.eql(u8, kind, activity_publisher.KIND_EVENT_COMPLETE)) return;
            continue;
        }
        if (std.time.milliTimestamp() - last_msg_at > @as(i64, @intCast(DRAIN_QUIET_MS))) return;
    }
}

/// Parse the `kind` field from an activity-channel JSON payload.
fn extractKind(alloc: Allocator, payload: []const u8) ![]u8 {
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, payload, .{});
    defer parsed.deinit();
    const root = parsed.value;
    if (root != .object) return error.MissingKind;
    const v = root.object.get("kind") orelse return error.MissingKind;
    if (v != .string) return error.MissingKind;
    return alloc.dupe(u8, v.string);
}

/// Free the kind slice on every frame, then the backing list.
pub fn freeFrames(alloc: Allocator, list: *FrameList) void {
    for (list.items) |f| alloc.free(f.kind);
    list.deinit(alloc);
}
