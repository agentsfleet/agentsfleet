const std = @import("std");
const pg = @import("pg");
const error_codes = @import("../errors/codes.zig");
const id_format = @import("../types/id_format.zig");
const billing = @import("./workspace_billing.zig");
const store = @import("workspace_credit_store.zig");

pub const FREE_PLAN_INITIAL_CREDIT_CENTS: i64 = 1000;
pub const FREE_PLAN_CENTS_PER_AGENT_SECOND: i64 = 1;
pub const CREDIT_CURRENCY = "USD";

pub const CreditView = struct {
    currency: []const u8,
    initial_credit_cents: i64,
    consumed_credit_cents: i64,
    remaining_credit_cents: i64,
    exhausted_at: ?i64,
};

const CreditRow = store.CreditRow;

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
    try store.upsertCreditState(conn, alloc, workspace_id, .{
        .currency = CREDIT_CURRENCY,
        .initial_credit_cents = FREE_PLAN_INITIAL_CREDIT_CENTS,
        .consumed_credit_cents = 0,
        .remaining_credit_cents = FREE_PLAN_INITIAL_CREDIT_CENTS,
        .exhausted_at = null,
    }, now_ms);
    try store.insertAudit(conn, alloc, workspace_id, "CREDIT_GRANTED", FREE_PLAN_INITIAL_CREDIT_CENTS, FREE_PLAN_INITIAL_CREDIT_CENTS, "workspace_created", actor, "{}");
}

pub fn getOrProvisionWorkspaceCredit(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !CreditView {
    const existing = store.loadCreditRow(conn, alloc, workspace_id) catch |err| switch (err) {
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

pub fn deductCompletedRuntimeUsage(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    run_id: []const u8,
    attempt: u32,
    agent_seconds: u64,
    actor: []const u8,
) !CreditView {
    const debit_cents = runtimeUsageCostCents(agent_seconds);
    if (debit_cents <= 0) return getOrProvisionWorkspaceCredit(conn, alloc, workspace_id);

    const metadata_json = try store.runtimeDeductionMetadata(alloc, run_id, attempt, agent_seconds, debit_cents);
    defer alloc.free(metadata_json);

    const existing_audit = try store.hasAuditEvent(
        conn,
        workspace_id,
        "CREDIT_DEDUCTED",
        "runtime_completed",
        metadata_json,
    );
    if (existing_audit) return getOrProvisionWorkspaceCredit(conn, alloc, workspace_id);

    const now_ms = std.time.milliTimestamp();
    const current = try getOrProvisionWorkspaceCredit(conn, alloc, workspace_id);
    defer alloc.free(current.currency);

    const applied_debit = @min(current.remaining_credit_cents, debit_cents);
    if (applied_debit <= 0) return .{
        .currency = try alloc.dupe(u8, current.currency),
        .initial_credit_cents = current.initial_credit_cents,
        .consumed_credit_cents = current.consumed_credit_cents,
        .remaining_credit_cents = current.remaining_credit_cents,
        .exhausted_at = current.exhausted_at,
    };

    const next_remaining = current.remaining_credit_cents - applied_debit;
    const next_exhausted_at = if (next_remaining == 0) current.exhausted_at orelse now_ms else null;
    try store.upsertCreditState(conn, alloc, workspace_id, .{
        .currency = CREDIT_CURRENCY,
        .initial_credit_cents = current.initial_credit_cents,
        .consumed_credit_cents = current.consumed_credit_cents + applied_debit,
        .remaining_credit_cents = next_remaining,
        .exhausted_at = next_exhausted_at,
    }, now_ms);
    try store.insertAudit(
        conn,
        alloc,
        workspace_id,
        "CREDIT_DEDUCTED",
        -applied_debit,
        next_remaining,
        "runtime_completed",
        actor,
        metadata_json,
    );
    if (next_remaining == 0 and current.remaining_credit_cents > 0) {
        try store.insertAudit(
            conn,
            alloc,
            workspace_id,
            "CREDIT_EXHAUSTED",
            0,
            0,
            "runtime_completed",
            actor,
            metadata_json,
        );
    }

    return .{
        .currency = try alloc.dupe(u8, CREDIT_CURRENCY),
        .initial_credit_cents = current.initial_credit_cents,
        .consumed_credit_cents = current.consumed_credit_cents + applied_debit,
        .remaining_credit_cents = next_remaining,
        .exhausted_at = next_exhausted_at,
    };
}

pub fn runtimeUsageCostCents(agent_seconds: u64) i64 {
    if (agent_seconds == 0) return 0;
    const seconds = std.math.cast(i64, agent_seconds) orelse return std.math.maxInt(i64);
    return std.math.mul(i64, seconds, FREE_PLAN_CENTS_PER_AGENT_SECOND) catch std.math.maxInt(i64);
}

// T1 + T2 — runtimeUsageCostCents: zero, normal, and u64 overflow all clamp safely
test "runtimeUsageCostCents handles zero, normal, and overflow inputs" {
    try std.testing.expectEqual(@as(i64, 0), runtimeUsageCostCents(0));
    try std.testing.expectEqual(@as(i64, 42), runtimeUsageCostCents(42));
    try std.testing.expectEqual(std.math.maxInt(i64), runtimeUsageCostCents(std.math.maxInt(u64)));
}

// T10 — errorCode and errorMessage cover the CreditExhausted branch and unknown passthrough
test "errorCode and errorMessage map CreditExhausted; return null for unknown errors" {
    try std.testing.expect(errorCode(error.CreditExhausted) != null);
    try std.testing.expectEqual(@as(?[]const u8, null), errorCode(error.OutOfMemory));
    const msg = errorMessage(error.CreditExhausted);
    try std.testing.expect(msg != null);
    try std.testing.expect(std.mem.indexOf(u8, msg.?, "Free plan") != null);
    try std.testing.expectEqual(@as(?[]const u8, null), errorMessage(error.OutOfMemory));
}

test "provisionWorkspaceCredit grants initial free credit deterministically" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

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
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempCreditTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22");
    try store.upsertCreditState(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22", .{
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

test "deductCompletedRuntimeUsage debits free-plan credit once per completed run" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempCreditTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f23");
    try provisionWorkspaceCredit(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f23", "test");

    const first = try deductCompletedRuntimeUsage(
        db_ctx.conn,
        std.testing.allocator,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f23",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f93",
        1,
        42,
        "worker",
    );
    defer std.testing.allocator.free(first.currency);
    try std.testing.expectEqual(@as(i64, 42), first.consumed_credit_cents);
    try std.testing.expectEqual(FREE_PLAN_INITIAL_CREDIT_CENTS - 42, first.remaining_credit_cents);

    const second = try deductCompletedRuntimeUsage(
        db_ctx.conn,
        std.testing.allocator,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f23",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f93",
        1,
        42,
        "worker",
    );
    defer std.testing.allocator.free(second.currency);
    try std.testing.expectEqual(@as(i64, 42), second.consumed_credit_cents);
    try std.testing.expectEqual(FREE_PLAN_INITIAL_CREDIT_CENTS - 42, second.remaining_credit_cents);

    {
        var q = try db_ctx.conn.query(
            \\SELECT COUNT(*)::BIGINT
            \\FROM workspace_credit_audit
            \\WHERE workspace_id = $1
            \\  AND event_type = 'CREDIT_DEDUCTED'
        , .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f23"});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        const cnt = try row.get(i64, 0);
        try q.drain();
        try std.testing.expectEqual(@as(i64, 1), cnt);
    }
}

test "deductCompletedRuntimeUsage clamps to zero and records exhaustion" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempCreditTables(db_ctx.conn);
    try seedWorkspace(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f24");
    try store.upsertCreditState(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f24", .{
        .currency = CREDIT_CURRENCY,
        .initial_credit_cents = FREE_PLAN_INITIAL_CREDIT_CENTS,
        .consumed_credit_cents = 995,
        .remaining_credit_cents = 5,
        .exhausted_at = null,
    }, 123);

    const credit = try deductCompletedRuntimeUsage(
        db_ctx.conn,
        std.testing.allocator,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f24",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f94",
        1,
        30,
        "worker",
    );
    defer std.testing.allocator.free(credit.currency);
    try std.testing.expectEqual(FREE_PLAN_INITIAL_CREDIT_CENTS, credit.consumed_credit_cents);
    try std.testing.expectEqual(@as(i64, 0), credit.remaining_credit_cents);
    try std.testing.expect(credit.exhausted_at != null);

    {
        var q = try db_ctx.conn.query(
            \\SELECT COUNT(*)::BIGINT
            \\FROM workspace_credit_audit
            \\WHERE workspace_id = $1
            \\  AND event_type = 'CREDIT_EXHAUSTED'
        , .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f24"});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        const cnt = try row.get(i64, 0);
        try q.drain();
        try std.testing.expectEqual(@as(i64, 1), cnt);
    }
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
        _ = try conn.exec(
            \\CREATE TEMP TABLE workspaces (
            \\  workspace_id TEXT PRIMARY KEY
            \\) ON COMMIT DROP
        , .{});
    }
    {
        _ = try conn.exec(
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
    }
    {
        _ = try conn.exec(
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
    }
}

fn seedWorkspace(conn: *pg.Conn, workspace_id: []const u8) !void {
    _ = try conn.exec(
        "INSERT INTO workspaces (workspace_id) VALUES ($1)",
        .{workspace_id},
    );
}
