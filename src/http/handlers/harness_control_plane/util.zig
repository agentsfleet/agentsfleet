const std = @import("std");
const pg = @import("pg");
const db = @import("../../../db/pool.zig");
const id_format = @import("../../../types/id_format.zig");

pub fn generateProfileVersionId(alloc: std.mem.Allocator) ![]const u8 {
    return id_format.generateProfileVersionId(alloc);
}

pub fn generateCompileJobId(alloc: std.mem.Allocator) ![]const u8 {
    return id_format.generateCompileJobId(alloc);
}

pub fn isSupportedProfileVersionId(id: []const u8) bool {
    return id_format.isSupportedProfileVersionId(id);
}

pub fn isSupportedProfileId(id: []const u8) bool {
    return id_format.isSupportedProfileId(id);
}

pub fn isSupportedCompileJobId(id: []const u8) bool {
    return id_format.isSupportedCompileJobId(id);
}

pub fn normalizeProfileId(
    alloc: std.mem.Allocator,
    provided: ?[]const u8,
) ![]u8 {
    if (provided) |raw| {
        if (!id_format.isSupportedProfileId(raw)) return error.InvalidProfileId;
        return alloc.dupe(u8, raw);
    }
    const generated = try id_format.generateProfileId(alloc);
    return @constCast(generated);
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
