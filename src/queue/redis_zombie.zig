// M1_001: Zombie Redis stream operations.
//
// Extracted from redis_client.zig to keep files under 500 lines.
// Operates on per-zombie streams (zombie:{zombie_id}:events) using
// XREADGROUP / XAUTOCLAIM / XACK with a shared consumer group.

const std = @import("std");
const queue_consts = @import("constants.zig");
const redis_protocol = @import("redis_protocol.zig");
const redis_client = @import("redis_client.zig");

const log = std.log.scoped(.redis_zombie);

/// ZombieEvent fields decoded from a zombie:{id}:events stream message.
pub const ZombieEvent = struct {
    message_id: []u8,
    event_id: []u8,
    event_type: []u8,
    source: []u8,
    data_json: []u8,

    pub fn deinit(self: *ZombieEvent, alloc: std.mem.Allocator) void {
        alloc.free(self.message_id);
        alloc.free(self.event_id);
        alloc.free(self.event_type);
        alloc.free(self.source);
        alloc.free(self.data_json);
    }
};

/// Build the stream key for a zombie's event stream.
fn zombieStreamKey(buf: []u8, zombie_id: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, "{s}{s}{s}", .{
        queue_consts.zombie_stream_prefix,
        zombie_id,
        queue_consts.zombie_stream_suffix,
    });
}

/// XGROUP CREATE on a zombie's event stream (MKSTREAM, idempotent).
pub fn ensureZombieConsumerGroup(client: *redis_client.Client, zombie_id: []const u8) !void {
    var key_buf: [128]u8 = undefined;
    const stream_key = try zombieStreamKey(&key_buf, zombie_id);
    var resp = try client.commandAllowError(&.{
        "XGROUP",                           "CREATE", stream_key,
        queue_consts.zombie_consumer_group, "0",      "MKSTREAM",
    });
    defer resp.deinit(client.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, "OK")) return error.RedisGroupCreateFailed,
        .err => |msg| {
            if (std.mem.indexOf(u8, msg, "BUSYGROUP") != null) return;
            return error.RedisGroupCreateFailed;
        },
        else => return error.RedisGroupCreateFailed,
    }
}

/// XREADGROUP on zombie:{zombie_id}:events — blocks for 5s waiting for new events.
pub fn xreadgroupZombie(
    client: *redis_client.Client,
    zombie_id: []const u8,
    consumer_id: []const u8,
) !?ZombieEvent {
    var key_buf: [128]u8 = undefined;
    const stream_key = try zombieStreamKey(&key_buf, zombie_id);
    var resp = try client.command(&.{
        "XREADGROUP",                       "GROUP",
        queue_consts.zombie_consumer_group, consumer_id,
        "COUNT",                            queue_consts.zombie_xread_count,
        "BLOCK",                            queue_consts.zombie_xread_block_ms,
        "STREAMS",                          stream_key,
        ">",
    });
    defer resp.deinit(client.alloc);
    return decodeSingleZombieEvent(client.alloc, resp);
}

/// XAUTOCLAIM stale zombie events (idle > 5 min).
pub fn xautoclaimZombie(
    client: *redis_client.Client,
    zombie_id: []const u8,
    consumer_id: []const u8,
) !?ZombieEvent {
    var key_buf: [128]u8 = undefined;
    const stream_key = try zombieStreamKey(&key_buf, zombie_id);
    var resp = try client.command(&.{
        "XAUTOCLAIM",                               stream_key,
        queue_consts.zombie_consumer_group,         consumer_id,
        queue_consts.zombie_xautoclaim_min_idle_ms, queue_consts.xautoclaim_start,
        "COUNT",                                    queue_consts.xautoclaim_count,
    });
    defer resp.deinit(client.alloc);
    return decodeAutoClaimZombieEvent(client.alloc, resp);
}

/// XACK on a zombie event stream after successful processing.
pub fn xackZombie(client: *redis_client.Client, zombie_id: []const u8, message_id: []const u8) !void {
    var key_buf: [128]u8 = undefined;
    const stream_key = try zombieStreamKey(&key_buf, zombie_id);
    var resp = try client.command(&.{
        "XACK",                             stream_key,
        queue_consts.zombie_consumer_group, message_id,
    });
    defer resp.deinit(client.alloc);
    switch (resp) {
        .integer => |v| if (v < 0) {
            log.err("redis.xack_zombie_fail zombie_id={s} message_id={s}", .{ zombie_id, message_id });
            return error.RedisXackFailed;
        },
        else => {
            log.err("redis.xack_zombie_fail zombie_id={s} message_id={s}", .{ zombie_id, message_id });
            return error.RedisXackFailed;
        },
    }
    log.debug("redis.xack_zombie zombie_id={s} message_id={s}", .{ zombie_id, message_id });
}

// ── Decoders ─────────────────────────────────────────────────────────────

fn decodeSingleZombieEvent(alloc: std.mem.Allocator, value: redis_protocol.RespValue) !?ZombieEvent {
    if (value != .array) return null;
    const top = value.array orelse return null;
    if (top.len == 0) return null;
    if (top.len != 1) return error.RedisUnexpectedResponse;
    if (top[0] != .array) return error.RedisUnexpectedResponse;
    const stream_entry = top[0].array orelse return error.RedisUnexpectedResponse;
    if (stream_entry.len != 2) return error.RedisUnexpectedResponse;
    if (stream_entry[1] != .array) return error.RedisUnexpectedResponse;
    const messages = stream_entry[1].array orelse return null;
    if (messages.len == 0) return null;
    return try decodeZombieEventTuple(alloc, messages[0]);
}

fn decodeAutoClaimZombieEvent(alloc: std.mem.Allocator, value: redis_protocol.RespValue) !?ZombieEvent {
    if (value != .array) return null;
    const top = value.array orelse return null;
    if (top.len < 2) return error.RedisUnexpectedResponse;
    if (top[1] != .array) return error.RedisUnexpectedResponse;
    const messages = top[1].array orelse return null;
    if (messages.len == 0) return null;
    return try decodeZombieEventTuple(alloc, messages[0]);
}

fn decodeZombieEventTuple(alloc: std.mem.Allocator, item: redis_protocol.RespValue) !ZombieEvent {
    if (item != .array) return error.RedisUnexpectedResponse;
    const tuple = item.array orelse return error.RedisUnexpectedResponse;
    if (tuple.len != 2) return error.RedisUnexpectedResponse;
    const msg_id = redis_protocol.valueAsString(tuple[0]) orelse return error.RedisUnexpectedResponse;

    if (tuple[1] != .array) return error.RedisUnexpectedResponse;
    const fields = tuple[1].array orelse return error.RedisUnexpectedResponse;
    if (fields.len % 2 != 0) return error.RedisUnexpectedResponse;

    var event_id: ?[]u8 = null;
    var event_type: ?[]u8 = null;
    var source: ?[]u8 = null;
    var data_json: ?[]u8 = null;

    var i: usize = 0;
    while (i < fields.len) : (i += 2) {
        const key = redis_protocol.valueAsString(fields[i]) orelse continue;
        const val = redis_protocol.valueAsString(fields[i + 1]) orelse continue;

        if (std.mem.eql(u8, key, queue_consts.zombie_field_event_id)) {
            event_id = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.zombie_field_type)) {
            event_type = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.zombie_field_source)) {
            source = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.zombie_field_data)) {
            data_json = try alloc.dupe(u8, val);
        }
    }

    if (event_id == null or event_type == null) {
        if (event_id) |e| alloc.free(e);
        if (event_type) |e| alloc.free(e);
        if (source) |s| alloc.free(s);
        if (data_json) |d| alloc.free(d);
        return error.RedisUnexpectedResponse;
    }

    return .{
        .message_id = try alloc.dupe(u8, msg_id),
        .event_id = event_id.?,
        .event_type = event_type.?,
        .source = source orelse try alloc.dupe(u8, ""),
        .data_json = data_json orelse try alloc.dupe(u8, "{}"),
    };
}
