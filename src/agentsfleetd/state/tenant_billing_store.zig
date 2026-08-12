const std = @import("std");
const sql = @import("sql.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const pool_elevation = @import("../db/pool_elevation.zig");

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

/// Returns true when a row was inserted; false means the tenant already had a
/// wallet and the ON CONFLICT DO NOTHING left it — and its balance — untouched.
pub fn insertIfAbsent(
    conn: *pg.Conn,
    tenant_id: []const u8,
    balance_nanos: i64,
    grant_source: []const u8,
) !bool {
    const now_ms = clock.nowMillis();
    // The wallet belongs to `billing_runtime` (schema/700). Inside the signup
    // bootstrap's transaction the scope brackets just the starter-grant
    // INSERT, so the tenant-create statements around it keep running as
    // `api_runtime`.
    //
    // The affected-row count is read INSIDE the scope and returned after the
    // commit. `healStarterGrant` reaches the wallet only through here, so it
    // inherits the elevation rather than needing its own — an unelevated
    // INSERT on this table is refused by PostgreSQL, and the replay path that
    // calls it would fail at runtime rather than at compile time.
    var scope = try pool_elevation.begin(conn, .billing);
    defer scope.deinit();
    const affected = try scope.exec(sql.INSERT_TENANT_BILLING, .{ tenant_id, balance_nanos, grant_source, now_ms });
    try scope.commit();
    return (affected orelse 0) > 0;
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
    // Both statements here (the debit and the 0-row probe) are wallet reads
    // and writes, so one `billing_runtime` scope covers the pair; each result
    // drains (defer) before the next statement or the commit.
    var scope = try pool_elevation.begin(conn, .billing);
    defer scope.deinit();

    // A successful debit clears `balance_exhausted_at` — the only path there
    // is a prior top-up moving balance_nanos above zero. Keeping this in the
    // same UPDATE keeps the transition atomic so the `stop` gate can never see
    // "positive balance AND exhausted_at set".
    const debited: ?DebitResult = blk: {
        var q = PgQuery.from(try scope.query(sql.DEBIT_TENANT_BALANCE, .{ tenant_id, nanos, now_ms }));
        defer q.deinit();
        const row = (try q.next()) orelse break :blk null;
        break :blk .{ .balance_nanos = try row.get(i64, 0), .updated_at_ms = try row.get(i64, 1) };
    };
    if (debited) |d| {
        try scope.commit();
        return d;
    }
    if (!try rowExists(scope.handle(), tenant_id)) return error.TenantBillingMissing;
    return error.CreditExhausted;
}

/// Wallet read; the `Elevated(.billing)` parameter is the compile-time proof
/// it runs only inside an already-elevated callback (RULE OWN).
fn rowExists(v: pool_elevation.Elevated(.billing), tenant_id: []const u8) !bool {
    var q = PgQuery.from(try v.query(sql.SELECT_TENANT_BILLING_EXISTS, .{tenant_id}));
    defer q.deinit();
    return (try q.next()) != null;
}

pub fn loadByTenant(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    tenant_id: []const u8,
) !?BillingRow {
    var scope = try pool_elevation.begin(conn, .billing);
    defer scope.deinit();

    // Drains at this block's exit, before the commit. The owned string is an
    // ordinary local, so the errdefer below covers a commit that fails after
    // the read succeeded.
    const loaded: ?BillingRow = blk: {
        var q = PgQuery.from(try scope.query(sql.SELECT_TENANT_BALANCE, .{tenant_id}));
        defer q.deinit();
        const row = (try q.next()) orelse break :blk null;
        const bal = try row.get(i64, 0);
        const grant_source = try alloc.dupe(u8, try row.get([]const u8, 1));
        errdefer alloc.free(grant_source);
        break :blk .{
            .balance_nanos = bal,
            .grant_source = grant_source,
            .updated_at_ms = try row.get(i64, 2),
            .exhausted_at_ms = try row.get(?i64, 3),
        };
    };
    errdefer if (loaded) |*r| @constCast(r).deinit(alloc);

    try scope.commit();
    return loaded;
}

/// Atomic first-debit-exhaustion mark. Sets balance_exhausted_at=now_ms only
/// if currently NULL. Returns true if the transition happened (first call),
/// false if the row was already marked (idempotent replay).
pub fn markExhausted(conn: *pg.Conn, tenant_id: []const u8) !bool {
    return transitionExhaustion(conn, tenant_id, sql.MARK_BALANCE_EXHAUSTED);
}

/// Atomic exhaustion clear. Sets `balance_exhausted_at = NULL`
/// unconditionally; returns true when a row was present and had been
/// previously marked. Complements `debit` (which auto-clears on
/// successful deduction) — intended for paths that top up without
/// going through `debit`, e.g. an admin manual credit. Required so the
/// `stop` gate is not a one-way door (greptile #3121312916 follow-up).
pub fn clearExhausted(conn: *pg.Conn, tenant_id: []const u8) !bool {
    return transitionExhaustion(conn, tenant_id, sql.CLEAR_BALANCE_EXHAUSTED);
}

/// The shared body of the two exhaustion transitions: one elevated statement
/// bound to `(tenant_id, now_ms)`, true when it returned a row. Extracted
/// because the mark and the clear differed only in which statement they ran.
fn transitionExhaustion(conn: *pg.Conn, tenant_id: []const u8, comptime statement: []const u8) !bool {
    const now_ms = clock.nowMillis();
    var scope = try pool_elevation.begin(conn, .billing);
    defer scope.deinit();

    const transitioned = blk: {
        var q = PgQuery.from(try scope.query(statement, .{ tenant_id, now_ms }));
        defer q.deinit();
        break :blk (try q.next()) != null;
    };
    try scope.commit();
    return transitioned;
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
