const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const error_codes = @import("../errors/error_registry.zig");
const id_format = @import("../types/id_format.zig");
const billing = @import("./workspace_billing.zig");
const store = @import("workspace_credit_store.zig");

const log = std.log.scoped(.state);

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

// bvisor pattern: comptime-validated error mapping table.
const credit_error_table = [_]error_codes.ErrorMapping{
    .{ .err = error.CreditExhausted, .code = error_codes.ERR_CREDIT_EXHAUSTED, .message = "Free plan credit exhausted. Upgrade to Scale to continue." },
};
comptime {
    error_codes.validateErrorTable(&credit_error_table);
}

pub fn errorCode(err: anyerror) ?[]const u8 {
    inline for (credit_error_table) |entry| {
        if (err == entry.err) return entry.code;
    }
    return null;
}

pub fn errorMessage(err: anyerror) ?[]const u8 {
    inline for (credit_error_table) |entry| {
        if (err == entry.err) return entry.message;
    }
    return null;
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
    log.info("credit.provisioned workspace_id={s} initial_cents={d}", .{ workspace_id, FREE_PLAN_INITIAL_CREDIT_CENTS });
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

test {
    _ = @import("workspace_credit_test.zig");
}
