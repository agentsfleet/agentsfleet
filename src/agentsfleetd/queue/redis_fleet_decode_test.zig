//! Decoder tests for `fleet:{id}:events` stream replies.
//!
//! Moved out of `redis_fleet.zig` when the decoders were split into
//! `redis_fleet_decode.zig`. Test files are exempt from the file-length cap, so
//! the setup narrative that the source file cannot afford lives here.

const std = @import("std");
const decode = @import("redis_fleet_decode.zig");
const queue_consts = @import("constants.zig");
const redis_protocol = @import("redis_protocol.zig");

const testing = std.testing;

fn dupBulk(alloc: std.mem.Allocator, s: []const u8) !redis_protocol.RespValue {
    return .{ .bulk = try alloc.dupe(u8, s) };
}

/// `[id, [type,…,actor,…(,workspace,…,request,…)]]` stream-entry tuple owned by
/// `alloc`. `full` adds the optional workspace_id + request fields; omitting
/// them drives `intoOwned`'s default-dupe path. The testing allocator never
/// fails mid-build, so the whole tuple is always freed via the caller's deinit.
fn buildTuple(alloc: std.mem.Allocator, full: bool) !redis_protocol.RespValue {
    const n: usize = if (full) 8 else 4;
    const fields = try alloc.alloc(redis_protocol.RespValue, n);
    fields[0] = try dupBulk(alloc, queue_consts.fleet_field_type);
    fields[1] = try dupBulk(alloc, "run.started");
    fields[2] = try dupBulk(alloc, queue_consts.fleet_field_actor);
    fields[3] = try dupBulk(alloc, "steer:x");
    if (full) {
        fields[4] = try dupBulk(alloc, queue_consts.fleet_field_workspace_id);
        fields[5] = try dupBulk(alloc, "ws_1");
        fields[6] = try dupBulk(alloc, queue_consts.fleet_field_request);
        fields[7] = try dupBulk(alloc, "{\"k\":1}");
    }
    const tuple = try alloc.alloc(redis_protocol.RespValue, 2);
    tuple[0] = try dupBulk(alloc, "1700000000000-0");
    tuple[1] = .{ .array = fields };
    return .{ .array = tuple };
}

fn decodeForLeakCheck(alloc: std.mem.Allocator, item: *const redis_protocol.RespValue) !void {
    var ev = try decode.decodeFleetEventTuple(alloc, item.*);
    ev.deinit(alloc);
}

test "decodeFleetEventTuple unwinds every owned slice on OOM at any step" {
    var item = try buildTuple(testing.allocator, false);
    defer item.deinit(testing.allocator);
    // Fails each internal dupe in turn and asserts the only error is OutOfMemory
    // with zero leaked bytes — proving the parseEventFields + intoOwned errdefer chain.
    try testing.checkAllAllocationFailures(testing.allocator, decodeForLeakCheck, .{&item});
}

test "decodeFleetEventTuple round-trips all present fields" {
    var item = try buildTuple(testing.allocator, true);
    defer item.deinit(testing.allocator);
    var ev = try decode.decodeFleetEventTuple(testing.allocator, item);
    defer ev.deinit(testing.allocator);
    try testing.expectEqualStrings("1700000000000-0", ev.event_id);
    try testing.expectEqualStrings("run.started", ev.event_type);
    try testing.expectEqualStrings("steer:x", ev.actor);
    try testing.expectEqualStrings("ws_1", ev.workspace_id);
    try testing.expectEqualStrings("{\"k\":1}", ev.request_json);
}

test "decodeFleetEventTuple defaults workspace and request when absent" {
    var item = try buildTuple(testing.allocator, false);
    defer item.deinit(testing.allocator);
    var ev = try decode.decodeFleetEventTuple(testing.allocator, item);
    defer ev.deinit(testing.allocator);
    try testing.expectEqualStrings(decode.EMPTY_WORKSPACE, ev.workspace_id);
    try testing.expectEqualStrings(decode.EMPTY_JSON_BODY, ev.request_json);
}
