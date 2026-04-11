const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");

pub const CreditRow = struct {
    currency: []u8,
    initial_credit_cents: i64,
    consumed_credit_cents: i64,
    remaining_credit_cents: i64,
    exhausted_at: ?i64,

    pub fn deinit(self: *CreditRow, alloc: std.mem.Allocator) void {
        alloc.free(self.currency);
    }
};

pub fn loadCreditRow(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !?CreditRow {
    var q = PgQuery.from(try conn.query(
        \\SELECT currency, initial_credit_cents, consumed_credit_cents, remaining_credit_cents, exhausted_at
        \\FROM workspace_credit_state
        \\WHERE workspace_id = $1
        \\LIMIT 1
    , .{workspace_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.WorkspaceCreditStateMissing;
    const currency = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(currency);
    const initial_credit_cents = try row.get(i64, 1);
    const consumed_credit_cents = try row.get(i64, 2);
    const remaining_credit_cents = try row.get(i64, 3);
    const exhausted_at = try row.get(?i64, 4);
    return .{
        .currency = currency,
        .initial_credit_cents = initial_credit_cents,
        .consumed_credit_cents = consumed_credit_cents,
        .remaining_credit_cents = remaining_credit_cents,
        .exhausted_at = exhausted_at,
    };
}

pub fn hasAuditEvent(
    conn: *pg.Conn,
    workspace_id: []const u8,
    event_type: []const u8,
    reason: []const u8,
    metadata_json: []const u8,
) !bool {
    var q = PgQuery.from(try conn.query(
        \\SELECT 1
        \\FROM workspace_credit_audit
        \\WHERE workspace_id = $1
        \\  AND event_type = $2
        \\  AND reason = $3
        \\  AND metadata_json = $4
        \\LIMIT 1
    , .{ workspace_id, event_type, reason, metadata_json }));
    defer q.deinit();
    return (try q.next()) != null;
}

pub fn upsertCreditState(
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
    _ = try conn.exec(
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
}

pub fn insertAudit(
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
    _ = try conn.exec(
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
}

pub fn runtimeDeductionMetadata(
    alloc: std.mem.Allocator,
    run_id: []const u8,
    attempt: u32,
    agent_seconds: u64,
    debit_cents: i64,
) ![]u8 {
    return std.fmt.allocPrint(
        alloc,
        "{{\"run_id\":\"{s}\",\"attempt\":{d},\"billable_unit\":\"agent_second\",\"billable_quantity\":{d},\"debit_cents\":{d}}}",
        .{ run_id, attempt, agent_seconds, debit_cents },
    );
}
