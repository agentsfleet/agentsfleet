//! `zombied reconcile` — standalone outbox reconciler.
//! Scans for stale pending side-effect outbox rows and dead-letters them.
//! Runs either as one-shot (`zombied reconcile`) or daemon
//! (`zombied reconcile --daemon`) mode.
//!
//! This file is a thin orchestrator that delegates to focused submodules:
//!   - `reconcile/args.zig`    — CLI/env argument parsing
//!   - `reconcile/daemon.zig`  — daemon lifecycle, leader lock, signal handlers
//!   - `reconcile/tick.zig`    — per-tick reconcile logic
//!   - `reconcile/emit.zig`    — result emission (log, JSON, OTLP)
//!   - `reconcile/metrics.zig` — HTTP /healthz and /metrics server
//!
//! Observability:
//!   - Structured JSON log line on every run (log aggregator pickup).
//!   - OTLP push if OTEL_EXPORTER_OTLP_ENDPOINT is set.
//!   - Exit code: 0 = success, 1 = fatal error.

const std = @import("std");
const posthog = @import("posthog");
const db = @import("../db/pool.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const sql_rollback = "ROLLBACK";
const outbox = @import("../state/outbox_reconciler.zig");
const id_format = @import("../types/id_format.zig");
const obs_log = @import("../observability/logging.zig");

const args_mod = @import("./reconcile/args.zig");
const daemon_mod = @import("./reconcile/daemon.zig");
const tick_mod = @import("./reconcile/tick.zig");

fn openDbOrExit(alloc: std.mem.Allocator) *db.Pool {
    const pool = db.initFromEnvForRole(alloc, .api) catch |err| {
        std.debug.print("fatal: database init failed: {}\n", .{err});
        std.process.exit(1);
    };
    return pool;
}

pub fn run(alloc: std.mem.Allocator) !void {
    const args = args_mod.parseArgs(alloc) catch |err| args_mod.printArgErrorAndExit(err);
    const pool = openDbOrExit(alloc);
    defer pool.deinit();
    const posthog_api_key = std.process.getEnvVarOwned(alloc, "POSTHOG_API_KEY") catch null;
    defer if (posthog_api_key) |key| alloc.free(key);
    const ph_client: ?*posthog.PostHogClient = if (posthog_api_key) |key| blk: {
        break :blk posthog.init(alloc, .{
            .api_key = key,
            .host = "https://us.i.posthog.com",
            .flush_interval_ms = 10_000,
            .flush_at = 20,
            .max_retries = 3,
        }) catch |err| {
            obs_log.logWarnErr(.reconcile, err, "posthog init failed; analytics disabled", .{});
            break :blk null;
        };
    } else null;
    defer if (ph_client) |client| client.deinit();

    switch (args.mode) {
        .one_shot => {
            if (!tick_mod.runOnce(alloc, pool, ph_client)) std.process.exit(1);
        },
        .daemon => try daemon_mod.runDaemon(alloc, pool, ph_client, args.interval_seconds, args.metrics_port),
    }
}

// ---------------------------------------------------------------------------
// Integration tests (retained in the thin orchestrator)
// ---------------------------------------------------------------------------

fn openReconcileTestConn(alloc: std.mem.Allocator) !?struct { pool: *db.Pool, conn: *db.Conn } {
    const url = std.process.getEnvVarOwned(alloc, "TEST_DATABASE_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);

    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const opts = try db.parseUrl(arena.allocator(), url);
    const pool = try db.Pool.init(alloc, opts);
    errdefer pool.deinit();
    const conn = try pool.acquire();
    return .{ .pool = pool, .conn = conn };
}

fn createTempOutboxTable(conn: *db.Conn) !void {
    _ = try conn.exec(
        \\CREATE TEMP TABLE run_side_effect_outbox (
        \\  id UUID PRIMARY KEY,
        \\  run_id TEXT NOT NULL,
        \\  effect_key TEXT NOT NULL,
        \\  status TEXT NOT NULL,
        \\  last_event TEXT NOT NULL DEFAULT 'claimed',
        \\  payload TEXT,
        \\  reconciled_state TEXT,
        \\  created_at BIGINT NOT NULL,
        \\  updated_at BIGINT NOT NULL
        \\) ON COMMIT DROP
    , .{});
}

fn insertPendingRows(conn: *db.Conn, count: usize) !void {
    var i: usize = 0;
    while (i < count) : (i += 1) {
        var key_buf: [48]u8 = undefined;
        const run_id = try id_format.generateRunId(std.testing.allocator);
        defer std.testing.allocator.free(run_id);
        const outbox_id = try id_format.generateOutboxId(std.testing.allocator);
        defer std.testing.allocator.free(outbox_id);
        const key = try std.fmt.bufPrint(&key_buf, "k_{d}", .{i});
        _ = try conn.exec(
            \\INSERT INTO run_side_effect_outbox
            \\  (id, run_id, effect_key, status, last_event, created_at, updated_at)
            \\VALUES ($1, $2, $3, 'pending', 'claimed', $4, $4)
        , .{ outbox_id, run_id, key, @as(i64, @intCast(i + 1)) });
    }
}

fn simulateSingleBatchDeadLetter(conn: *db.Conn, now_ms: i64) !u32 {
    var q = PgQuery.from(try conn.query(
        \\UPDATE run_side_effect_outbox
        \\SET status = 'dead_letter',
        \\    reconciled_state = 'startup_reconcile',
        \\    updated_at = $1
        \\WHERE id IN (
        \\    SELECT id FROM run_side_effect_outbox
        \\    WHERE status = 'pending'
        \\    ORDER BY created_at ASC
        \\    LIMIT $2
        \\    FOR UPDATE SKIP LOCKED
        \\)
        \\RETURNING id
    , .{ now_ms, @as(i32, @intCast(outbox.RECONCILE_BATCH_LIMIT)) }));
    defer q.deinit();

    var count: u32 = 0;
    while (try q.next()) |_| {
        count += 1;
    }
    return count;
}

fn pendingCount(conn: *db.Conn) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::BIGINT FROM run_side_effect_outbox WHERE status = 'pending'",
        .{},
    ));
    defer q.deinit();
    const row = (try q.next()).?;
    return try row.get(i64, 0);
}

test "integration: reconcile handles reachable postgres with no pending rows" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempOutboxTable(db_ctx.conn);

    const result = try outbox.reconcileStartup(db_ctx.conn);
    try std.testing.expectEqual(@as(u32, 0), result.dead_lettered);
}

test "integration: reconcile dead-letters stale pending rows and is idempotent" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempOutboxTable(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO run_side_effect_outbox
        \\  (id, run_id, effect_key, status, last_event, created_at, updated_at)
        \\VALUES
        \\  ('0195b4ba-8d3a-7f13-8abc-000000000001', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91', 'k1', 'pending', 'claimed', 1, 1),
        \\  ('0195b4ba-8d3a-7f13-8abc-000000000002', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f92', 'k2', 'pending', 'claimed', 2, 2),
        \\  ('0195b4ba-8d3a-7f13-8abc-000000000003', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f93', 'k3', 'delivered', 'done', 3, 3),
        \\  ('0195b4ba-8d3a-7f13-8abc-000000000004', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f94', 'k4', 'pending', 'claimed', 4, 4)
    , .{});

    const first = try outbox.reconcileStartup(db_ctx.conn);
    try std.testing.expectEqual(@as(u32, 3), first.dead_lettered);

    var counts_q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT
        \\  COUNT(*) FILTER (WHERE status = 'pending')::BIGINT,
        \\  COUNT(*) FILTER (WHERE status = 'dead_letter')::BIGINT,
        \\  COUNT(*) FILTER (WHERE status = 'delivered')::BIGINT
        \\FROM run_side_effect_outbox
    , .{}));
    defer counts_q.deinit();
    const row = (try counts_q.next()).?;
    try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    try std.testing.expectEqual(@as(i64, 3), try row.get(i64, 1));
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 2));

    const second = try outbox.reconcileStartup(db_ctx.conn);
    try std.testing.expectEqual(@as(u32, 0), second.dead_lettered);
}

test "integration: advisory lock enforces single active reconcile leader" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const conn_2 = try db_ctx.pool.acquire();
    defer db_ctx.pool.release(conn_2);

    try std.testing.expect(try daemon_mod.tryAcquireLeaderLock(db_ctx.conn));
    try std.testing.expect(!(try daemon_mod.tryAcquireLeaderLock(conn_2)));
    daemon_mod.releaseLeaderLock(db_ctx.conn);
    try std.testing.expect(try daemon_mod.tryAcquireLeaderLock(conn_2));
    daemon_mod.releaseLeaderLock(conn_2);
}

test "integration: reconciler restart drains remaining rows after partial pre-crash progress" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempOutboxTable(db_ctx.conn);
    try insertPendingRows(db_ctx.conn, 130);

    const one_batch = try simulateSingleBatchDeadLetter(db_ctx.conn, std.time.milliTimestamp());
    try std.testing.expectEqual(outbox.RECONCILE_BATCH_LIMIT, one_batch);
    try std.testing.expectEqual(@as(i64, 66), try pendingCount(db_ctx.conn));

    const after_restart = try outbox.reconcileStartup(db_ctx.conn);
    try std.testing.expectEqual(@as(u32, 66), after_restart.dead_lettered);
    try std.testing.expectEqual(@as(i64, 0), try pendingCount(db_ctx.conn));
}

test "integration: rollback preserves pending rows for restart recovery" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempOutboxTable(db_ctx.conn);
    try insertPendingRows(db_ctx.conn, 1);

    _ = try db_ctx.conn.exec("BEGIN", .{});
    errdefer {
        _ = db_ctx.conn.exec(sql_rollback, .{}) catch {};
    }

    _ = try db_ctx.conn.exec(
        \\UPDATE run_side_effect_outbox
        \\SET status = 'dead_letter', reconciled_state = 'simulated_crash', updated_at = $1
        \\WHERE status = 'pending'
    , .{std.time.milliTimestamp()});

    _ = try db_ctx.conn.exec(sql_rollback, .{});

    try std.testing.expectEqual(@as(i64, 1), try pendingCount(db_ctx.conn));

    const recovered = try outbox.reconcileStartup(db_ctx.conn);
    try std.testing.expectEqual(@as(u32, 1), recovered.dead_lettered);
    try std.testing.expectEqual(@as(i64, 0), try pendingCount(db_ctx.conn));
}
// ---------------------------------------------------------------------------
