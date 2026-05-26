const std = @import("std");

pub const RespValue = union(enum) {
    simple: []u8,
    err: []u8,
    integer: i64,
    bulk: ?[]u8,
    array: ?[]RespValue,

    pub fn deinit(self: *RespValue, alloc: std.mem.Allocator) void {
        switch (self.*) {
            .simple => |v| alloc.free(v),
            .err => |v| alloc.free(v),
            .bulk => |v| if (v) |s| alloc.free(s),
            .array => |v| {
                if (v) |arr| {
                    for (arr) |*item| item.deinit(alloc);
                    alloc.free(arr);
                }
            },
            .integer => {},
        }
    }
};

pub fn valueAsString(v: RespValue) ?[]const u8 {
    return switch (v) {
        .simple => |s| s,
        .bulk => |s| s,
        .err, .integer, .array => null,
    };
}

pub fn ensureSimpleOk(v: RespValue) !void {
    switch (v) {
        .simple => |msg| if (!std.mem.eql(u8, msg, "OK")) return error.RedisAuthFailed,
        else => return error.RedisAuthFailed,
    }
}

pub fn readRespValue(alloc: std.mem.Allocator, reader: *std.Io.Reader) !RespValue {
    const prefix = try reader.takeByte();
    return switch (prefix) {
        '+' => RespValue{ .simple = try readRespLine(alloc, reader) },
        '-' => RespValue{ .err = try readRespLine(alloc, reader) },
        ':' => blk: {
            const line = try readRespLine(alloc, reader);
            defer alloc.free(line);
            break :blk RespValue{ .integer = try std.fmt.parseInt(i64, line, 10) };
        },
        '$' => blk: {
            const line = try readRespLine(alloc, reader);
            defer alloc.free(line);
            const len = try std.fmt.parseInt(i64, line, 10);
            if (len == -1) break :blk RespValue{ .bulk = null };
            if (len < 0) return error.RedisProtocolError;

            const usize_len: usize = @intCast(len);
            const out = try alloc.alloc(u8, usize_len);
            errdefer alloc.free(out);
            try reader.readSliceAll(out);
            const crlf = try reader.takeArray(2);
            if (!std.mem.eql(u8, crlf, "\r\n")) return error.RedisProtocolError;
            break :blk RespValue{ .bulk = out };
        },
        '*' => blk: {
            const line = try readRespLine(alloc, reader);
            defer alloc.free(line);
            const count = try std.fmt.parseInt(i64, line, 10);
            if (count == -1) break :blk RespValue{ .array = null };
            if (count < 0) return error.RedisProtocolError;
            const usize_count: usize = @intCast(count);
            const arr = try alloc.alloc(RespValue, usize_count);
            errdefer alloc.free(arr);
            for (arr, 0..) |*item, idx| {
                item.* = readRespValue(alloc, reader) catch |err| {
                    var j: usize = 0;
                    while (j < idx) : (j += 1) arr[j].deinit(alloc);
                    return err;
                };
            }
            break :blk RespValue{ .array = arr };
        },
        else => error.RedisProtocolError,
    };
}

fn readRespLine(alloc: std.mem.Allocator, reader: *std.Io.Reader) ![]u8 {
    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(alloc);

    while (true) {
        const b = try reader.takeByte();
        if (b == '\r') {
            const next = try reader.takeByte();
            if (next != '\n') return error.RedisProtocolError;
            break;
        }
        try buf.append(alloc, b);
    }

    return buf.toOwnedSlice(alloc);
}

// ── Tests ────────────────────────────────────────────────────────────────

test "readRespValue parses simple string (+OK)" {
    const alloc = std.testing.allocator;
    var reader = std.Io.Reader.fixed("+OK\r\n");
    var v = try readRespValue(alloc, &reader);
    defer v.deinit(alloc);
    try std.testing.expectEqualStrings("OK", v.simple);
}

test "readRespValue parses error string (-ERR msg)" {
    const alloc = std.testing.allocator;
    var reader = std.Io.Reader.fixed("-ERR unknown\r\n");
    var v = try readRespValue(alloc, &reader);
    defer v.deinit(alloc);
    try std.testing.expectEqualStrings("ERR unknown", v.err);
}

test "readRespValue parses integer (:42)" {
    const alloc = std.testing.allocator;
    var reader = std.Io.Reader.fixed(":42\r\n");
    var v = try readRespValue(alloc, &reader);
    defer v.deinit(alloc);
    try std.testing.expectEqual(@as(i64, 42), v.integer);
}

test "readRespValue parses negative integer (:-1)" {
    const alloc = std.testing.allocator;
    var reader = std.Io.Reader.fixed(":-1\r\n");
    var v = try readRespValue(alloc, &reader);
    defer v.deinit(alloc);
    try std.testing.expectEqual(@as(i64, -1), v.integer);
}

test "readRespValue parses bulk string ($5 hello)" {
    const alloc = std.testing.allocator;
    var reader = std.Io.Reader.fixed("$5\r\nhello\r\n");
    var v = try readRespValue(alloc, &reader);
    defer v.deinit(alloc);
    try std.testing.expectEqualStrings("hello", v.bulk.?);
}

test "readRespValue parses null bulk string ($-1)" {
    const alloc = std.testing.allocator;
    var reader = std.Io.Reader.fixed("$-1\r\n");
    var v = try readRespValue(alloc, &reader);
    defer v.deinit(alloc);
    try std.testing.expectEqual(@as(?[]u8, null), v.bulk);
}

test "readRespValue parses empty array (*0)" {
    const alloc = std.testing.allocator;
    var reader = std.Io.Reader.fixed("*0\r\n");
    var v = try readRespValue(alloc, &reader);
    defer v.deinit(alloc);
    try std.testing.expect(v.array != null);
    try std.testing.expectEqual(@as(usize, 0), v.array.?.len);
}

test "readRespValue parses null array (*-1)" {
    const alloc = std.testing.allocator;
    var reader = std.Io.Reader.fixed("*-1\r\n");
    var v = try readRespValue(alloc, &reader);
    defer v.deinit(alloc);
    try std.testing.expectEqual(@as(?[]RespValue, null), v.array);
}

test "readRespValue parses nested array (XREADGROUP-shaped response)" {
    const alloc = std.testing.allocator;
    const data =
        "*1\r\n" ++
        "*2\r\n" ++
        "$9\r\nrun_queue\r\n" ++
        "*1\r\n" ++
        "*2\r\n" ++
        "$3\r\n1-0\r\n" ++
        "*2\r\n" ++
        "$6\r\nrun_id\r\n" ++
        "$7\r\nrun_abc\r\n";
    var reader = std.Io.Reader.fixed(data);
    var v = try readRespValue(alloc, &reader);
    defer v.deinit(alloc);
    try std.testing.expect(v == .array);
    const top = v.array.?;
    try std.testing.expectEqual(@as(usize, 1), top.len);
    const stream_entry = top[0].array.?;
    try std.testing.expectEqual(@as(usize, 2), stream_entry.len);
    try std.testing.expectEqualStrings("run_queue", valueAsString(stream_entry[0]).?);
}

test "valueAsString returns value for simple and bulk, null for others" {
    const alloc = std.testing.allocator;

    var simple = RespValue{ .simple = try alloc.dupe(u8, "OK") };
    defer simple.deinit(alloc);
    try std.testing.expectEqualStrings("OK", valueAsString(simple).?);

    var bulk = RespValue{ .bulk = try alloc.dupe(u8, "data") };
    defer bulk.deinit(alloc);
    try std.testing.expectEqualStrings("data", valueAsString(bulk).?);

    const int_val = RespValue{ .integer = 42 };
    try std.testing.expectEqual(@as(?[]const u8, null), valueAsString(int_val));

    const null_bulk = RespValue{ .bulk = null };
    try std.testing.expectEqual(@as(?[]const u8, null), valueAsString(null_bulk));
}

test "ensureSimpleOk accepts OK and rejects other strings" {
    const alloc = std.testing.allocator;

    var ok = RespValue{ .simple = try alloc.dupe(u8, "OK") };
    defer ok.deinit(alloc);
    try ensureSimpleOk(ok);

    var not_ok = RespValue{ .simple = try alloc.dupe(u8, "QUEUED") };
    defer not_ok.deinit(alloc);
    try std.testing.expectError(error.RedisAuthFailed, ensureSimpleOk(not_ok));

    const int_val = RespValue{ .integer = 1 };
    try std.testing.expectError(error.RedisAuthFailed, ensureSimpleOk(int_val));
}

test "readRespValue rejects unknown prefix byte" {
    const alloc = std.testing.allocator;
    var reader = std.Io.Reader.fixed("?unknown\r\n");
    try std.testing.expectError(error.RedisProtocolError, readRespValue(alloc, &reader));
}
