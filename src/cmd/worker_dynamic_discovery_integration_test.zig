//! Integration smoke coverage for the M40 worker substrate seam.
//!
//! Spec §Test Specification listed six end-to-end timing assertions
//! (≤1s install-to-claim, ≤200ms kill, drain in-flight count, etc.) — those
//! require a full API + worker harness that does not exist yet and would
//! triple this file's blast radius. This file covers the three lower-level
//! invariants that prove the control-stream seam compiles and round-trips
//! against real Redis:
//!
//!   1. `control_stream.ensureControlGroup` is idempotent — second call on
//!      an existing group returns BUSYGROUP-as-success rather than failing.
//!   2. `control_stream.publish` + `control_stream.decodeEntry` round-trip
//!      through a real `XADD` + `XREADGROUP` cycle.
//!   3. The per-zombie events stream group setup (`XGROUP CREATE MKSTREAM
//!      zombie:{id}:events ...`) is also idempotent — the install path
//!      depends on this for retried installs.
//!
//! Higher-level timing + failure-injection tests (kill ≤200ms, watcher
//! reconnect, XADD-failure rollback) are deferred to a follow-up spec
//! that builds the harness.
//!
//! Skips when `REDIS_URL_WORKER` is unset (local dev without Redis).

const std = @import("std");
const queue_redis = @import("../queue/redis_client.zig");
const redis_protocol = @import("../queue/redis_protocol.zig");
const control_stream = @import("../zombie/control_stream.zig");

fn connectRedisOrSkip(alloc: std.mem.Allocator) !?queue_redis.Client {
    const url = std.process.getEnvVarOwned(alloc, "REDIS_URL_WORKER") catch return null;
    defer alloc.free(url);
    return try queue_redis.Client.connectFromUrl(alloc, url);
}

/// Generate a unique stream key suffix so each test run starts from a clean
/// state. (Redis `XGROUP DESTROY` would also work but adds a teardown call
/// that can fail on flaky connections.)
fn uniqueSuffix() u64 {
    return @as(u64, @intCast(std.time.milliTimestamp()));
}

test "integration: ensureControlGroup is idempotent" {
    const alloc = std.testing.allocator;
    var client = (try connectRedisOrSkip(alloc)) orelse return error.SkipZigTest;
    defer client.deinit();

    // First call — creates the group (or BUSYGROUPs if a prior run left it).
    try control_stream.ensureControlGroup(&client);
    // Second call — must not error. BUSYGROUP is the expected branch.
    try control_stream.ensureControlGroup(&client);
}

test "integration: publish + decodeEntry round-trip via real Redis" {
    const alloc = std.testing.allocator;
    var client = (try connectRedisOrSkip(alloc)) orelse return error.SkipZigTest;
    defer client.deinit();

    try control_stream.ensureControlGroup(&client);

    var zombie_id_buf: [40]u8 = undefined;
    const zombie_id = try std.fmt.bufPrint(&zombie_id_buf, "z-test-{d}", .{uniqueSuffix()});
    const workspace_id = "ws-test-1";

    try control_stream.publish(&client, .{
        .zombie_created = .{
            .zombie_id = zombie_id,
            .workspace_id = workspace_id,
        },
    });

    // Read the message back via XREADGROUP. Use a unique consumer name so
    // we don't compete with any other test or watcher.
    var consumer_buf: [64]u8 = undefined;
    const consumer = try std.fmt.bufPrint(&consumer_buf, "test-consumer-{d}", .{uniqueSuffix()});

    var resp = try client.command(&.{
        "XREADGROUP",   "GROUP",
        control_stream.consumer_group,
        consumer,       "COUNT",
        "16",           "BLOCK",
        "1000",         "STREAMS",
        control_stream.stream_key, ">",
    });
    defer resp.deinit(client.alloc);

    // The group may have older messages from prior runs — walk all entries
    // and find the one matching our zombie_id.
    var found = false;
    if (resp == .array) {
        if (resp.array) |top| {
            if (top.len > 0 and top[0] == .array) {
                const stream_tuple = top[0].array.?;
                if (stream_tuple.len == 2 and stream_tuple[1] == .array) {
                    const entries = stream_tuple[1].array.?;
                    for (entries) |entry| {
                        if (entry != .array) continue;
                        const tuple = entry.array.?;
                        if (tuple.len != 2) continue;
                        const msg_id = redis_protocol.valueAsString(tuple[0]) orelse continue;
                        if (tuple[1] != .array) continue;
                        const fields = tuple[1].array.?;

                        var decoded = control_stream.decodeEntry(alloc, msg_id, fields) catch continue;
                        defer decoded.deinit(alloc);

                        switch (decoded.message) {
                            .zombie_created => |m| {
                                if (std.mem.eql(u8, m.zombie_id, zombie_id)) {
                                    try std.testing.expectEqualStrings(workspace_id, m.workspace_id);
                                    found = true;
                                    break;
                                }
                            },
                            else => {},
                        }
                    }
                }
            }
        }
    }
    try std.testing.expect(found);
}

test "integration: per-zombie events group create is idempotent" {
    const alloc = std.testing.allocator;
    var client = (try connectRedisOrSkip(alloc)) orelse return error.SkipZigTest;
    defer client.deinit();

    var zombie_id_buf: [40]u8 = undefined;
    const zombie_id = try std.fmt.bufPrint(&zombie_id_buf, "z-events-{d}", .{uniqueSuffix()});

    // Mirror of the install path's helper — XGROUP CREATE MKSTREAM with
    // BUSYGROUP-as-success. Two consecutive calls should both succeed.
    var stream_key_buf: [128]u8 = undefined;
    const stream_key = try std.fmt.bufPrint(&stream_key_buf, "zombie:{s}:events", .{zombie_id});

    inline for (0..2) |_| {
        var resp = try client.commandAllowError(&.{
            "XGROUP", "CREATE", stream_key, "zombie_workers", "0", "MKSTREAM",
        });
        defer resp.deinit(client.alloc);

        const ok = switch (resp) {
            .simple => |v| std.mem.eql(u8, v, "OK"),
            .err => |msg| std.mem.indexOf(u8, msg, "BUSYGROUP") != null,
            else => false,
        };
        try std.testing.expect(ok);
    }
}
