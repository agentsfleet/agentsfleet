const std = @import("std");
const pg = @import("pg");
const db = @import("../../../db/pool.zig");
const id_format = @import("../../../types/id_format.zig");

pub fn prefixedId(alloc: std.mem.Allocator, prefix: []const u8) ![]const u8 {
    if (std.mem.eql(u8, prefix, "pver")) return id_format.generateProfileVersionId(alloc);
    if (std.mem.eql(u8, prefix, "cjob")) return id_format.generateCompileJobId(alloc);
    return error.UnsupportedPrefix;
}

pub fn isSupportedProfileVersionId(id: []const u8) bool {
    return id_format.isSupportedProfileVersionId(id);
}

pub fn isSupportedCompileJobId(id: []const u8) bool {
    return id_format.isSupportedCompileJobId(id);
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
