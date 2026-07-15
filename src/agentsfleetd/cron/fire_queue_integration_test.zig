//! Real-Redis proofs for atomic cron replay suppression and event append.

const std = @import("std");
const common = @import("common");

const FireQueue = @import("FireQueue.zig");
const queue_constants = @import("../queue/constants.zig");
const queue_redis = @import("../queue/redis.zig");

const CONTENDERS: usize = 100;
const TEST_REDIS_URL_ENV: [:0]const u8 = "TEST_REDIS_TLS_URL";
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-105000000401";
const SCHEDULE_ID = "0195b4ba-8d3a-7f13-8abc-105000000402";
const SIGNED_MESSAGE_ID = "jwt_m105_fire";
const PROVIDER_MESSAGE_ID = "msg_m105_fire";
const FIRED_AT_MS: i64 = 1000;
const REQUEST_JSON_FORMAT = "{{\"message\":\"summarize\",\"schedule_id\":\"" ++ SCHEDULE_ID ++ "\",\"generation\":1,\"fired_at\":{d}}}";
const REQUEST_JSON = std.fmt.comptimePrint(REQUEST_JSON_FORMAT, .{FIRED_AT_MS});

const Counts = struct {
    enqueued: std.atomic.Value(u32) = .init(0),
    duplicate: std.atomic.Value(u32) = .init(0),
    errors: std.atomic.Value(u32) = .init(0),
};

const Worker = struct {
    queue: FireQueue,
    fleet_id: []const u8,
    ready: *std.atomic.Value(u32),
    gate: *std.atomic.Value(bool),
    counts: *Counts,

    fn run(self: Worker) void {
        _ = self.ready.fetchAdd(1, .acq_rel);
        while (!self.gate.load(.acquire)) std.atomic.spinLoopHint();
        const result = self.queue.enqueue(
            self.fleet_id,
            WORKSPACE_ID,
            SCHEDULE_ID,
            SIGNED_MESSAGE_ID,
            PROVIDER_MESSAGE_ID,
            REQUEST_JSON,
            FIRED_AT_MS,
        ) catch {
            _ = self.counts.errors.fetchAdd(1, .monotonic);
            return;
        };
        switch (result) {
            .enqueued => _ = self.counts.enqueued.fetchAdd(1, .monotonic),
            .duplicate => _ = self.counts.duplicate.fetchAdd(1, .monotonic),
        }
    }
};

fn redisOrSkip(alloc: std.mem.Allocator) !queue_redis.Client {
    const url = common.env.testLiveValue(TEST_REDIS_URL_ENV) orelse return error.SkipZigTest;
    return queue_redis.testing.connectFromUrl(common.globalIo(), alloc, url);
}

fn keyFor(buffer: []u8, fleet_id: []const u8, kind: []const u8, message_id: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buffer, "cron:dedup:{s}:{s}:{s}", .{ fleet_id, kind, message_id });
}

fn streamFor(buffer: []u8, fleet_id: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buffer, "{s}{s}{s}", .{
        queue_constants.fleet_stream_prefix,
        fleet_id,
        queue_constants.fleet_stream_suffix,
    });
}

fn deleteKeys(client: *queue_redis.Client, alloc: std.mem.Allocator, keys: []const []const u8) void {
    var argv: [8][]const u8 = undefined;
    argv[0] = "DEL";
    for (keys, 0..) |key, index| argv[index + 1] = key;
    var response = client.command(argv[0 .. keys.len + 1]) catch return;
    response.deinit(alloc);
}

fn expectStreamLength(client: *queue_redis.Client, alloc: std.mem.Allocator, stream: []const u8, expected: i64) !void {
    var response = try client.command(&.{ "XLEN", stream });
    defer response.deinit(alloc);
    try std.testing.expectEqual(expected, response.integer);
}

test "fire queue: signed and provider identities each suppress replay atomically" {
    const alloc = std.testing.allocator;
    var client = try redisOrSkip(alloc);
    defer client.deinit();
    const fleet_id = try std.fmt.allocPrint(alloc, "m105-fire-{d}", .{common.clock.nowNanos()});
    defer alloc.free(fleet_id);
    var signed_buffer: [384]u8 = undefined;
    const signed_key = try keyFor(&signed_buffer, fleet_id, "jwt", SIGNED_MESSAGE_ID);
    var provider_buffer: [384]u8 = undefined;
    const provider_key = try keyFor(&provider_buffer, fleet_id, "msg", PROVIDER_MESSAGE_ID);
    var stream_buffer: [128]u8 = undefined;
    const stream = try streamFor(&stream_buffer, fleet_id);
    const keys = [_][]const u8{ signed_key, provider_key, stream };
    deleteKeys(&client, alloc, &keys);
    defer deleteKeys(&client, alloc, &keys);

    const queue = FireQueue.init(alloc, &client);
    try std.testing.expectEqual(FireQueue.Outcome.enqueued, try queue.enqueue(
        fleet_id,
        WORKSPACE_ID,
        SCHEDULE_ID,
        SIGNED_MESSAGE_ID,
        PROVIDER_MESSAGE_ID,
        REQUEST_JSON,
        FIRED_AT_MS,
    ));
    try std.testing.expectEqual(FireQueue.Outcome.duplicate, try queue.enqueue(
        fleet_id,
        WORKSPACE_ID,
        SCHEDULE_ID,
        SIGNED_MESSAGE_ID,
        "msg_changed",
        REQUEST_JSON,
        FIRED_AT_MS,
    ));
    try std.testing.expectEqual(FireQueue.Outcome.duplicate, try queue.enqueue(
        fleet_id,
        WORKSPACE_ID,
        SCHEDULE_ID,
        "jwt_changed",
        PROVIDER_MESSAGE_ID,
        REQUEST_JSON,
        FIRED_AT_MS,
    ));
    try expectStreamLength(&client, alloc, stream, 1);
}

test "fire queue: 100 simultaneous copies append exactly once without a process lock" {
    const alloc = std.testing.allocator;
    var client = try redisOrSkip(alloc);
    defer client.deinit();
    const fleet_id = try std.fmt.allocPrint(alloc, "m105-fire-cc-{d}", .{common.clock.nowNanos()});
    defer alloc.free(fleet_id);
    var signed_buffer: [384]u8 = undefined;
    const signed_key = try keyFor(&signed_buffer, fleet_id, "jwt", SIGNED_MESSAGE_ID);
    var provider_buffer: [384]u8 = undefined;
    const provider_key = try keyFor(&provider_buffer, fleet_id, "msg", PROVIDER_MESSAGE_ID);
    var stream_buffer: [128]u8 = undefined;
    const stream = try streamFor(&stream_buffer, fleet_id);
    const keys = [_][]const u8{ signed_key, provider_key, stream };
    deleteKeys(&client, alloc, &keys);
    defer deleteKeys(&client, alloc, &keys);

    var threads: [CONTENDERS]std.Thread = undefined;
    var ready = std.atomic.Value(u32).init(0);
    var gate = std.atomic.Value(bool).init(false);
    var counts: Counts = .{};
    const worker: Worker = .{
        .queue = FireQueue.init(alloc, &client),
        .fleet_id = fleet_id,
        .ready = &ready,
        .gate = &gate,
        .counts = &counts,
    };
    var spawned: usize = 0;
    errdefer {
        gate.store(true, .release);
        for (threads[0..spawned]) |*thread| thread.join();
    }
    for (&threads) |*thread| {
        thread.* = try std.Thread.spawn(.{}, Worker.run, .{worker});
        spawned += 1;
    }
    while (ready.load(.acquire) != CONTENDERS) std.atomic.spinLoopHint();
    gate.store(true, .release);
    for (&threads) |*thread| thread.join();
    spawned = 0;

    try std.testing.expectEqual(@as(u32, 1), counts.enqueued.load(.acquire));
    try std.testing.expectEqual(@as(u32, CONTENDERS - 1), counts.duplicate.load(.acquire));
    try std.testing.expectEqual(@as(u32, 0), counts.errors.load(.acquire));
    try expectStreamLength(&client, alloc, stream, 1);
}
