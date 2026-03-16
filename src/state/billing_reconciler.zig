const std = @import("std");
const pg = @import("pg");
const billing_adapter = @import("./billing_adapter.zig");

pub const DEFAULT_BATCH_LIMIT: u32 = 64;

pub const ReconcileResult = struct {
    scanned: u32 = 0,
    delivered: u32 = 0,
    retried: u32 = 0,
    dead_lettered: u32 = 0,
};

const PendingRow = struct {
    id: i64,
    run_id: []u8,
    workspace_id: []u8,
    attempt: u32,
    idempotency_key: []u8,
    billable_unit: []u8,
    billable_quantity: u64,
};

pub fn reconcilePending(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    adapter: billing_adapter.Adapter,
    batch_limit: u32,
) !ReconcileResult {
    const limit = if (batch_limit == 0) DEFAULT_BATCH_LIMIT else batch_limit;
    var result = ReconcileResult{};
    const now_ms = std.time.milliTimestamp();

    var query = try conn.query(
        \\SELECT id, run_id::TEXT, workspace_id::TEXT, attempt, idempotency_key, billable_unit, billable_quantity
        \\FROM billing_delivery_outbox
        \\WHERE status = 'pending' AND next_retry_at <= $1
        \\ORDER BY created_at ASC
        \\LIMIT $2
        \\FOR UPDATE SKIP LOCKED
    , .{ now_ms, @as(i32, @intCast(limit)) });
    defer query.deinit();

    while (try query.next()) |row| {
        const pending = PendingRow{
            .id = try row.get(i64, 0),
            .run_id = try row.get([]u8, 1),
            .workspace_id = try row.get([]u8, 2),
            .attempt = @as(u32, @intCast(try row.get(i32, 3))),
            .idempotency_key = try row.get([]u8, 4),
            .billable_unit = try row.get([]u8, 5),
            .billable_quantity = @as(u64, @intCast(@max(try row.get(i64, 6), 0))),
        };
        defer {
            alloc.free(pending.run_id);
            alloc.free(pending.workspace_id);
            alloc.free(pending.idempotency_key);
            alloc.free(pending.billable_unit);
        }

        result.scanned += 1;
        const delivered = billing_adapter.deliver(alloc, adapter, .{
            .idempotency_key = pending.idempotency_key,
            .workspace_id = pending.workspace_id,
            .run_id = pending.run_id,
            .attempt = pending.attempt,
            .billable_unit = pending.billable_unit,
            .billable_quantity = pending.billable_quantity,
        }) catch |err| {
            switch (err) {
                billing_adapter.AdapterError.AdapterUnavailable => {
                    try markRetry(conn, pending.id, now_ms, "adapter_unavailable");
                    result.retried += 1;
                },
                else => {
                    try markDeadLetter(conn, pending.id, now_ms, @errorName(err));
                    result.dead_lettered += 1;
                },
            }
            continue;
        };

        try markDelivered(conn, pending.id, now_ms, delivered.reference);
        result.delivered += 1;
    }

    return result;
}

fn markDelivered(conn: *pg.Conn, row_id: i64, now_ms: i64, adapter_reference: []const u8) !void {
    var q = try conn.query(
        \\UPDATE billing_delivery_outbox
        \\SET status = 'delivered',
        \\    adapter_reference = $1,
        \\    delivered_at = $2,
        \\    delivery_attempts = delivery_attempts + 1,
        \\    updated_at = $2
        \\WHERE id = $3
    , .{ adapter_reference, now_ms, row_id });
    q.deinit();
}

fn markRetry(conn: *pg.Conn, row_id: i64, now_ms: i64, err_name: []const u8) !void {
    const backoff_ms = @as(i64, 5_000);
    var q = try conn.query(
        \\UPDATE billing_delivery_outbox
        \\SET status = 'pending',
        \\    delivery_attempts = delivery_attempts + 1,
        \\    next_retry_at = $1,
        \\    last_error = $2,
        \\    updated_at = $3
        \\WHERE id = $4
    , .{ now_ms + backoff_ms, err_name, now_ms, row_id });
    q.deinit();
}

fn markDeadLetter(conn: *pg.Conn, row_id: i64, now_ms: i64, err_name: []const u8) !void {
    var q = try conn.query(
        \\UPDATE billing_delivery_outbox
        \\SET status = 'dead_letter',
        \\    delivery_attempts = delivery_attempts + 1,
        \\    last_error = $1,
        \\    updated_at = $2
        \\WHERE id = $3
    , .{ err_name, now_ms, row_id });
    q.deinit();
}

test "integration: adapter outage preserves accounting state and retries safely" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempBillingOutboxTable(db_ctx.conn);
    try seedPendingCharge(db_ctx.conn, 42);

    try std.posix.setenv("BILLING_ADAPTER_MODE", "provider_stub", true);
    try std.posix.setenv("BILLING_PROVIDER_API_KEY", "test-secret", true);
    try std.posix.setenv("BILLING_PROVIDER_STUB_OUTAGE", "1", true);
    defer std.posix.unsetenv("BILLING_ADAPTER_MODE");
    defer std.posix.unsetenv("BILLING_PROVIDER_API_KEY");
    defer std.posix.unsetenv("BILLING_PROVIDER_STUB_OUTAGE");

    var adapter = try billing_adapter.adapterFromEnv(std.testing.allocator);
    defer adapter.deinit(std.testing.allocator);

    const first = try reconcilePending(std.testing.allocator, db_ctx.conn, adapter, 16);
    try std.testing.expectEqual(@as(u32, 1), first.retried);
    try std.testing.expectEqual(@as(u32, 0), first.delivered);

    try assertOutboxRow(db_ctx.conn, "pending", 42, 1);

    try std.posix.setenv("BILLING_PROVIDER_STUB_OUTAGE", "0", true);
    const second = try reconcilePending(std.testing.allocator, db_ctx.conn, adapter, 16);
    try std.testing.expectEqual(@as(u32, 1), second.delivered);
    try assertOutboxRow(db_ctx.conn, "delivered", 42, 2);
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

fn createTempBillingOutboxTable(conn: *pg.Conn) !void {
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

fn seedPendingCharge(conn: *pg.Conn, quantity: u64) !void {
    var q = try conn.query(
        \\INSERT INTO billing_delivery_outbox
        \\  (run_id, workspace_id, attempt, idempotency_key, billable_unit, billable_quantity,
        \\   status, delivery_attempts, next_retry_at, adapter, created_at, updated_at)
        \\VALUES
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11',
        \\   1, 'billing:run:1', 'agent_second', $1, 'pending', 0, 0, 'provider_stub', 1, 1)
    , .{@as(i64, @intCast(quantity))});
    q.deinit();
}

fn assertOutboxRow(conn: *pg.Conn, expected_status: []const u8, expected_quantity: u64, expected_attempts: u32) !void {
    var q = try conn.query(
        \\SELECT status, billable_quantity, delivery_attempts
        \\FROM billing_delivery_outbox
        \\ORDER BY id DESC
        \\LIMIT 1
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedEqual;
    try std.testing.expectEqualStrings(expected_status, try row.get([]const u8, 0));
    try std.testing.expectEqual(@as(i64, @intCast(expected_quantity)), try row.get(i64, 1));
    try std.testing.expectEqual(@as(i32, @intCast(expected_attempts)), try row.get(i32, 2));
}
