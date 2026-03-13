const std = @import("std");
const pg = @import("pg");
const error_codes = @import("../errors/codes.zig");
const id_format = @import("../types/id_format.zig");
const billing = @import("./workspace_billing.zig");

pub const FREE_PLAN_INITIAL_CREDIT_CENTS: i64 = 1000;
pub const CREDIT_CURRENCY = "USD";

pub const CreditView = struct {
    currency: []const u8,
    initial_credit_cents: i64,
    consumed_credit_cents: i64,
    remaining_credit_cents: i64,
    exhausted_at: ?i64,
};

const CreditRow = struct {
    currency: []u8,
    initial_credit_cents: i64,
    consumed_credit_cents: i64,
    remaining_credit_cents: i64,
    exhausted_at: ?i64,

    fn deinit(self: *CreditRow, alloc: std.mem.Allocator) void {
        alloc.free(self.currency);
    }
};

pub fn errorCode(err: anyerror) ?[]const u8 {
    return switch (err) {
        error.CreditExhausted => error_codes.ERR_CREDIT_EXHAUSTED,
        else => null,
    };
}

pub fn errorMessage(err: anyerror) ?[]const u8 {
    return switch (err) {
        error.CreditExhausted => "Free plan credit exhausted. Upgrade to Scale to continue.",
        else => null,
    };
}

pub fn provisionWorkspaceCredit(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    actor: []const u8,
) !void {
    const now_ms = std.time.milliTimestamp();
    try upsertCreditState(conn, alloc, workspace_id, .{
        .currency = CREDIT_CURRENCY,
        .initial_credit_cents = FREE_PLAN_INITIAL_CREDIT_CENTS,
        .consumed_credit_cents = 0,
        .remaining_credit_cents = FREE_PLAN_INITIAL_CREDIT_CENTS,
        .exhausted_at = null,
    }, now_ms);
    try insertAudit(conn, alloc, workspace_id, "CREDIT_GRANTED", FREE_PLAN_INITIAL_CREDIT_CENTS, FREE_PLAN_INITIAL_CREDIT_CENTS, "workspace_created", actor, "{}");
}

pub fn getOrProvisionWorkspaceCredit(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !CreditView {
    const existing = loadCreditRow(conn, alloc, workspace_id) catch |err| switch (err) {
        error.WorkspaceCreditStateMissing => null,
        else => return err,
    };
    if (existing) |row| {
        defer {
            var copy = row;
            copy.deinit(alloc);
        }
        return .{
            .currency = try alloc.dupe(u8, row.currency),
            .initial_credit_cents = row.initial_credit_cents,
            .consumed_credit_cents = row.consumed_credit_cents,
            .remaining_credit_cents = row.remaining_credit_cents,
            .exhausted_at = row.exhausted_at,
        };
    }

    try provisionWorkspaceCredit(conn, alloc, workspace_id, "system");
    return getOrProvisionWorkspaceCredit(conn, alloc, workspace_id);
}

pub fn enforceExecutionAllowed(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    plan_tier: billing.PlanTier,
) !CreditView {
    const credit = try getOrProvisionWorkspaceCredit(conn, alloc, workspace_id);
    if (plan_tier == .free and credit.remaining_credit_cents <= 0) {
        alloc.free(credit.currency);
        return error.CreditExhausted;
    }
    return credit;
}

fn loadCreditRow(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !?CreditRow {
    var q = try conn.query(
        \\SELECT currency, initial_credit_cents, consumed_credit_cents, remaining_credit_cents, exhausted_at
        \\FROM workspace_credit_state
        \\WHERE workspace_id = $1
        \\LIMIT 1
    , .{workspace_id});
    defer q.deinit();
    const row = (try q.next()) orelse return error.WorkspaceCreditStateMissing;
    return .{
        .currency = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .initial_credit_cents = try row.get(i64, 1),
        .consumed_credit_cents = try row.get(i64, 2),
        .remaining_credit_cents = try row.get(i64, 3),
        .exhausted_at = try row.get(?i64, 4),
    };
}

fn upsertCreditState(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    state: struct {
        currency: []const u8,
        initial_credit_cents: i64,
        consumed_credit_cents: i64,
        remaining_credit_cents: i64,
        exhausted_at: ?i64,
    },
    now_ms: i64,
) !void {
    const credit_id = try id_format.generateEntitlementSnapshotId(alloc);
    defer alloc.free(credit_id);
    var q = try conn.query(
        \\INSERT INTO workspace_credit_state
        \\  (credit_id, workspace_id, currency, initial_credit_cents, consumed_credit_cents, remaining_credit_cents, exhausted_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8)
        \\ON CONFLICT (workspace_id) DO UPDATE
        \\SET currency = EXCLUDED.currency,
        \\    initial_credit_cents = EXCLUDED.initial_credit_cents,
        \\    consumed_credit_cents = EXCLUDED.consumed_credit_cents,
        \\    remaining_credit_cents = EXCLUDED.remaining_credit_cents,
        \\    exhausted_at = EXCLUDED.exhausted_at,
        \\    updated_at = EXCLUDED.updated_at
    , .{
        credit_id,
        workspace_id,
        state.currency,
        state.initial_credit_cents,
        state.consumed_credit_cents,
        state.remaining_credit_cents,
        state.exhausted_at,
        now_ms,
    });
    q.deinit();
}

fn insertAudit(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    event_type: []const u8,
    delta_credit_cents: i64,
    remaining_credit_cents: i64,
    reason: []const u8,
    actor: []const u8,
    metadata_json: []const u8,
) !void {
    const audit_id = try id_format.generateEntitlementSnapshotId(alloc);
    defer alloc.free(audit_id);
    var q = try conn.query(
        \\INSERT INTO workspace_credit_audit
        \\  (audit_id, workspace_id, event_type, delta_credit_cents, remaining_credit_cents, reason, actor, metadata_json, created_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $9)
    , .{
        audit_id,
        workspace_id,
        event_type,
        delta_credit_cents,
        remaining_credit_cents,
        reason,
        actor,
        metadata_json,
        std.time.milliTimestamp(),
    });
    q.deinit();
}

test "provisionWorkspaceCredit grants initial free credit deterministically" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempCreditTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21");

    try provisionWorkspaceCredit(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21", "test");

    const credit = try getOrProvisionWorkspaceCredit(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21");
    defer std.testing.allocator.free(credit.currency);
    try std.testing.expectEqual(FREE_PLAN_INITIAL_CREDIT_CENTS, credit.initial_credit_cents);
    try std.testing.expectEqual(@as(i64, 0), credit.consumed_credit_cents);
    try std.testing.expectEqual(FREE_PLAN_INITIAL_CREDIT_CENTS, credit.remaining_credit_cents);
}

test "enforceExecutionAllowed blocks exhausted free plan and allows scale" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempCreditTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22");
    try upsertCreditState(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22", .{
        .currency = CREDIT_CURRENCY,
        .initial_credit_cents = FREE_PLAN_INITIAL_CREDIT_CENTS,
        .consumed_credit_cents = FREE_PLAN_INITIAL_CREDIT_CENTS,
        .remaining_credit_cents = 0,
        .exhausted_at = 123,
    }, 123);

    try std.testing.expectError(error.CreditExhausted, enforceExecutionAllowed(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22", .free));

    const scale_credit = try enforceExecutionAllowed(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22", .scale);
    defer std.testing.allocator.free(scale_credit.currency);
    try std.testing.expectEqual(@as(i64, 0), scale_credit.remaining_credit_cents);
}

fn openTestConn(alloc: std.mem.Allocator) !?struct { pool: *pg.Pool, conn: *pg.Conn } {
    const url = std.process.getEnvVarOwned(alloc, "HANDLER_DB_TEST_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);

    const db = @import("../db/pool.zig");
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const opts = try db.parseUrl(arena.allocator(), url);
    const pool = try pg.Pool.init(alloc, opts);
    errdefer pool.deinit();
    const conn = try pool.acquire();
    return .{ .pool = pool, .conn = conn };
}

fn createTempCreditTables(conn: *pg.Conn) !void {
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE workspaces (
            \\  workspace_id TEXT PRIMARY KEY
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE workspace_credit_state (
            \\  credit_id TEXT PRIMARY KEY,
            \\  workspace_id TEXT NOT NULL UNIQUE REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
            \\  currency TEXT NOT NULL,
            \\  initial_credit_cents BIGINT NOT NULL,
            \\  consumed_credit_cents BIGINT NOT NULL,
            \\  remaining_credit_cents BIGINT NOT NULL,
            \\  exhausted_at BIGINT,
            \\  created_at BIGINT NOT NULL,
            \\  updated_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE workspace_credit_audit (
            \\  audit_id TEXT PRIMARY KEY,
            \\  workspace_id TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
            \\  event_type TEXT NOT NULL,
            \\  delta_credit_cents BIGINT NOT NULL,
            \\  remaining_credit_cents BIGINT NOT NULL,
            \\  reason TEXT NOT NULL,
            \\  actor TEXT NOT NULL,
            \\  metadata_json TEXT NOT NULL,
            \\  created_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
}

fn seedWorkspace(conn: *pg.Conn, workspace_id: []const u8) !void {
    var q = try conn.query(
        "INSERT INTO workspaces (workspace_id) VALUES ($1)",
        .{workspace_id},
    );
    q.deinit();
}
