const std = @import("std");
const pg = @import("pg");
const types = @import("../types.zig");
const workspace_billing = @import("./workspace_billing.zig");
const workspace_credit = @import("./workspace_credit.zig");
const id_format = @import("../types/id_format.zig");

const log = std.log.scoped(.state);

// ── Named constants for usage_ledger string fields (T10 compliance) ──────────
// These replace string literals in SQL queries so magic values live in one place.
pub const LEDGER_SOURCE_RUNTIME_STAGE = "runtime_stage";
pub const LEDGER_SOURCE_RUNTIME_SUMMARY = "runtime_summary";
pub const LEDGER_EVENT_STAGE_COMPLETED = "stage_completed";
pub const LEDGER_ACTOR_ORCHESTRATOR = "orchestrator";

pub const BillableUnit = enum {
    agent_second,

    pub fn label(self: BillableUnit) []const u8 {
        return switch (self) {
            .agent_second => "agent_second",
        };
    }
};

pub const FinalizeOutcome = enum {
    completed,
    non_billable,
    /// M27_002: run scored below BILLING_QUALITY_THRESHOLD; non-billable with distinct ledger reason.
    score_gated,
};

pub const UsageSnapshot = struct {
    event_key: []const u8,
    is_billable: bool,
    billable_quantity: u64,
};

pub fn recordRuntimeStageUsage(
    conn: *pg.Conn,
    workspace_id: []const u8,
    run_id: []const u8,
    attempt: u32,
    stage_id: []const u8,
    actor: types.Actor,
    token_count: u64,
    agent_seconds: u64,
) !void {
    var event_key_buf: [192]u8 = undefined;
    const event_key = try std.fmt.bufPrint(&event_key_buf, "stage:{d}:{s}:{s}", .{
        attempt,
        stage_id,
        actor.label(),
    });

    const now_ms = std.time.milliTimestamp();
    const usage_id = try id_format.generateUsageLedgerId(conn._allocator);
    defer conn._allocator.free(usage_id);
    _ = try conn.exec(
        \\INSERT INTO usage_ledger
        \\  (id, workspace_id, run_id, attempt, actor, token_count, agent_seconds, created_at,
        \\   event_key, lifecycle_event, billable_unit, billable_quantity, is_billable, source)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 0, false, $12)
        \\ON CONFLICT (run_id, event_key) DO NOTHING
    , .{
        usage_id,
        workspace_id,
        run_id,
        @as(i32, @intCast(attempt)),
        actor.label(),
        @as(i64, @intCast(token_count)),
        @as(i64, @intCast(agent_seconds)),
        now_ms,
        event_key,
        LEDGER_EVENT_STAGE_COMPLETED,
        BillableUnit.agent_second.label(),
        LEDGER_SOURCE_RUNTIME_STAGE,
    });
    log.debug("billing.stage_usage_recorded run_id={s} stage_id={s} agent_seconds={d}", .{ run_id, stage_id, agent_seconds });
}

pub fn finalizeRunForBilling(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    run_id: []const u8,
    attempt: u32,
    outcome: FinalizeOutcome,
) !void {
    const stage_total = try aggregateStageAgentSeconds(conn, run_id, attempt);
    const event_key = try std.fmt.allocPrint(alloc, "run_finalize:{d}:{s}", .{
        attempt,
        switch (outcome) {
            .completed => "completed",
            .non_billable => "non_billable",
            .score_gated => "score_gated",
        },
    });
    defer alloc.free(event_key);

    const is_billable = outcome == .completed and stage_total > 0;
    const billable_quantity: i64 = if (is_billable) @intCast(stage_total) else 0;
    const now_ms = std.time.milliTimestamp();

    const finalize_id = try id_format.generateUsageLedgerId(alloc);
    defer alloc.free(finalize_id);
    var q = try conn.query(
        \\INSERT INTO usage_ledger
        \\  (id, workspace_id, run_id, attempt, actor, token_count, agent_seconds, created_at,
        \\   event_key, lifecycle_event, billable_unit, billable_quantity, is_billable, source)
        \\VALUES ($1, $2, $3, $4, $5, 0, 0, $6, $7, $8, $9, $10, $11, $12)
        \\ON CONFLICT (run_id, event_key) DO NOTHING
        \\RETURNING 1
    , .{
        finalize_id,
        workspace_id,
        run_id,
        @as(i32, @intCast(attempt)),
        LEDGER_ACTOR_ORCHESTRATOR,
        now_ms,
        event_key,
        switch (outcome) {
            .completed => "run_completed",
            .non_billable => "run_not_billable",
            .score_gated => "run_not_billable_score_gated",
        },
        BillableUnit.agent_second.label(),
        billable_quantity,
        is_billable,
        LEDGER_SOURCE_RUNTIME_SUMMARY,
    });
    defer q.deinit();
    const inserted = (try q.next()) != null;
    try q.drain();

    if (!inserted or !is_billable) {
        log.debug("billing.finalize_skip run_id={s} inserted={} is_billable={}", .{ run_id, inserted, is_billable });
        return;
    }
    log.debug("billing.finalize_billing run_id={s} billable_quantity={d}", .{ run_id, billable_quantity });

    const billing_state = try workspace_billing.reconcileWorkspaceBilling(conn, alloc, workspace_id, now_ms, "worker");
    defer alloc.free(billing_state.plan_sku);
    defer if (billing_state.subscription_id) |value| alloc.free(value);

    if (billing_state.plan_tier == .free) {
        const updated_credit = try workspace_credit.deductCompletedRuntimeUsage(
            conn,
            alloc,
            workspace_id,
            run_id,
            attempt,
            stage_total,
            "worker",
        );
        alloc.free(updated_credit.currency);
        return;
    }

    try queueBillingDelivery(conn, alloc, workspace_id, run_id, attempt, stage_total);
}

pub fn aggregateStageAgentSeconds(
    conn: *pg.Conn,
    run_id: []const u8,
    attempt: u32,
) !u64 {
    var q = try conn.query(
        \\SELECT COALESCE(SUM(agent_seconds), 0)::BIGINT
        \\FROM usage_ledger
        \\WHERE run_id = $1
        \\  AND attempt = $2
        \\  AND source = $3
    , .{ run_id, @as(i32, @intCast(attempt)), LEDGER_SOURCE_RUNTIME_STAGE });
    defer q.deinit();

    const row = (try q.next()) orelse return 0;
    const raw = try row.get(i64, 0);
    try q.drain();
    return @as(u64, @intCast(@max(raw, 0)));
}

fn queueBillingDelivery(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    run_id: []const u8,
    attempt: u32,
    billable_quantity: u64,
) !void {
    const now_ms = std.time.milliTimestamp();
    var idem_buf: [192]u8 = undefined;
    const idempotency_key = try std.fmt.bufPrint(&idem_buf, "billing:{s}:{d}:{s}", .{
        run_id,
        attempt,
        BillableUnit.agent_second.label(),
    });

    const billing_id = try id_format.generateBillingDeliveryId(alloc);
    defer alloc.free(billing_id);
    _ = try conn.exec(
        \\INSERT INTO billing_delivery_outbox
        \\  (id, run_id, workspace_id, attempt, idempotency_key, billable_unit, billable_quantity,
        \\   status, delivery_attempts, next_retry_at, adapter, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', 0, 0, $8, $9, $9)
        \\ON CONFLICT (idempotency_key) DO UPDATE
        \\SET billable_quantity = EXCLUDED.billable_quantity,
        \\    adapter = EXCLUDED.adapter,
        \\    updated_at = EXCLUDED.updated_at
    , .{
        billing_id,
        run_id,
        workspace_id,
        @as(i32, @intCast(attempt)),
        idempotency_key,
        BillableUnit.agent_second.label(),
        @as(i64, @intCast(billable_quantity)),
        configuredAdapterModeLabel(alloc),
        now_ms,
    });
}

fn configuredAdapterModeLabel(alloc: std.mem.Allocator) []const u8 {
    const raw = std.process.getEnvVarOwned(alloc, "BILLING_ADAPTER_MODE") catch return "noop";
    defer alloc.free(raw);
    const trimmed = std.mem.trim(u8, raw, " \t\r\n");
    if (std.mem.eql(u8, trimmed, "manual")) return "manual";
    if (std.mem.eql(u8, trimmed, "provider_stub")) return "provider_stub";
    return "noop";
}

pub fn aggregateBillableQuantityFromSnapshots(
    alloc: std.mem.Allocator,
    snapshots: []const UsageSnapshot,
) !u64 {
    var seen = std.StringHashMap(void).init(alloc);
    defer seen.deinit();

    var total: u64 = 0;
    for (snapshots) |snapshot| {
        if (!snapshot.is_billable) continue;
        const gop = try seen.getOrPut(snapshot.event_key);
        if (gop.found_existing) continue;
        gop.value_ptr.* = {};
        total += snapshot.billable_quantity;
    }
    return total;
}
