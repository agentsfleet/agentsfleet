const std = @import("std");
const pg = @import("pg");
const db = @import("../../../db/pool.zig");

pub fn prefixedId(alloc: std.mem.Allocator, prefix: []const u8) []const u8 {
    var id: [16]u8 = undefined;
    std.crypto.random.bytes(&id);
    const hex = std.fmt.bytesToHex(id, .lower);
    return std.fmt.allocPrint(alloc, "{s}_{s}", .{ prefix, hex[0..12] }) catch "id_unknown";
}

pub fn normalizeProfileId(
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    provided: ?[]const u8,
) ![]u8 {
    if (provided) |raw| {
        if (raw.len == 0) return error.InvalidProfileId;
        var out: std.ArrayList(u8) = .{};
        defer out.deinit(alloc);
        for (raw) |c| {
            if (std.ascii.isAlphanumeric(c) or c == '-' or c == '_') {
                try out.append(alloc, std.ascii.toLower(c));
            } else {
                try out.append(alloc, '-');
            }
        }
        return out.toOwnedSlice(alloc);
    }
    return std.fmt.allocPrint(alloc, "{s}-harness", .{workspace_id});
}

pub fn openHarnessHandlerTestConn(alloc: std.mem.Allocator) !?struct { pool: *db.Pool, conn: *pg.Conn } {
    const url = std.process.getEnvVarOwned(alloc, "HANDLER_DB_TEST_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);

    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const opts = try db.parseUrl(arena.allocator(), url);
    const pool = try pg.Pool.init(alloc, opts);
    errdefer pool.deinit();
    const conn = try pool.acquire();
    return .{ .pool = pool, .conn = conn };
}
