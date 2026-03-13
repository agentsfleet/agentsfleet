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
        \\  AND source = 'runtime_stage'
    , .{ run_id, @as(i32, @intCast(attempt)) });
    defer q.deinit();

    const row = (try q.next()) orelse return 0;
    const raw = try row.get(i64, 0);
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
        configuredAdapterModeLabel(alloc),
        now_ms,
    });
    q.deinit();
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

test "completed runs are metered and non-billable runs are not charged" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempBillingTables(db_ctx.conn);

    try recordRuntimeStageUsage(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91",
        1,
        "plan",
        .echo,
        10,
        12,
    );
    try recordRuntimeStageUsage(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91",
        1,
        "implement",
        .scout,
        20,
        30,
    );

    try finalizeRunForBilling(
        std.testing.allocator,
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91",
        1,
        .completed,
    );

    {
        var q = try db_ctx.conn.query(
            \\SELECT billable_quantity, is_billable
            \\FROM usage_ledger
            \\WHERE run_id = $1 AND lifecycle_event = 'run_completed'
            \\LIMIT 1
        , .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91"});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 42), try row.get(i64, 0));
        try std.testing.expect(try row.get(bool, 1));
    }

    {
        var q = try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM billing_delivery_outbox WHERE run_id = $1",
            .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91"},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
    }

    try recordRuntimeStageUsage(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f92",
        1,
        "plan",
        .echo,
        8,
        9,
    );
    try finalizeRunForBilling(
        std.testing.allocator,
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f92",
        1,
        .non_billable,
    );

    {
        var q = try db_ctx.conn.query(
            \\SELECT billable_quantity, is_billable
            \\FROM usage_ledger
            \\WHERE run_id = $1 AND lifecycle_event = 'run_not_billable'
            \\LIMIT 1
        , .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f92"});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
        try std.testing.expect(!try row.get(bool, 1));
    }

    {
        var q = try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM billing_delivery_outbox WHERE run_id = $1",
            .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f92"},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
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

fn createTempBillingTables(conn: *pg.Conn) !void {
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE usage_ledger (
            \\  id BIGSERIAL PRIMARY KEY,
            \\  workspace_id TEXT NOT NULL,
            \\  run_id TEXT NOT NULL,
            \\  attempt INT NOT NULL,
            \\  actor TEXT NOT NULL,
            \\  token_count BIGINT NOT NULL DEFAULT 0,
            \\  agent_seconds BIGINT NOT NULL DEFAULT 0,
            \\  created_at BIGINT NOT NULL,
            \\  event_key TEXT,
            \\  lifecycle_event TEXT NOT NULL DEFAULT 'stage_completed',
            \\  billable_unit TEXT NOT NULL DEFAULT 'agent_second',
            \\  billable_quantity BIGINT NOT NULL DEFAULT 0,
            \\  is_billable BOOLEAN NOT NULL DEFAULT FALSE,
            \\  source TEXT NOT NULL DEFAULT 'runtime_stage',
            \\  UNIQUE (run_id, event_key)
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE billing_delivery_outbox (
            \\  id BIGSERIAL PRIMARY KEY,
            \\  run_id TEXT NOT NULL,
            \\  workspace_id TEXT NOT NULL,
            \\  attempt INT NOT NULL,
            \\  idempotency_key TEXT NOT NULL UNIQUE,
            \\  billable_unit TEXT NOT NULL,
            \\  billable_quantity BIGINT NOT NULL,
            \\  status TEXT NOT NULL DEFAULT 'pending',
            \\  delivery_attempts INT NOT NULL DEFAULT 0,
            \\  next_retry_at BIGINT NOT NULL DEFAULT 0,
            \\  adapter TEXT NOT NULL,
            \\  adapter_reference TEXT,
            \\  last_error TEXT,
            \\  created_at BIGINT NOT NULL,
            \\  updated_at BIGINT NOT NULL,
            \\  delivered_at BIGINT
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
}
