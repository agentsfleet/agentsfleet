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

pub fn readRespLine(alloc: std.mem.Allocator, reader: *std.Io.Reader) ![]u8 {
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
