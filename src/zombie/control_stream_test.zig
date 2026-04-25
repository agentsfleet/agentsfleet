const std = @import("std");
const control_stream = @import("control_stream.zig");
const redis_protocol = @import("../queue/redis_protocol.zig");

const Decoded = control_stream.Decoded;
const ControlMessage = control_stream.ControlMessage;
const MessageType = control_stream.MessageType;
const ZombieStatus = control_stream.ZombieStatus;
const RespValue = redis_protocol.RespValue;

/// Build a heap-owned bulk RespValue so `RespValue.deinit` can free it.
fn bulk(alloc: std.mem.Allocator, s: []const u8) !RespValue {
    return .{ .bulk = try alloc.dupe(u8, s) };
}

fn freeFields(alloc: std.mem.Allocator, fields: []RespValue) void {
    for (fields) |*f| f.deinit(alloc);
    alloc.free(fields);
}

test "MessageType: round-trip toSlice/fromSlice covers all variants" {
    const cases = [_]MessageType{ .zombie_created, .zombie_status_changed, .zombie_config_changed, .worker_drain_request };
    for (cases) |t| {
        const s = t.toSlice();
        const back = MessageType.fromSlice(s) orelse return error.RoundTripFailed;
        try std.testing.expectEqual(t, back);
    }
    try std.testing.expectEqual(@as(?MessageType, null), MessageType.fromSlice("garbage"));
}

test "ZombieStatus: round-trip toSlice/fromSlice covers all variants" {
    const cases = [_]ZombieStatus{ .active, .killed, .paused };
    for (cases) |s| {
        const back = ZombieStatus.fromSlice(s.toSlice()) orelse return error.RoundTripFailed;
        try std.testing.expectEqual(s, back);
    }
    try std.testing.expectEqual(@as(?ZombieStatus, null), ZombieStatus.fromSlice("nonsense"));
}

test "decodeEntry: zombie_created carries zombie_id + workspace_id" {
    const alloc = std.testing.allocator;
    var fields = try alloc.alloc(RespValue, 6);
    fields[0] = try bulk(alloc, "type");
    fields[1] = try bulk(alloc, "zombie_created");
    fields[2] = try bulk(alloc, "zombie_id");
    fields[3] = try bulk(alloc, "z-abc");
    fields[4] = try bulk(alloc, "workspace_id");
    fields[5] = try bulk(alloc, "ws-1");
    defer freeFields(alloc, fields);

    var decoded = try control_stream.decodeEntry(alloc, "1700000000-0", fields);
    defer decoded.deinit(alloc);

    try std.testing.expectEqualStrings("1700000000-0", decoded.message_id);
    switch (decoded.message) {
        .zombie_created => |m| {
            try std.testing.expectEqualStrings("z-abc", m.zombie_id);
            try std.testing.expectEqualStrings("ws-1", m.workspace_id);
        },
        else => return error.WrongVariant,
    }
}

test "decodeEntry: zombie_status_changed carries zombie_id + status" {
    const alloc = std.testing.allocator;
    var fields = try alloc.alloc(RespValue, 6);
    fields[0] = try bulk(alloc, "type");
    fields[1] = try bulk(alloc, "zombie_status_changed");
    fields[2] = try bulk(alloc, "zombie_id");
    fields[3] = try bulk(alloc, "z-xyz");
    fields[4] = try bulk(alloc, "status");
    fields[5] = try bulk(alloc, "killed");
    defer freeFields(alloc, fields);

    var decoded = try control_stream.decodeEntry(alloc, "1700000000-1", fields);
    defer decoded.deinit(alloc);

    switch (decoded.message) {
        .zombie_status_changed => |m| {
            try std.testing.expectEqualStrings("z-xyz", m.zombie_id);
            try std.testing.expectEqual(ZombieStatus.killed, m.status);
        },
        else => return error.WrongVariant,
    }
}

test "decodeEntry: zombie_config_changed parses revision" {
    const alloc = std.testing.allocator;
    var fields = try alloc.alloc(RespValue, 6);
    fields[0] = try bulk(alloc, "type");
    fields[1] = try bulk(alloc, "zombie_config_changed");
    fields[2] = try bulk(alloc, "zombie_id");
    fields[3] = try bulk(alloc, "z-cfg");
    fields[4] = try bulk(alloc, "config_revision");
    fields[5] = try bulk(alloc, "42");
    defer freeFields(alloc, fields);

    var decoded = try control_stream.decodeEntry(alloc, "1700000000-2", fields);
    defer decoded.deinit(alloc);

    switch (decoded.message) {
        .zombie_config_changed => |m| {
            try std.testing.expectEqualStrings("z-cfg", m.zombie_id);
            try std.testing.expectEqual(@as(u32, 42), m.config_revision);
        },
        else => return error.WrongVariant,
    }
}

test "decodeEntry: worker_drain_request with optional reason" {
    const alloc = std.testing.allocator;
    var fields = try alloc.alloc(RespValue, 4);
    fields[0] = try bulk(alloc, "type");
    fields[1] = try bulk(alloc, "worker_drain_request");
    fields[2] = try bulk(alloc, "reason");
    fields[3] = try bulk(alloc, "shutdown");
    defer freeFields(alloc, fields);

    var decoded = try control_stream.decodeEntry(alloc, "1700000000-3", fields);
    defer decoded.deinit(alloc);

    switch (decoded.message) {
        .worker_drain_request => |m| {
            try std.testing.expectEqualStrings("shutdown", m.reason orelse return error.MissingReason);
        },
        else => return error.WrongVariant,
    }
}

test "decodeEntry: worker_drain_request without reason" {
    const alloc = std.testing.allocator;
    var fields = try alloc.alloc(RespValue, 2);
    fields[0] = try bulk(alloc, "type");
    fields[1] = try bulk(alloc, "worker_drain_request");
    defer freeFields(alloc, fields);

    var decoded = try control_stream.decodeEntry(alloc, "1700000000-4", fields);
    defer decoded.deinit(alloc);

    switch (decoded.message) {
        .worker_drain_request => |m| try std.testing.expectEqual(@as(?[]const u8, null), m.reason),
        else => return error.WrongVariant,
    }
}

test "decodeEntry: malformed odd field count returns error" {
    const alloc = std.testing.allocator;
    var fields = try alloc.alloc(RespValue, 3);
    fields[0] = try bulk(alloc, "type");
    fields[1] = try bulk(alloc, "zombie_created");
    fields[2] = try bulk(alloc, "zombie_id");
    defer freeFields(alloc, fields);

    try std.testing.expectError(error.ControlDecodeMalformed, control_stream.decodeEntry(alloc, "id", fields));
}

test "decodeEntry: unknown type returns ControlDecodeUnknownType" {
    const alloc = std.testing.allocator;
    var fields = try alloc.alloc(RespValue, 2);
    fields[0] = try bulk(alloc, "type");
    fields[1] = try bulk(alloc, "fictitious_event");
    defer freeFields(alloc, fields);

    try std.testing.expectError(error.ControlDecodeUnknownType, control_stream.decodeEntry(alloc, "id", fields));
}

test "decodeEntry: zombie_created missing workspace_id returns ControlDecodeMissingField" {
    const alloc = std.testing.allocator;
    var fields = try alloc.alloc(RespValue, 4);
    fields[0] = try bulk(alloc, "type");
    fields[1] = try bulk(alloc, "zombie_created");
    fields[2] = try bulk(alloc, "zombie_id");
    fields[3] = try bulk(alloc, "z-abc");
    defer freeFields(alloc, fields);

    try std.testing.expectError(error.ControlDecodeMissingField, control_stream.decodeEntry(alloc, "id", fields));
}
