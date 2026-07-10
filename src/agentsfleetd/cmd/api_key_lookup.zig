//! DB-backed `LookupFn` for the `tenant_api_key` middleware.
//!
//! `src/auth/middleware/` is portability-locked — it cannot reach into
//! `src/db/`. This module lives in `src/cmd/` (alongside the serve host
//! that wires it) and provides the concrete SHA-256-hex → `core.api_keys`
//! lookup, duplicating the kept fields into the caller's allocator.

const std = @import("std");
const pg = @import("pg");

const clock = @import("common").clock;
const db = @import("../db/pool.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const tenant_api_key = @import("../auth/middleware/tenant_api_key.zig");

const LOOKUP_SQL =
    \\SELECT uid::text, tenant_id::text, created_by::text, active
    \\FROM core.api_keys
    \\WHERE key_hash = $1
    \\LIMIT 1
;

const STAMP_LAST_USED_SQL =
    \\UPDATE core.api_keys
    \\SET last_used_at = $2
    \\WHERE uid IN (
    \\    SELECT uid
    \\    FROM core.api_keys
    \\    WHERE key_hash = $1 AND active = TRUE
    \\    FOR UPDATE SKIP LOCKED
    \\)
;

pub const LookupResult = tenant_api_key.LookupResult;

/// Host context carrying the shared connection pool. A stable pointer to a
/// value of this type is passed as `host` in the `LookupFn` call.
pub const Ctx = struct {
    pool: *pg.Pool,
};

/// Resolve a SHA-256 hex digest to a `core.api_keys` row. Returns null when
/// no row matches. Allocates `api_key_id`, `tenant_id`, and `user_id` via
/// `alloc`; caller is responsible for freeing them (the middleware frees
/// `api_key_id` unconditionally and the rest via the principal lifecycle).
pub fn lookup(
    host: *anyopaque,
    alloc: std.mem.Allocator,
    key_hash_hex: []const u8,
) anyerror!?LookupResult {
    const self: *Ctx = @ptrCast(@alignCast(host));
    const conn = self.pool.acquire() catch return error.DbUnavailable;
    defer self.pool.release(conn);

    const result = blk: {
        var q = PgQuery.from(conn.query(LOOKUP_SQL, .{key_hash_hex}) catch return error.DbQueryFailed);
        defer q.deinit();

        const row = (q.next() catch return error.DbQueryFailed) orelse return null;
        break :blk try copyRow(alloc, row);
    };

    if (result.active) stampLastUsed(conn, key_hash_hex, clock.nowMillis());
    return result;
}

fn stampLastUsed(conn: *pg.Conn, key_hash_hex: []const u8, now_ms: i64) void {
    _ = conn.exec(STAMP_LAST_USED_SQL, .{ key_hash_hex, now_ms }) catch {
        // Authentication already succeeded; usage metadata must never decide it.
        return;
    };
}

fn copyRow(alloc: std.mem.Allocator, row: pg.Row) !LookupResult {
    const api_key_id_raw = row.get([]u8, 0) catch return error.DbRowShape;
    const tenant_id_raw = row.get([]u8, 1) catch return error.DbRowShape;
    const created_by_raw = row.get([]u8, 2) catch return error.DbRowShape;
    const active = row.get(bool, 3) catch return error.DbRowShape;

    const api_key_id = try alloc.dupe(u8, api_key_id_raw);
    errdefer alloc.free(api_key_id);
    const tenant_id = try alloc.dupe(u8, tenant_id_raw);
    errdefer alloc.free(tenant_id);
    const user_id = try alloc.dupe(u8, created_by_raw);

    return .{
        .api_key_id = api_key_id,
        .tenant_id = tenant_id,
        .user_id = user_id,
        .active = active,
    };
}

// Referenced to silence "unused" warnings when the host isn't wired yet.
test {
    _ = db;
}

test "tenant key lookup keeps usage stamp out of the authentication query" {
    try std.testing.expect(std.mem.indexOf(u8, LOOKUP_SQL, "UPDATE core.api_keys") == null);
    try std.testing.expect(std.mem.indexOf(u8, STAMP_LAST_USED_SQL, "FOR UPDATE SKIP LOCKED") != null);
}
