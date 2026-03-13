const std = @import("std");
const pg = @import("pg");
const types = @import("../types.zig");

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
    var q = try conn.query(
        \\INSERT INTO usage_ledger
        \\  (workspace_id, run_id, attempt, actor, token_count, agent_seconds, created_at,
        \\   event_key, lifecycle_event, billable_unit, billable_quantity, is_billable, source)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'stage_completed', $9, 0, false, 'runtime_stage')
        \\ON CONFLICT (run_id, event_key) DO NOTHING
    , .{
        workspace_id,
        run_id,
        @as(i32, @intCast(attempt)),
        actor.label(),
        @as(i64, @intCast(token_count)),
        @as(i64, @intCast(agent_seconds)),
        now_ms,
        event_key,
        BillableUnit.agent_second.label(),
    });
    q.deinit();
}

pub fn finalizeRunForBilling(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    run_id: []const u8,
    attempt: u32,
    outcome: FinalizeOutcome,
    adapter_mode: []const u8,
) !void {
    const stage_total = try aggregateStageAgentSeconds(conn, run_id, attempt);
    const now_ms = std.time.milliTimestamp();
    const event_key = try std.fmt.allocPrint(alloc, "run_finalize:{d}:{s}", .{
        attempt,
        switch (outcome) {
            .completed => "completed",
            .non_billable => "non_billable",
        },
    });
    defer alloc.free(event_key);

    const is_billable = outcome == .completed and stage_total > 0;
    const billable_quantity: i64 = if (is_billable) @intCast(stage_total) else 0;

    var q = try conn.query(
        \\INSERT INTO usage_ledger
        \\  (workspace_id, run_id, attempt, actor, token_count, agent_seconds, created_at,
        \\   event_key, lifecycle_event, billable_unit, billable_quantity, is_billable, source)
        \\VALUES ($1, $2, $3, 'orchestrator', 0, 0, $4, $5, $6, $7, $8, $9, 'runtime_summary')
        \\ON CONFLICT (run_id, event_key) DO NOTHING
    , .{
        workspace_id,
        run_id,
        @as(i32, @intCast(attempt)),
        now_ms,
        event_key,
        if (outcome == .completed) "run_completed" else "run_not_billable",
        BillableUnit.agent_second.label(),
        billable_quantity,
        is_billable,
    });
    q.deinit();

    if (!is_billable) return;
    try queueBillingDelivery(conn, workspace_id, run_id, attempt, stage_total, adapter_mode);
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
        \\  AND source = 'runtime_stage'
    , .{ run_id, @as(i32, @intCast(attempt)) });
    defer q.deinit();

    const row = (try q.next()) orelse return 0;
    const raw = try row.get(i64, 0);
    return @as(u64, @intCast(@max(raw, 0)));
}

fn queueBillingDelivery(
    conn: *pg.Conn,
    workspace_id: []const u8,
    run_id: []const u8,
    attempt: u32,
    billable_quantity: u64,
    adapter_mode: []const u8,
) !void {
    const now_ms = std.time.milliTimestamp();
    var idem_buf: [192]u8 = undefined;
    const idempotency_key = try std.fmt.bufPrint(&idem_buf, "billing:{s}:{d}:{s}", .{
        run_id,
        attempt,
        BillableUnit.agent_second.label(),
    });

    var q = try conn.query(
        \\INSERT INTO billing_delivery_outbox
        \\  (run_id, workspace_id, attempt, idempotency_key, billable_unit, billable_quantity,
        \\   status, delivery_attempts, next_retry_at, adapter, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, 'pending', 0, 0, $7, $8, $8)
        \\ON CONFLICT (idempotency_key) DO UPDATE
        \\SET billable_quantity = EXCLUDED.billable_quantity,
        \\    adapter = EXCLUDED.adapter,
        \\    updated_at = EXCLUDED.updated_at
    , .{
        run_id,
        workspace_id,
        @as(i32, @intCast(attempt)),
        idempotency_key,
        BillableUnit.agent_second.label(),
        @as(i64, @intCast(billable_quantity)),
        adapter_mode,
        now_ms,
    });
    q.deinit();
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

test "ledger replay yields identical totals regardless of ordering" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const a = [_]UsageSnapshot{
        .{ .event_key = "k1", .is_billable = true, .billable_quantity = 20 },
        .{ .event_key = "k2", .is_billable = false, .billable_quantity = 999 },
        .{ .event_key = "k3", .is_billable = true, .billable_quantity = 40 },
    };
    const b = [_]UsageSnapshot{
        .{ .event_key = "k3", .is_billable = true, .billable_quantity = 40 },
        .{ .event_key = "k1", .is_billable = true, .billable_quantity = 20 },
        .{ .event_key = "k2", .is_billable = false, .billable_quantity = 999 },
    };

    const t1 = try aggregateBillableQuantityFromSnapshots(alloc, &a);
    const t2 = try aggregateBillableQuantityFromSnapshots(alloc, &b);
    try std.testing.expectEqual(t1, t2);
    try std.testing.expectEqual(@as(u64, 60), t1);
}

test "duplicate events do not double-charge" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const events = [_]UsageSnapshot{
        .{ .event_key = "k1", .is_billable = true, .billable_quantity = 10 },
        .{ .event_key = "k1", .is_billable = true, .billable_quantity = 10 },
        .{ .event_key = "k2", .is_billable = true, .billable_quantity = 5 },
    };
    const total = try aggregateBillableQuantityFromSnapshots(alloc, &events);
    try std.testing.expectEqual(@as(u64, 15), total);
}
