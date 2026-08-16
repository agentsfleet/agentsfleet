const std = @import("std");
const sql = @import("sql.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

/// Every field is a copied scalar, so the row owns no memory and needs no
/// allocator to read or release. It carried `grant_source` — the only allocated
/// field, and the reason this struct once had a `deinit` — until M164 found that
/// no reader consumed it: the wallet read duplicated the string on every billing
/// request and every metered stage, and both callers freed it unexamined. The
/// column is still WRITTEN at provisioning, where it is the audit record of why
/// a tenant holds a balance; it is simply no longer read back.
const BillingRow = struct {
    balance_nanos: i64,
    updated_at_ms: i64,
    exhausted_at_ms: ?i64,
};

/// Returns true when a row was inserted; false means the tenant already had a
/// wallet and the ON CONFLICT DO NOTHING left it — and its balance — untouched.
pub fn insertIfAbsent(
    conn: *pg.Conn,
    tenant_id: []const u8,
    balance_nanos: i64,
    grant_source: []const u8,
) !bool {
    const now_ms = clock.nowMillis();
    const affected = try conn.exec(sql.INSERT_TENANT_BILLING, .{ tenant_id, balance_nanos, grant_source, now_ms });
    return (affected orelse 0) > 0;
}

pub fn loadByTenant(conn: *pg.Conn, tenant_id: []const u8) !?BillingRow {
    var q = PgQuery.from(try conn.query(sql.SELECT_TENANT_BALANCE, .{tenant_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    return .{
        .balance_nanos = try row.get(i64, 0),
        .updated_at_ms = try row.get(i64, 1),
        .exhausted_at_ms = try row.get(?i64, 2),
    };
}

pub fn resolveTenantFromWorkspace(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) ![]u8 {
    var q = PgQuery.from(try conn.query(sql.SELECT_TENANT_FOR_WORKSPACE, .{workspace_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.WorkspaceNotFound;
    return alloc.dupe(u8, try row.get([]const u8, 0));
}
