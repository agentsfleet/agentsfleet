const std = @import("std");
const pg = @import("pg");
const billing_adapter = @import("./billing_adapter.zig");

const log = std.log.scoped(.state);

pub const DEFAULT_BATCH_LIMIT: u32 = 64;

pub const ReconcileResult = struct {
    scanned: u32 = 0,
    delivered: u32 = 0,
    retried: u32 = 0,
    dead_lettered: u32 = 0,
};

const PendingRow = struct {
    id: []u8,
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
    // check-pg-drain: ok — full while loop exhausts all rows, natural drain
    const limit = if (batch_limit == 0) DEFAULT_BATCH_LIMIT else batch_limit;
    var result = ReconcileResult{};
    const now_ms = std.time.milliTimestamp();

    var query = try conn.query(
        \\SELECT id::TEXT, run_id::TEXT, workspace_id::TEXT, attempt, idempotency_key, billable_unit, billable_quantity
        \\FROM billing_delivery_outbox
        \\WHERE status = 'pending' AND next_retry_at <= $1
        \\ORDER BY created_at ASC
        \\LIMIT $2
        \\FOR UPDATE SKIP LOCKED
    , .{ now_ms, @as(i32, @intCast(limit)) });
    defer query.deinit();

    while (try query.next()) |row| {
        const pending = PendingRow{
            .id = try row.get([]u8, 0),
            .run_id = try row.get([]u8, 1),
            .workspace_id = try row.get([]u8, 2),
            .attempt = @as(u32, @intCast(try row.get(i32, 3))),
            .idempotency_key = try row.get([]u8, 4),
            .billable_unit = try row.get([]u8, 5),
            .billable_quantity = @as(u64, @intCast(@max(try row.get(i64, 6), 0))),
        };
        defer {
            alloc.free(pending.id);
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

    log.info("billing.reconcile_cycle scanned={d} delivered={d} retried={d} dead_lettered={d}", .{ result.scanned, result.delivered, result.retried, result.dead_lettered });
    return result;
}

fn markDelivered(conn: *pg.Conn, row_id: []const u8, now_ms: i64, adapter_reference: []const u8) !void {
    _ = try conn.exec(
        \\UPDATE billing_delivery_outbox
        \\SET status = 'delivered',
        \\    adapter_reference = $1,
        \\    delivered_at = $2,
        \\    delivery_attempts = delivery_attempts + 1,
        \\    updated_at = $2
        \\WHERE id = $3::uuid
    , .{ adapter_reference, now_ms, row_id });
}

fn markRetry(conn: *pg.Conn, row_id: []const u8, now_ms: i64, err_name: []const u8) !void {
    const backoff_ms = @as(i64, 5_000);
    _ = try conn.exec(
        \\UPDATE billing_delivery_outbox
        \\SET status = 'pending',
        \\    delivery_attempts = delivery_attempts + 1,
        \\    next_retry_at = $1,
        \\    last_error = $2,
        \\    updated_at = $3
        \\WHERE id = $4::uuid
    , .{ now_ms + backoff_ms, err_name, now_ms, row_id });
}

fn markDeadLetter(conn: *pg.Conn, row_id: []const u8, now_ms: i64, err_name: []const u8) !void {
    _ = try conn.exec(
        \\UPDATE billing_delivery_outbox
        \\SET status = 'dead_letter',
        \\    delivery_attempts = delivery_attempts + 1,
        \\    last_error = $1,
        \\    updated_at = $2
        \\WHERE id = $3::uuid
    , .{ err_name, now_ms, row_id });
}

const base = @import("../db/test_fixtures.zig");
const uc3 = @import("../db/test_fixtures_uc3.zig");

fn seedBillingOutboxRow(conn: *pg.Conn, outbox_id: []const u8, quantity: u64) !void {
    _ = try conn.exec(
        \\INSERT INTO billing_delivery_outbox
        \\  (id, run_id, workspace_id, attempt, idempotency_key, billable_unit, billable_quantity,
        \\   status, delivery_attempts, next_retry_at, adapter, created_at, updated_at)
        \\VALUES
        \\  ($1::uuid, $2::uuid, $3::uuid,
        \\   1, 'billing:reconciler:test:1', 'agent_second', $4,
        \\   'pending', 0, 0, 'provider_stub', 1, 1)
        \\ON CONFLICT DO NOTHING
    , .{ outbox_id, uc3.RUN_RECONCILER, uc3.WS_RECONCILER, @as(i64, @intCast(quantity)) });
}

fn assertOutboxRow(conn: *pg.Conn, outbox_id: []const u8, expected_status: []const u8, expected_quantity: u64, expected_attempts: u32) !void {
    var q = try conn.query(
        \\SELECT status, billable_quantity, delivery_attempts
        \\FROM billing_delivery_outbox
        \\WHERE id = $1::uuid
    , .{outbox_id});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedEqual;
    const status = try row.get([]const u8, 0);
    const qty = try row.get(i64, 1);
    const attempts = try row.get(i32, 2);
    try q.drain();
    try std.testing.expectEqualStrings(expected_status, status);
    try std.testing.expectEqual(@as(i64, @intCast(expected_quantity)), qty);
    try std.testing.expectEqual(@as(i32, @intCast(expected_attempts)), attempts);
}

test "integration: adapter outage preserves accounting state and retries safely" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc3.teardownWithRuns(db_ctx.conn, uc3.WS_RECONCILER);
    defer uc3.teardownWithRuns(db_ctx.conn, uc3.WS_RECONCILER);

    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, uc3.WS_RECONCILER);
    try base.seedSpec(db_ctx.conn, uc3.SPEC_RECONCILER, uc3.WS_RECONCILER);
    try base.seedRun(db_ctx.conn, uc3.RUN_RECONCILER, uc3.WS_RECONCILER, uc3.SPEC_RECONCILER);
    try seedBillingOutboxRow(db_ctx.conn, uc3.OUTBOX_RECONCILER, 42);

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

    try assertOutboxRow(db_ctx.conn, uc3.OUTBOX_RECONCILER, "pending", 42, 1);

    try std.posix.setenv("BILLING_PROVIDER_STUB_OUTAGE", "0", true);
    const second = try reconcilePending(std.testing.allocator, db_ctx.conn, adapter, 16);
    try std.testing.expectEqual(@as(u32, 1), second.delivered);
    try assertOutboxRow(db_ctx.conn, uc3.OUTBOX_RECONCILER, "delivered", 42, 2);
}
