//! Decoding for `fleet:{id}:events` stream replies.
//!
//! Split out of `redis_fleet.zig` (which owns the commands) so each file stays
//! reviewable: this one turns RESP values into owned `FleetEvent`s and knows
//! nothing about connections, retries, or command shapes. Field names must stay
//! in lockstep with `contract/event_envelope.zig::encodeForXAdd` — that module is
//! the producer side of the same wire shape.
//!
//! `redis_fleet.zig` re-exports `FleetEvent`, so callers keep naming it
//! `redis_fleet.FleetEvent` and this module never imports its parent.

const std = @import("std");
const queue_consts = @import("constants.zig");
const redis_protocol = @import("redis_protocol.zig");

/// Default for a stream entry that omits the optional workspace_id field.
pub const EMPTY_WORKSPACE = "";
/// Default request body for an entry that omits the optional request field.
pub const EMPTY_JSON_BODY = "{}";

/// FleetEvent fields decoded from a `fleet:{id}:events` stream message.
///
/// `event_id` IS the Redis stream entry id (`<ms>-<seq>`); it is also the
/// argument passed to `xackFleet`. The producer does NOT write a separate
/// event_id field — XADD `*` assigns the id and the reader takes it from the
/// stream entry header.
pub const FleetEvent = struct {
    const Self = @This();

    event_id: []u8,
    actor: []u8,
    event_type: []u8,
    workspace_id: []u8,
    request_json: []u8,
    created_at_ms: i64,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.event_id);
        alloc.free(self.actor);
        alloc.free(self.event_type);
        alloc.free(self.workspace_id);
        alloc.free(self.request_json);
        self.* = undefined;
    }
};

pub fn decodeSingleFleetEvent(alloc: std.mem.Allocator, value: redis_protocol.RespValue) !?FleetEvent {
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
    return try decodeFleetEventTuple(alloc, messages[0]);
}

pub fn decodeAutoClaimFleetEvent(alloc: std.mem.Allocator, value: redis_protocol.RespValue) !?FleetEvent {
    if (value != .array) return null;
    const top = value.array orelse return null;
    if (top.len < 2) return error.RedisUnexpectedResponse;
    if (top[1] != .array) return error.RedisUnexpectedResponse;
    const messages = top[1].array orelse return null;
    if (messages.len == 0) return null;
    return try decodeFleetEventTuple(alloc, messages[0]);
}

pub fn decodeFleetEventTuple(alloc: std.mem.Allocator, item: redis_protocol.RespValue) !FleetEvent {
    if (item != .array) return error.RedisUnexpectedResponse;
    const tuple = item.array orelse return error.RedisUnexpectedResponse;
    if (tuple.len != 2) return error.RedisUnexpectedResponse;
    const event_id_raw = redis_protocol.valueAsString(tuple[0]) orelse return error.RedisUnexpectedResponse;

    if (tuple[1] != .array) return error.RedisUnexpectedResponse;
    const fields = tuple[1].array orelse return error.RedisUnexpectedResponse;
    if (fields.len % 2 != 0) return error.RedisUnexpectedResponse;

    var parsed = try parseEventFields(alloc, fields);
    if (parsed.event_type == null or parsed.actor == null) {
        parsed.freeOwned(alloc);
        return error.RedisUnexpectedResponse;
    }
    return parsed.intoOwned(alloc, event_id_raw);
}

/// Owned (heap-duped) fields decoded from the XADD field array, before the
/// required-field check promotes them into a fully-owned `FleetEvent`. Each
/// non-null slice is caller-owned; `freeOwned` releases whatever is present.
const ParsedFields = struct {
    const Self = @This();

    event_type: ?[]u8 = null,
    actor: ?[]u8 = null,
    workspace_id: ?[]u8 = null,
    request_json: ?[]u8 = null,
    created_at_ms: i64 = 0,

    fn freeOwned(self: Self, alloc: std.mem.Allocator) void {
        if (self.event_type) |e| alloc.free(e);
        if (self.actor) |a| alloc.free(a);
        if (self.workspace_id) |w| alloc.free(w);
        if (self.request_json) |r| alloc.free(r);
    }

    /// Promote to a fully-owned `FleetEvent`. Each dupe lands in a local with
    /// its own `errdefer` so a late OOM frees every already-owned slice — Zig
    /// does not unwind earlier struct-literal fields when a later one errors.
    fn intoOwned(self: Self, alloc: std.mem.Allocator, event_id_raw: []const u8) !FleetEvent {
        const event_type = self.event_type.?;
        errdefer alloc.free(event_type);
        const actor = self.actor.?;
        errdefer alloc.free(actor);
        const workspace_id = self.workspace_id orelse try alloc.dupe(u8, EMPTY_WORKSPACE);
        errdefer alloc.free(workspace_id);
        const request_json = self.request_json orelse try alloc.dupe(u8, EMPTY_JSON_BODY);
        errdefer alloc.free(request_json);
        const event_id = try alloc.dupe(u8, event_id_raw);
        return .{
            .event_id = event_id,
            .actor = actor,
            .event_type = event_type,
            .workspace_id = workspace_id,
            .request_json = request_json,
            .created_at_ms = self.created_at_ms,
        };
    }
};

/// Walk the `[key, val, key, val, …]` field array, duping recognized values.
/// `errdefer freeOwned` unwinds partial dupes if a later `dupe` OOMs — the
/// original inline literal leaked every earlier dupe on a mid-build failure.
fn parseEventFields(alloc: std.mem.Allocator, fields: []const redis_protocol.RespValue) !ParsedFields {
    var out: ParsedFields = .{};
    errdefer out.freeOwned(alloc);
    var i: usize = 0;
    while (i < fields.len) : (i += 2) {
        const key = redis_protocol.valueAsString(fields[i]) orelse continue;
        const val = redis_protocol.valueAsString(fields[i + 1]) orelse continue;

        // Free-before-assign on every dupe: a duplicated key in a crafted or
        // buggy producer entry would otherwise leak the first copy on every
        // delivery, forever, on the lease hot path.
        if (std.mem.eql(u8, key, queue_consts.fleet_field_type)) {
            if (out.event_type) |old| alloc.free(old);
            out.event_type = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.fleet_field_actor)) {
            if (out.actor) |old| alloc.free(old);
            out.actor = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.fleet_field_workspace_id)) {
            if (out.workspace_id) |old| alloc.free(old);
            out.workspace_id = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.fleet_field_request)) {
            if (out.request_json) |old| alloc.free(old);
            out.request_json = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.fleet_field_created_at)) {
            out.created_at_ms = std.fmt.parseInt(i64, val, 10) catch 0;
        }
    }
    return out;
}

test {
    _ = @import("redis_fleet_decode_test.zig");
}
