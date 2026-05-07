//! Zombie Redis stream operations.
//!
//! Operates on per-zombie streams (zombie:{zombie_id}:events) using
//! XREADGROUP / XAUTOCLAIM / XACK with the shared `zombie_workers` consumer
//! group. Decoded entries match the EventEnvelope wire shape; field names
//! must stay in lockstep with `zombie/event_envelope.zig::encodeForXAdd`.

const std = @import("std");
const logging = @import("log");
const queue_consts = @import("constants.zig");
const redis_protocol = @import("redis_protocol.zig");
const redis_client = @import("redis_client.zig");
const error_codes = @import("../errors/error_registry.zig");

const log = logging.scoped(.redis_zombie);

/// ZombieEvent fields decoded from a `zombie:{id}:events` stream message.
///
/// `event_id` IS the Redis stream entry id (`<ms>-<seq>`); it is also the
/// argument passed to `xackZombie`. Producer-side does NOT write a
/// separate event_id field — XADD `*` assigns the id and the worker
/// reads it from the stream entry header.
pub const ZombieEvent = struct {
    event_id: []u8,
    actor: []u8,
    event_type: []u8,
    workspace_id: []u8,
    request_json: []u8,
    created_at_ms: i64,

    pub fn deinit(self: *ZombieEvent, alloc: std.mem.Allocator) void {
        alloc.free(self.event_id);
        alloc.free(self.actor);
        alloc.free(self.event_type);
        alloc.free(self.workspace_id);
        alloc.free(self.request_json);
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
pub fn xackZombie(client: *redis_client.Client, zombie_id: []const u8, event_id: []const u8) !void {
    var key_buf: [128]u8 = undefined;
    const stream_key = try zombieStreamKey(&key_buf, zombie_id);
    var resp = try client.command(&.{
        "XACK",                             stream_key,
        queue_consts.zombie_consumer_group, event_id,
    });
    defer resp.deinit(client.alloc);
    switch (resp) {
        .integer => |v| if (v < 0) {
            log.err("xack_failed", .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .zombie_id = zombie_id, .event_id = event_id });
            return error.RedisXackFailed;
        },
        else => {
            log.err("xack_failed", .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .zombie_id = zombie_id, .event_id = event_id });
            return error.RedisXackFailed;
        },
    }
    log.debug("xack_succeeded", .{ .zombie_id = zombie_id, .event_id = event_id });
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
    const event_id_raw = redis_protocol.valueAsString(tuple[0]) orelse return error.RedisUnexpectedResponse;

    if (tuple[1] != .array) return error.RedisUnexpectedResponse;
    const fields = tuple[1].array orelse return error.RedisUnexpectedResponse;
    if (fields.len % 2 != 0) return error.RedisUnexpectedResponse;

    var event_type: ?[]u8 = null;
    var actor: ?[]u8 = null;
    var workspace_id: ?[]u8 = null;
    var request_json: ?[]u8 = null;
    var created_at_ms: i64 = 0;

    var i: usize = 0;
    while (i < fields.len) : (i += 2) {
        const key = redis_protocol.valueAsString(fields[i]) orelse continue;
        const val = redis_protocol.valueAsString(fields[i + 1]) orelse continue;

        if (std.mem.eql(u8, key, queue_consts.zombie_field_type)) {
            event_type = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.zombie_field_actor)) {
            actor = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.zombie_field_workspace_id)) {
            workspace_id = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.zombie_field_request)) {
            request_json = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.zombie_field_created_at)) {
            created_at_ms = std.fmt.parseInt(i64, val, 10) catch 0;
        }
    }

    if (event_type == null or actor == null) {
        if (event_type) |e| alloc.free(e);
        if (actor) |a| alloc.free(a);
        if (workspace_id) |w| alloc.free(w);
        if (request_json) |r| alloc.free(r);
        return error.RedisUnexpectedResponse;
    }

    return .{
        .event_id = try alloc.dupe(u8, event_id_raw),
        .event_type = event_type.?,
        .actor = actor.?,
        .workspace_id = workspace_id orelse try alloc.dupe(u8, ""),
        .request_json = request_json orelse try alloc.dupe(u8, "{}"),
        .created_at_ms = created_at_ms,
    };
}
