const std = @import("std");
const pg = @import("pg");
const billing = @import("./billing_runtime.zig");
const workspace_billing = @import("./workspace_billing.zig");
const workspace_credit = @import("./workspace_credit.zig");

test "ledger replay yields identical totals regardless of ordering" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const a = [_]billing.UsageSnapshot{
        .{ .event_key = "k1", .is_billable = true, .billable_quantity = 20 },
        .{ .event_key = "k2", .is_billable = false, .billable_quantity = 999 },
        .{ .event_key = "k3", .is_billable = true, .billable_quantity = 40 },
    };
    const b = [_]billing.UsageSnapshot{
        .{ .event_key = "k3", .is_billable = true, .billable_quantity = 40 },
        .{ .event_key = "k1", .is_billable = true, .billable_quantity = 20 },
        .{ .event_key = "k2", .is_billable = false, .billable_quantity = 999 },
    };

    const t1 = try billing.aggregateBillableQuantityFromSnapshots(alloc, &a);
    const t2 = try billing.aggregateBillableQuantityFromSnapshots(alloc, &b);
    try std.testing.expectEqual(t1, t2);
    try std.testing.expectEqual(@as(u64, 60), t1);
}

test "duplicate events do not double-charge" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const events = [_]billing.UsageSnapshot{
        .{ .event_key = "k1", .is_billable = true, .billable_quantity = 10 },
        .{ .event_key = "k1", .is_billable = true, .billable_quantity = 10 },
        .{ .event_key = "k2", .is_billable = true, .billable_quantity = 5 },
    };
    const total = try billing.aggregateBillableQuantityFromSnapshots(alloc, &events);
    try std.testing.expectEqual(@as(u64, 15), total);
}

test "completed runs are metered and non-billable runs are not charged" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempBillingTables(db_ctx.conn);
    try seedWorkspaceBillingState(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
        "FREE",
        "ACTIVE",
    );
    try seedWorkspaceCreditState(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
        workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS,
        0,
        workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS,
        null,
    );

    try billing.recordRuntimeStageUsage(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91",
        1,
        "plan",
        .echo,
        10,
        12,
    );
    try billing.recordRuntimeStageUsage(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91",
        1,
        "implement",
        .scout,
        20,
        30,
    );

    try billing.finalizeRunForBilling(
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
            \\SELECT consumed_credit_cents, remaining_credit_cents
            \\FROM workspace_credit_state
            \\WHERE workspace_id = $1
            \\LIMIT 1
        , .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 42), try row.get(i64, 0));
        try std.testing.expectEqual(workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS - 42, try row.get(i64, 1));
    }

    {
        var q = try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM billing_delivery_outbox WHERE run_id = $1",
            .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91"},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    }

    try billing.recordRuntimeStageUsage(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f92",
        1,
        "plan",
        .echo,
        8,
        9,
    );
    try billing.finalizeRunForBilling(
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

test "scale runs queue billing delivery without touching free-credit ledger" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempBillingTables(db_ctx.conn);
    try seedWorkspaceBillingState(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12",
        "SCALE",
        "ACTIVE",
    );
    try seedWorkspaceCreditState(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12",
        workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS,
        0,
        workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS,
        null,
    );

    try billing.recordRuntimeStageUsage(
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f95",
        1,
        "plan",
        .echo,
        7,
        15,
    );

    try billing.finalizeRunForBilling(
        std.testing.allocator,
        db_ctx.conn,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f95",
        1,
        .completed,
    );

    {
        var q = try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM billing_delivery_outbox WHERE run_id = $1",
            .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f95"},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
    }

    {
        var q = try db_ctx.conn.query(
            \\SELECT consumed_credit_cents, remaining_credit_cents
            \\FROM workspace_credit_state
            \\WHERE workspace_id = $1
            \\LIMIT 1
        , .{"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12"});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestExpectedEqual;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
        try std.testing.expectEqual(workspace_credit.FREE_PLAN_INITIAL_CREDIT_CENTS, try row.get(i64, 1));
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
            \\CREATE TEMP TABLE workspace_billing_state (
            \\  billing_id TEXT PRIMARY KEY,
            \\  workspace_id TEXT NOT NULL UNIQUE,
            \\  plan_tier TEXT NOT NULL,
            \\  plan_sku TEXT NOT NULL,
            \\  billing_status TEXT NOT NULL,
            \\  adapter TEXT NOT NULL,
            \\  subscription_id TEXT,
            \\  payment_failed_at BIGINT,
            \\  grace_expires_at BIGINT,
            \\  pending_status TEXT,
            \\  pending_reason TEXT,
            \\  created_at BIGINT NOT NULL,
            \\  updated_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE workspace_credit_state (
            \\  credit_id TEXT PRIMARY KEY,
            \\  workspace_id TEXT NOT NULL UNIQUE,
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
            \\  workspace_id TEXT NOT NULL,
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

fn seedWorkspaceBillingState(
    conn: *pg.Conn,
    workspace_id: []const u8,
    plan_tier: []const u8,
    billing_status: []const u8,
) !void {
    var q = try conn.query(
        \\INSERT INTO workspace_billing_state
        \\  (billing_id, workspace_id, plan_tier, plan_sku, billing_status, adapter, subscription_id,
        \\   payment_failed_at, grace_expires_at, pending_status, pending_reason, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, 'noop', NULL, NULL, NULL, NULL, NULL, 1, 1)
    , .{
        if (std.mem.eql(u8, plan_tier, "FREE")) "billing-free-test-id" else "billing-scale-test-id",
        workspace_id,
        plan_tier,
        if (std.mem.eql(u8, plan_tier, "FREE")) workspace_billing.FREE_PLAN_SKU else workspace_billing.SCALE_PLAN_SKU,
        billing_status,
    });
    q.deinit();
}

fn seedWorkspaceCreditState(
    conn: *pg.Conn,
    workspace_id: []const u8,
    initial_credit_cents: i64,
    consumed_credit_cents: i64,
    remaining_credit_cents: i64,
    exhausted_at: ?i64,
) !void {
    var q = try conn.query(
        \\INSERT INTO workspace_credit_state
        \\  (credit_id, workspace_id, currency, initial_credit_cents, consumed_credit_cents, remaining_credit_cents, exhausted_at, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, 1, 1)
    , .{
        if (std.mem.eql(u8, workspace_id, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11")) "credit-free-test-id" else "credit-scale-test-id",
        workspace_id,
        workspace_credit.CREDIT_CURRENCY,
        initial_credit_cents,
        consumed_credit_cents,
        remaining_credit_cents,
        exhausted_at,
    });
    q.deinit();
}
