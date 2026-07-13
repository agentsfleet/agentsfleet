const std = @import("std");
const pg = @import("pg");

const PgQuery = @import("../db/pg_query.zig").PgQuery;

/// Return the subset of `names` for which the workspace holds no vault secret —
/// the install-gate secret check (used by fleet create when installing a
/// template that declares required secrets).
pub fn missingSecretNames(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    names: []const []const u8,
) ![]const []const u8 {
    var missing: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (missing.items) |name| alloc.free(name);
        missing.deinit(alloc);
    }
    for (names) |name| {
        if (!try secretExists(conn, workspace_id, name)) {
            try missing.append(alloc, try alloc.dupe(u8, name));
        }
    }
    return missing.toOwnedSlice(alloc);
}

pub fn freeStringSlice(alloc: std.mem.Allocator, values: []const []const u8) void {
    for (values) |value| alloc.free(value);
    alloc.free(values);
}

fn secretExists(conn: *pg.Conn, workspace_id: []const u8, name: []const u8) !bool {
    var q = PgQuery.from(try conn.query(
        \\SELECT 1 FROM vault.secrets
        \\WHERE workspace_id = $1::uuid AND key_name = $2
        \\LIMIT 1
    , .{ workspace_id, name }));
    defer q.deinit();
    return (try q.next()) != null;
}
