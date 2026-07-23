const std = @import("std");
const sql = @import("sql.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
const BillingRow = struct {
    const Self = @This();

    balance_nanos: i64,
    grant_source: []u8,
    updated_at_ms: i64,
    exhausted_at_ms: ?i64,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.grant_source);
    }
};

pub fn insertIfAbsent(
    conn: *pg.Conn,
    tenant_id: []const u8,
    balance_nanos: i64,
    grant_source: []const u8,
) !void {
    const now_ms = clock.nowMillis();
    _ = try conn.exec(
        sql.INSERT_TENANT_BILLING, .{ tenant_id, balance_nanos, grant_source, now_ms });
}

pub const DebitResult = struct { balance_nanos: i64, updated_at_ms: i64 };

/// Atomic conditional debit. Returns the post-debit balance, or a typed
/// error distinguishing "tenant has no billing row" from "row exists but
/// would go negative":
///
///   error.TenantBillingMissing — provision was never called for this tenant.
///                                Always a bootstrap invariant bug.
///   error.CreditExhausted      — row present but balance < nanos. Expected
///                                operational outcome on a free-plan tenant.
///
/// The primary UPDATE is still a single atomic statement; the EXISTS probe
/// only fires on the 0-row path, so the happy path stays one round-trip.
pub fn debit(conn: *pg.Conn, tenant_id: []const u8, nanos: i64) !DebitResult {
    if (nanos < 0) return error.InvalidDebit;
    const now_ms = clock.nowMillis();
    // A successful debit clears `balance_exhausted_at` — the only path
    // there is a prior top-up moving balance_nanos above zero. Keeping
    // this in the same UPDATE keeps the transition atomic so the `stop`
    // gate can never see "positive balance AND exhausted_at set".
    var q = PgQuery.from(try conn.query(
        sql.DEBIT_TENANT_BALANCE, .{ tenant_id, nanos, now_ms }));
    defer q.deinit();
    const row = (try q.next()) orelse {
        if (!try rowExists(conn, tenant_id)) return error.TenantBillingMissing;
        return error.CreditExhausted;
    };
    const bal = try row.get(i64, 0);
    const ts = try row.get(i64, 1);
    return .{ .balance_nanos = bal, .updated_at_ms = ts };
}

fn rowExists(conn: *pg.Conn, tenant_id: []const u8) !bool {
    var q = PgQuery.from(try conn.query(
        sql.SELECT_TENANT_BILLING_EXISTS, .{tenant_id}));
    defer q.deinit();
    return (try q.next()) != null;
}

pub fn loadByTenant(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    tenant_id: []const u8,
) !?BillingRow {
    var q = PgQuery.from(try conn.query(
        sql.SELECT_TENANT_BALANCE, .{tenant_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    const bal = try row.get(i64, 0);
    const grant_source = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(grant_source);
    const ts = try row.get(i64, 2);
    const exhausted_at_ms = try row.get(?i64, 3);
    return .{
        .balance_nanos = bal,
        .grant_source = grant_source,
        .updated_at_ms = ts,
        .exhausted_at_ms = exhausted_at_ms,
    };
}

/// Atomic first-debit-exhaustion mark. Sets balance_exhausted_at=now_ms only
/// if currently NULL. Returns true if the transition happened (first call),
/// false if the row was already marked (idempotent replay).
pub fn markExhausted(conn: *pg.Conn, tenant_id: []const u8) !bool {
    const now_ms = clock.nowMillis();
    var q = PgQuery.from(try conn.query(
        sql.MARK_BALANCE_EXHAUSTED, .{ tenant_id, now_ms }));
    defer q.deinit();
    return (try q.next()) != null;
}

/// Atomic exhaustion clear. Sets `balance_exhausted_at = NULL`
/// unconditionally; returns true when a row was present and had been
/// previously marked. Complements `debit` (which auto-clears on
/// successful deduction) — intended for paths that top up without
/// going through `debit`, e.g. an admin manual credit. Required so the
/// `stop` gate is not a one-way door (greptile #3121312916 follow-up).
pub fn clearExhausted(conn: *pg.Conn, tenant_id: []const u8) !bool {
    const now_ms = clock.nowMillis();
    var q = PgQuery.from(try conn.query(
        sql.CLEAR_BALANCE_EXHAUSTED, .{ tenant_id, now_ms }));
    defer q.deinit();
    return (try q.next()) != null;
}

pub fn resolveTenantFromWorkspace(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) ![]u8 {
    var q = PgQuery.from(try conn.query(
        sql.SELECT_TENANT_FOR_WORKSPACE, .{workspace_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.WorkspaceNotFound;
    return alloc.dupe(u8, try row.get([]const u8, 0));
}
