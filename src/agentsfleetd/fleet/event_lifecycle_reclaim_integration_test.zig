const std = @import("std");
const queue_consts = @import("../queue/constants.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const event_rows = @import("event_rows.zig");
const reclaim_sweeper = @import("reclaim_sweeper.zig");
const base = @import("event_lifecycle_integration_test.zig");

test "approval deadline expiry writes the terminal row: gate_blocked + approval_expired + XACK" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.AGENTSFLEET_GATED_EXP, "lifecycle-gatex", base.CONFIG_GATED_FAST, "8");

    const event_id = try base.publishEvent(h, base.AGENTSFLEET_GATED_EXP);
    defer h.queue.alloc.free(event_id);

    try std.testing.expect(!try base.pollLease(h));
    try base.expectRow(conn, base.AGENTSFLEET_GATED_EXP, event_id, event_rows.STATUS_RECEIVED, "");
    try std.testing.expectEqual(@as(i64, 1), try base.pendingCount(h, base.AGENTSFLEET_GATED_EXP));

    @import("common").sleepNanos(5 * std.time.ns_per_ms);
    try std.testing.expect(!try base.pollLease(h));
    try base.expectRow(conn, base.AGENTSFLEET_GATED_EXP, event_id, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_APPROVAL_EXPIRED);
    try std.testing.expectEqual(@as(i64, 0), try base.pendingCount(h, base.AGENTSFLEET_GATED_EXP));
}

test "markBlocked is guarded: terminal rows never reopen, second transition affects zero rows" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.AGENTSFLEET_ROW, "lifecycle-row", base.CONFIG_PLAIN, "6");
    const EVENT_ID = "1700000000000-7";
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (uid, fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, created_at, updated_at)
        \\VALUES ('0195c9da-1e2a-7f13-8abc-2b3e1e0d7e01'::uuid, $1::uuid, $2, $3::uuid,
        \\        'steer:test', 'chat', $4, '{}'::jsonb, 0, 0)
        \\ON CONFLICT (fleet_id, event_id) DO UPDATE SET status = EXCLUDED.status, failure_label = NULL
    , .{ base.AGENTSFLEET_ROW, EVENT_ID, base.WORKSPACE_ID, event_rows.STATUS_RECEIVED });

    try std.testing.expectEqual(@as(i64, 1), try event_rows.markBlocked(h.pool, base.AGENTSFLEET_ROW, EVENT_ID, event_rows.LABEL_BALANCE_EXHAUSTED));
    try base.expectRow(conn, base.AGENTSFLEET_ROW, EVENT_ID, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_BALANCE_EXHAUSTED);
    try std.testing.expectEqual(@as(i64, 0), try event_rows.markBlocked(h.pool, base.AGENTSFLEET_ROW, EVENT_ID, event_rows.LABEL_APPROVAL_DENIED));
    try base.expectRow(conn, base.AGENTSFLEET_ROW, EVENT_ID, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_BALANCE_EXHAUSTED);
}

test "terminal entry re-delivered from the PEL is re-acked, never re-executed" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.AGENTSFLEET_REACK, "lifecycle-reack", base.CONFIG_PLAIN, "7");

    const event_id = try base.publishEvent(h, base.AGENTSFLEET_REACK);
    defer h.queue.alloc.free(event_id);
    try std.testing.expect(try base.pollLease(h));
    try std.testing.expectEqual(@as(i64, 1), try base.pendingCount(h, base.AGENTSFLEET_REACK));

    _ = try conn.exec(
        "UPDATE core.fleet_events SET status = $3 WHERE fleet_id = $1::uuid AND event_id = $2",
        .{ base.AGENTSFLEET_REACK, event_id, event_rows.STATUS_PROCESSED },
    );
    _ = try conn.exec("DELETE FROM fleet.runner_leases WHERE workspace_id = $1::uuid", .{base.WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{base.AGENTSFLEET_REACK});

    try std.testing.expect(!try base.pollLease(h));
    try std.testing.expectEqual(@as(i64, 0), try base.pendingCount(h, base.AGENTSFLEET_REACK));
    try base.expectRow(conn, base.AGENTSFLEET_REACK, event_id, event_rows.STATUS_PROCESSED, "");
}

test "consumer identity is stable: repeated idle probes leave one consumer in the group" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.FLEET_IDLE, "lifecycle-idle", base.CONFIG_PLAIN, "4");
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, base.FLEET_IDLE);

    var i: usize = 0;
    while (i < 25) : (i += 1) _ = try base.pollLease(h);
    try std.testing.expectEqual(@as(usize, 1), try base.consumerCount(h, base.FLEET_IDLE));
}

test "reclaim sweep recovers a stranded delivery from a dead consumer and re-leases it" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.AGENTSFLEET_STRAND, "lifecycle-strand", base.CONFIG_PLAIN, "5");

    const event_id = try base.publishEvent(h, base.AGENTSFLEET_STRAND);
    defer h.queue.alloc.free(event_id);
    try base.deliverToDeadConsumer(h, base.AGENTSFLEET_STRAND);
    try base.forceIdle(h, base.AGENTSFLEET_STRAND, event_id, base.FORCED_IDLE_MS);

    const stats = try reclaim_sweeper.sweepOnce(h.pool, &h.queue, std.testing.allocator);
    try std.testing.expect(stats.reclaimed_entries >= 1);
    try std.testing.expect(try base.pollLease(h));
    try base.expectRow(conn, base.AGENTSFLEET_STRAND, event_id, event_rows.STATUS_RECEIVED, "");
}

test "reclaim sweep never touches an entry inside the lease window" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.AGENTSFLEET_STRAND, "lifecycle-strand", base.CONFIG_PLAIN, "5");

    const event_id = try base.publishEvent(h, base.AGENTSFLEET_STRAND);
    defer h.queue.alloc.free(event_id);
    try base.deliverToDeadConsumer(h, base.AGENTSFLEET_STRAND);

    const stats = try reclaim_sweeper.sweepOnce(h.pool, &h.queue, std.testing.allocator);
    try std.testing.expectEqual(@as(i64, 0), stats.reclaimed_entries);
    try std.testing.expectEqual(@as(i64, 1), try base.pendingCount(h, base.AGENTSFLEET_STRAND));
}

test "reclaim min-idle exceeds the lease window" {
    try std.testing.expect(queue_consts.fleet_xautoclaim_min_idle_ms_int > @import("common").LEASE_TTL_MS);
}
