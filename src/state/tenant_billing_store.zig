const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

pub const BillingRow = struct {
    plan_tier: []u8,
    plan_sku: []u8,
    balance_cents: i64,
    grant_source: []u8,
    updated_at_ms: i64,

    pub fn deinit(self: *BillingRow, alloc: std.mem.Allocator) void {
        alloc.free(self.plan_tier);
        alloc.free(self.plan_sku);
        alloc.free(self.grant_source);
    }
};

pub fn insertIfAbsent(
    conn: *pg.Conn,
    tenant_id: []const u8,
    plan_tier: []const u8,
    plan_sku: []const u8,
    balance_cents: i64,
    grant_source: []const u8,
) !void {
    const now_ms = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing
        \\  (tenant_id, plan_tier, plan_sku, balance_cents, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $6)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ tenant_id, plan_tier, plan_sku, balance_cents, grant_source, now_ms });
}

pub const DebitResult = struct { balance_cents: i64, updated_at_ms: i64 };

/// Atomic conditional debit. Returns the post-debit balance, or
/// error.CreditExhausted if the WHERE guard fails (row either missing or
/// would go negative).
pub fn debit(conn: *pg.Conn, tenant_id: []const u8, cents: i64) !DebitResult {
    if (cents < 0) return error.InvalidDebit;
    const now_ms = std.time.milliTimestamp();
    var q = PgQuery.from(try conn.query(
        \\UPDATE billing.tenant_billing
        \\SET balance_cents = balance_cents - $2,
        \\    updated_at = $3
        \\WHERE tenant_id = $1::uuid
        \\  AND balance_cents >= $2
        \\RETURNING balance_cents, updated_at
    , .{ tenant_id, cents, now_ms }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.CreditExhausted;
    const bal = try row.get(i64, 0);
    const ts = try row.get(i64, 1);
    return .{ .balance_cents = bal, .updated_at_ms = ts };
}

pub fn loadByTenant(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    tenant_id: []const u8,
) !?BillingRow {
    var q = PgQuery.from(try conn.query(
        \\SELECT plan_tier, plan_sku, balance_cents, grant_source, updated_at
        \\FROM billing.tenant_billing
        \\WHERE tenant_id = $1::uuid
        \\LIMIT 1
    , .{tenant_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    const plan_tier = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(plan_tier);
    const plan_sku = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(plan_sku);
    const bal = try row.get(i64, 2);
    const grant_source = try alloc.dupe(u8, try row.get([]const u8, 3));
    errdefer alloc.free(grant_source);
    const ts = try row.get(i64, 4);
    return .{
        .plan_tier = plan_tier,
        .plan_sku = plan_sku,
        .balance_cents = bal,
        .grant_source = grant_source,
        .updated_at_ms = ts,
    };
}

pub fn resolveTenantFromWorkspace(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) ![]u8 {
    var q = PgQuery.from(try conn.query(
        \\SELECT tenant_id::text
        \\FROM core.workspaces
        \\WHERE workspace_id = $1::uuid
        \\LIMIT 1
    , .{workspace_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.WorkspaceNotFound;
    return alloc.dupe(u8, try row.get([]const u8, 0));
}
