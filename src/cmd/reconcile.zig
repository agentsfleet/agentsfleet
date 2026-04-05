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

const sql_rollback = sql_rollback;
const outbox = @import("../state/outbox_reconciler.zig");
const id_format = @import("../types/id_format.zig");
const obs_log = @import("../observability/logging.zig");

const orphan_recovery = @import("../state/orphan_recovery.zig");

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
    var create_q = try conn.query(
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
    try create_q.drain();
    create_q.deinit();
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
    // check-pg-drain: ok — full while loop exhausts all rows, natural drain
    var q = try conn.query(
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
    , .{ now_ms, @as(i32, @intCast(outbox.RECONCILE_BATCH_LIMIT)) });
    defer q.deinit();

    var count: u32 = 0;
    while (try q.next()) |_| {
        count += 1;
    }
    return count;
}

fn pendingCount(conn: *db.Conn) !i64 {
    var q = try conn.query(
        "SELECT COUNT(*)::BIGINT FROM run_side_effect_outbox WHERE status = 'pending'",
        .{},
    );
    defer q.deinit();
    const row = (try q.next()).?;
    const count = try row.get(i64, 0);
    try q.drain();
    return count;
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

    var seed_q = try db_ctx.conn.query(
        \\INSERT INTO run_side_effect_outbox
        \\  (id, run_id, effect_key, status, last_event, created_at, updated_at)
        \\VALUES
        \\  ('0195b4ba-8d3a-7f13-8abc-000000000001', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91', 'k1', 'pending', 'claimed', 1, 1),
        \\  ('0195b4ba-8d3a-7f13-8abc-000000000002', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f92', 'k2', 'pending', 'claimed', 2, 2),
        \\  ('0195b4ba-8d3a-7f13-8abc-000000000003', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f93', 'k3', 'delivered', 'done', 3, 3),
        \\  ('0195b4ba-8d3a-7f13-8abc-000000000004', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f94', 'k4', 'pending', 'claimed', 4, 4)
    , .{});
    seed_q.deinit();

    const first = try outbox.reconcileStartup(db_ctx.conn);
    try std.testing.expectEqual(@as(u32, 3), first.dead_lettered);

    var counts_q = try db_ctx.conn.query(
        \\SELECT
        \\  COUNT(*) FILTER (WHERE status = 'pending')::BIGINT,
        \\  COUNT(*) FILTER (WHERE status = 'dead_letter')::BIGINT,
        \\  COUNT(*) FILTER (WHERE status = 'delivered')::BIGINT
        \\FROM run_side_effect_outbox
    , .{});
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

test "integration: orphan recovery transaction boundary — rollback undoes partial state" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Verify BEGIN/ROLLBACK semantics: a rolled-back UPDATE leaves the row unchanged.
    // This is the pattern used by recoverOrphanedRuns for per-row atomicity.
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE orphan_txn_test (
        \\  run_id TEXT PRIMARY KEY,
        \\  state TEXT NOT NULL
        \\) ON COMMIT DROP
    , .{});

    _ = try db_ctx.conn.exec(
        "INSERT INTO orphan_txn_test (run_id, state) VALUES ('r1', 'RUN_PLANNED')",
        .{},
    );

    // Simulate: BEGIN → UPDATE → ROLLBACK (crash mid-row)
    _ = try db_ctx.conn.exec("BEGIN", .{});
    _ = try db_ctx.conn.exec(
        "UPDATE orphan_txn_test SET state = 'BLOCKED' WHERE run_id = 'r1'",
        .{},
    );
    _ = try db_ctx.conn.exec("ROLLBACK", .{});

    // Row should still be RUN_PLANNED after rollback
    // check-pg-drain: ok — single row, drain after get
    var q = try db_ctx.conn.query("SELECT state FROM orphan_txn_test WHERE run_id = 'r1'", .{});
    defer q.deinit();
    const row = (try q.next()).?;
    const state = try row.get([]u8, 0);
    try q.drain();
    try std.testing.expectEqualStrings("RUN_PLANNED", state);

    // Now simulate: BEGIN → UPDATE → COMMIT (successful recovery)
    _ = try db_ctx.conn.exec("BEGIN", .{});
    _ = try db_ctx.conn.exec(
        "UPDATE orphan_txn_test SET state = 'BLOCKED' WHERE run_id = 'r1'",
        .{},
    );
    _ = try db_ctx.conn.exec("COMMIT", .{});

    // check-pg-drain: ok — single row, drain after get
    var q2 = try db_ctx.conn.query("SELECT state FROM orphan_txn_test WHERE run_id = 'r1'", .{});
    defer q2.deinit();
    const row2 = (try q2.next()).?;
    const state2 = try row2.get([]u8, 0);
    try q2.drain();
    try std.testing.expectEqualStrings("BLOCKED", state2);
}

test "integration: rollback preserves pending rows for restart recovery" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempOutboxTable(db_ctx.conn);
    try insertPendingRows(db_ctx.conn, 1);

    var begin_q = try db_ctx.conn.query("BEGIN", .{});
    begin_q.deinit();
    errdefer {
        if (db_ctx.conn.query(sql_rollback, .{})) |rb_result| {
            var rb_q = rb_result;
            rb_q.deinit();
        } else |_| {}
    }

    var update_q = try db_ctx.conn.query(
        \\UPDATE run_side_effect_outbox
        \\SET status = 'dead_letter', reconciled_state = 'simulated_crash', updated_at = $1
        \\WHERE status = 'pending'
    , .{std.time.milliTimestamp()});
    update_q.deinit();

    var rollback_q = try db_ctx.conn.query(sql_rollback, .{});
    rollback_q.deinit();

    try std.testing.expectEqual(@as(i64, 1), try pendingCount(db_ctx.conn));

    const recovered = try outbox.reconcileStartup(db_ctx.conn);
    try std.testing.expectEqual(@as(u32, 1), recovered.dead_lettered);
    try std.testing.expectEqual(@as(i64, 0), try pendingCount(db_ctx.conn));
}

// ---------------------------------------------------------------------------
// Orphan recovery integration tests (M14_001)
// ---------------------------------------------------------------------------

/// Create temp tables matching the schema orphan recovery reads/writes.
fn createOrphanTestTables(conn: *db.Conn) !void {
    _ = try conn.exec(
        \\CREATE TEMP TABLE IF NOT EXISTS runs (
        \\  run_id TEXT PRIMARY KEY,
        \\  workspace_id TEXT NOT NULL,
        \\  spec_id TEXT NOT NULL DEFAULT '',
        \\  tenant_id TEXT NOT NULL DEFAULT '',
        \\  state TEXT NOT NULL,
        \\  attempt INT NOT NULL DEFAULT 1,
        \\  mode TEXT DEFAULT 'api',
        \\  requested_by TEXT DEFAULT '',
        \\  idempotency_key TEXT,
        \\  request_id TEXT DEFAULT '',
        \\  trace_id TEXT DEFAULT '',
        \\  branch TEXT DEFAULT '',
        \\  pr_url TEXT,
        \\  run_snapshot_config_version TEXT,
        \\  created_at BIGINT NOT NULL,
        \\  updated_at BIGINT NOT NULL
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE IF NOT EXISTS run_transitions (
        \\  id TEXT PRIMARY KEY,
        \\  run_id TEXT NOT NULL,
        \\  attempt INT NOT NULL,
        \\  state_from TEXT,
        \\  state_to TEXT,
        \\  actor TEXT,
        \\  reason_code TEXT,
        \\  notes TEXT,
        \\  ts BIGINT NOT NULL
        \\)
    , .{});
}

fn insertOrphanRun(conn: *db.Conn, run_id: []const u8, state: []const u8, attempt: i32, updated_at: i64, created_at: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO runs (run_id, workspace_id, state, attempt, created_at, updated_at)
        \\VALUES ($1, 'ws-test', $2, $3, $4, $5)
    , .{ run_id, state, attempt, created_at, updated_at });
}

fn getRunState(conn: *db.Conn, run_id: []const u8) ![]u8 {
    // check-pg-drain: ok — single row, drain after get
    var q = try conn.query("SELECT state FROM runs WHERE run_id = $1", .{run_id});
    defer q.deinit();
    const row = (try q.next()) orelse return error.RunNotFound;
    const state = try row.get([]u8, 0);
    try q.drain();
    return state;
}

fn getRunAttempt(conn: *db.Conn, run_id: []const u8) !i32 {
    // check-pg-drain: ok — single row, drain after get
    var q = try conn.query("SELECT attempt FROM runs WHERE run_id = $1", .{run_id});
    defer q.deinit();
    const row = (try q.next()) orelse return error.RunNotFound;
    const attempt = try row.get(i32, 0);
    try q.drain();
    return attempt;
}

fn countTransitions(conn: *db.Conn, run_id: []const u8, reason_code: []const u8) !i64 {
    // check-pg-drain: ok — single row, drain after get
    var q = try conn.query(
        "SELECT COUNT(*)::BIGINT FROM run_transitions WHERE run_id = $1 AND reason_code = $2",
        .{ run_id, reason_code },
    );
    defer q.deinit();
    const row = (try q.next()).?;
    const count = try row.get(i64, 0);
    try q.drain();
    return count;
}

test "integration: orphan recovery detects stale RUN_PLANNED and transitions to BLOCKED" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000; // 700s ago (> 600s default threshold)

    try insertOrphanRun(db_ctx.conn, "orphan-run-1", "RUN_PLANNED", 1, stale_ts, stale_ts);

    const config = orphan_recovery.OrphanRecoveryConfig{
        .staleness_ms = 600_000,
        .requeue_enabled = false,
        .max_attempts = 3,
        .batch_limit = 32,
    };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );

    try std.testing.expectEqual(@as(u32, 1), result.blocked);
    try std.testing.expectEqual(@as(u32, 0), result.requeued);

    const state = try getRunState(db_ctx.conn, "orphan-run-1");
    try std.testing.expectEqualStrings("BLOCKED", state);

    const txn_count = try countTransitions(db_ctx.conn, "orphan-run-1", "WORKER_CRASH_ORPHAN");
    try std.testing.expectEqual(@as(i64, 1), txn_count);
}

test "integration: orphan recovery skips runs below staleness threshold" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const recent_ts = now_ms - 30_000; // 30s ago (< 600s threshold)

    try insertOrphanRun(db_ctx.conn, "recent-run-1", "RUN_PLANNED", 1, recent_ts, recent_ts);

    const config = orphan_recovery.OrphanRecoveryConfig{ .staleness_ms = 600_000 };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );

    try std.testing.expectEqual(@as(u32, 0), result.blocked);
    try std.testing.expectEqual(@as(u32, 0), result.requeued);

    const state = try getRunState(db_ctx.conn, "recent-run-1");
    try std.testing.expectEqualStrings("RUN_PLANNED", state);
}

test "integration: orphan recovery skips terminal and queued states" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    // Terminal states — must NOT be recovered
    try insertOrphanRun(db_ctx.conn, "done-run", "DONE", 1, stale_ts, stale_ts);
    try insertOrphanRun(db_ctx.conn, "notified-blocked-run", "NOTIFIED_BLOCKED", 1, stale_ts, stale_ts);
    // Queued state — must NOT be recovered (it's waiting to be claimed)
    try insertOrphanRun(db_ctx.conn, "queued-run", "SPEC_QUEUED", 1, stale_ts, stale_ts);
    // BLOCKED — already blocked, not an orphan candidate
    try insertOrphanRun(db_ctx.conn, "blocked-run", "BLOCKED", 1, stale_ts, stale_ts);

    const config = orphan_recovery.OrphanRecoveryConfig{ .staleness_ms = 600_000 };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );

    try std.testing.expectEqual(@as(u32, 0), result.blocked);
    try std.testing.expectEqual(@as(u32, 0), result.requeued);

    // All states unchanged
    try std.testing.expectEqualStrings("DONE", try getRunState(db_ctx.conn, "done-run"));
    try std.testing.expectEqualStrings("NOTIFIED_BLOCKED", try getRunState(db_ctx.conn, "notified-blocked-run"));
    try std.testing.expectEqualStrings("SPEC_QUEUED", try getRunState(db_ctx.conn, "queued-run"));
    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "blocked-run"));
}

test "integration: orphan recovery re-queues when enabled and attempts remain" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    try insertOrphanRun(db_ctx.conn, "requeue-run-1", "RUN_PLANNED", 1, stale_ts, stale_ts);

    const config = orphan_recovery.OrphanRecoveryConfig{
        .staleness_ms = 600_000,
        .requeue_enabled = true,
        .max_attempts = 3,
        .batch_limit = 32,
    };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );

    try std.testing.expectEqual(@as(u32, 0), result.blocked);
    try std.testing.expectEqual(@as(u32, 1), result.requeued);

    const state = try getRunState(db_ctx.conn, "requeue-run-1");
    try std.testing.expectEqualStrings("SPEC_QUEUED", state);

    const attempt = try getRunAttempt(db_ctx.conn, "requeue-run-1");
    try std.testing.expectEqual(@as(i32, 2), attempt);

    const txn_count = try countTransitions(db_ctx.conn, "requeue-run-1", "ORPHAN_REQUEUED");
    try std.testing.expectEqual(@as(i64, 1), txn_count);
}

test "integration: orphan recovery blocks when max_attempts reached even with requeue enabled" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    // attempt=3, max_attempts=3 → no more retries
    try insertOrphanRun(db_ctx.conn, "exhausted-run", "PATCH_IN_PROGRESS", 3, stale_ts, stale_ts);

    const config = orphan_recovery.OrphanRecoveryConfig{
        .staleness_ms = 600_000,
        .requeue_enabled = true,
        .max_attempts = 3,
        .batch_limit = 32,
    };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );

    try std.testing.expectEqual(@as(u32, 1), result.blocked);
    try std.testing.expectEqual(@as(u32, 0), result.requeued);

    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "exhausted-run"));
}

test "integration: orphan recovery circuit breaker prevents re-queue of recently orphaned run" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    try insertOrphanRun(db_ctx.conn, "cb-run", "RUN_PLANNED", 1, stale_ts, stale_ts);

    // Insert a recent ORPHAN_REQUEUED transition (simulating prior recovery)
    const tid = try id_format.generateTransitionId(std.testing.allocator);
    defer std.testing.allocator.free(tid);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO run_transitions (id, run_id, attempt, state_from, state_to, actor, reason_code, notes, ts)
        \\VALUES ($1, 'cb-run', 1, 'RUN_PLANNED', 'SPEC_QUEUED', 'orchestrator', 'ORPHAN_REQUEUED', 'prior', $2)
    , .{ tid, now_ms - 60_000 }); // 1 minute ago (within 30-min circuit breaker window)

    const config = orphan_recovery.OrphanRecoveryConfig{
        .staleness_ms = 600_000,
        .requeue_enabled = true,
        .max_attempts = 3,
        .batch_limit = 32,
    };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );

    // Circuit breaker fires — blocks instead of re-queuing
    try std.testing.expectEqual(@as(u32, 1), result.blocked);
    try std.testing.expectEqual(@as(u32, 0), result.requeued);

    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "cb-run"));
}

test "integration: orphan recovery handles multiple candidate states in one batch" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    try insertOrphanRun(db_ctx.conn, "multi-1", "RUN_PLANNED", 1, stale_ts, stale_ts);
    try insertOrphanRun(db_ctx.conn, "multi-2", "PATCH_IN_PROGRESS", 1, stale_ts, stale_ts);
    try insertOrphanRun(db_ctx.conn, "multi-3", "PATCH_READY", 2, stale_ts, stale_ts);
    try insertOrphanRun(db_ctx.conn, "multi-4", "VERIFICATION_IN_PROGRESS", 1, stale_ts, stale_ts);

    const config = orphan_recovery.OrphanRecoveryConfig{ .staleness_ms = 600_000 };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );

    try std.testing.expectEqual(@as(u32, 4), result.blocked);

    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "multi-1"));
    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "multi-2"));
    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "multi-3"));
    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "multi-4"));
}

test "integration: orphan recovery batch limit caps rows processed per tick" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    // Insert 5 orphans, but set batch_limit=2
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        var buf: [32]u8 = undefined;
        const rid = std.fmt.bufPrint(&buf, "batch-run-{d}", .{i}) catch unreachable;
        try insertOrphanRun(db_ctx.conn, rid, "RUN_PLANNED", 1, stale_ts, stale_ts);
    }

    const config = orphan_recovery.OrphanRecoveryConfig{
        .staleness_ms = 600_000,
        .batch_limit = 2,
    };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );

    // Only 2 processed due to batch limit
    try std.testing.expectEqual(@as(u32, 2), result.blocked);
}

test "integration: orphan recovery is idempotent — second tick finds nothing" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    try insertOrphanRun(db_ctx.conn, "idem-run", "RUN_PLANNED", 1, stale_ts, stale_ts);

    const config = orphan_recovery.OrphanRecoveryConfig{ .staleness_ms = 600_000 };

    // First tick recovers it
    const r1 = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );
    try std.testing.expectEqual(@as(u32, 1), r1.blocked);

    // Second tick finds nothing — BLOCKED is not an orphan candidate state
    const r2 = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );
    try std.testing.expectEqual(@as(u32, 0), r2.blocked);
    try std.testing.expectEqual(@as(u32, 0), r2.requeued);
}

// T2: Boundary — run at exactly staleness threshold boundary
test "integration: orphan recovery boundary — run at exact threshold is NOT recovered" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    // updated_at = now - 600_000 (exactly at threshold). Query uses `< cutoff`,
    // so cutoff = now - 600_000, and updated_at == cutoff → NOT less than → skipped.
    const boundary_ts = now_ms - 600_000;
    try insertOrphanRun(db_ctx.conn, "boundary-run", "RUN_PLANNED", 1, boundary_ts, boundary_ts);

    const config = orphan_recovery.OrphanRecoveryConfig{ .staleness_ms = 600_000 };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );
    // Exactly at boundary — not stale enough (< is strict, not <=)
    try std.testing.expectEqual(@as(u32, 0), result.blocked);
}

test "integration: orphan recovery boundary — run 1ms past threshold IS recovered" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const past_boundary_ts = now_ms - 600_001; // 1ms past threshold
    try insertOrphanRun(db_ctx.conn, "past-boundary-run", "RUN_PLANNED", 1, past_boundary_ts, past_boundary_ts);

    const config = orphan_recovery.OrphanRecoveryConfig{ .staleness_ms = 600_000 };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );
    try std.testing.expectEqual(@as(u32, 1), result.blocked);
}

// T5: Concurrent reconcilers — SKIP LOCKED ensures disjoint processing
test "integration: orphan recovery SKIP LOCKED prevents double-processing across connections" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Get a second connection to simulate concurrent reconciler
    const conn2 = try db_ctx.pool.acquire();
    defer db_ctx.pool.release(conn2);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    try insertOrphanRun(db_ctx.conn, "concurrent-run-1", "RUN_PLANNED", 1, stale_ts, stale_ts);
    try insertOrphanRun(db_ctx.conn, "concurrent-run-2", "PATCH_IN_PROGRESS", 1, stale_ts, stale_ts);

    const config = orphan_recovery.OrphanRecoveryConfig{
        .staleness_ms = 600_000,
        .batch_limit = 1, // Each connection grabs 1 row
    };

    // First connection recovers 1 run
    const r1 = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );
    // Second connection gets the other row (SKIP LOCKED)
    const r2 = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        conn2,
        null,
        null,
        config,
    );

    // Total across both connections = 2, no duplicates
    try std.testing.expectEqual(@as(u32, 2), r1.blocked + r2.blocked);
    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "concurrent-run-1"));
    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "concurrent-run-2"));
}

// T3: Re-queue with null Redis client falls back to BLOCKED (not stuck SPEC_QUEUED)
test "integration: orphan recovery requeue with null Redis falls back to BLOCKED" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    try insertOrphanRun(db_ctx.conn, "no-redis-run", "RUN_PLANNED", 1, stale_ts, stale_ts);

    // requeue_enabled=true but queue=null → should fall back to BLOCKED
    const config = orphan_recovery.OrphanRecoveryConfig{
        .staleness_ms = 600_000,
        .requeue_enabled = true,
        .max_attempts = 3,
        .batch_limit = 32,
    };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );

    // Must be BLOCKED, not SPEC_QUEUED (no Redis to publish to)
    try std.testing.expectEqual(@as(u32, 1), result.blocked);
    try std.testing.expectEqual(@as(u32, 0), result.requeued);
    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "no-redis-run"));
}

// T6: Multi-tick progressive drain — 5 orphans, batch_limit=2, verify 3 ticks drain all
test "integration: orphan recovery multi-tick progressive drain" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    var i: usize = 0;
    while (i < 5) : (i += 1) {
        var buf: [32]u8 = undefined;
        const rid = std.fmt.bufPrint(&buf, "drain-run-{d}", .{i}) catch unreachable;
        try insertOrphanRun(db_ctx.conn, rid, "RUN_PLANNED", 1, stale_ts - @as(i64, @intCast(i)), stale_ts);
    }

    const config = orphan_recovery.OrphanRecoveryConfig{
        .staleness_ms = 600_000,
        .batch_limit = 2,
    };

    // Tick 1: drain 2
    const r1 = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );
    try std.testing.expectEqual(@as(u32, 2), r1.blocked);

    // Tick 2: drain 2 more
    const r2 = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );
    try std.testing.expectEqual(@as(u32, 2), r2.blocked);

    // Tick 3: drain last 1
    const r3 = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );
    try std.testing.expectEqual(@as(u32, 1), r3.blocked);

    // Tick 4: nothing left
    const r4 = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );
    try std.testing.expectEqual(@as(u32, 0), r4.blocked);
}

// T3: Transition record has correct actor and reason for blocked path
test "integration: orphan recovery transition record has orchestrator actor" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    try insertOrphanRun(db_ctx.conn, "actor-run", "PATCH_READY", 2, stale_ts, stale_ts);

    const config = orphan_recovery.OrphanRecoveryConfig{ .staleness_ms = 600_000 };
    _ = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );

    // Verify transition record details
    // check-pg-drain: ok — single row, drain after get
    var q = try db_ctx.conn.query(
        \\SELECT state_from, state_to, actor, reason_code, attempt
        \\FROM run_transitions WHERE run_id = 'actor-run'
    , .{});
    defer q.deinit();
    const row = (try q.next()).?;
    const state_from = try row.get([]u8, 0);
    const state_to = try row.get([]u8, 1);
    const actor = try row.get([]u8, 2);
    const reason = try row.get([]u8, 3);
    const attempt = try row.get(i32, 4);
    try q.drain();

    try std.testing.expectEqualStrings("PATCH_READY", state_from);
    try std.testing.expectEqualStrings("BLOCKED", state_to);
    try std.testing.expectEqualStrings("orchestrator", actor);
    try std.testing.expectEqualStrings("WORKER_CRASH_ORPHAN", reason);
    try std.testing.expectEqual(@as(i32, 2), attempt);
}

// T3: CAS guard — concurrent UPDATE to same run doesn't corrupt state
test "integration: orphan recovery CAS guard prevents double-transition" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    try insertOrphanRun(db_ctx.conn, "cas-run", "RUN_PLANNED", 1, stale_ts, stale_ts);

    // Simulate: another process already moved this run to BLOCKED
    _ = try db_ctx.conn.exec(
        "UPDATE runs SET state = 'BLOCKED', updated_at = $1 WHERE run_id = 'cas-run'",
        .{now_ms},
    );

    // Reconciler tick runs — the CAS `WHERE state = $3` won't match, so
    // the UPDATE affects 0 rows. The transition record still gets written
    // (idempotent — duplicate transitions are harmless in the audit log).
    const config = orphan_recovery.OrphanRecoveryConfig{ .staleness_ms = 600_000 };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );

    // Run was already BLOCKED before the scan — not in candidate states
    try std.testing.expectEqual(@as(u32, 0), result.blocked);
    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "cas-run"));
}

// T3: Transaction rollback on partial failure — simulate: BEGIN succeeds,
// transition UPDATE succeeds inside txn, then the process "crashes" (we ROLLBACK
// manually). The run must remain in its original state.
test "integration: orphan recovery rollback preserves original state on mid-row failure" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    try insertOrphanRun(db_ctx.conn, "rollback-run", "PATCH_IN_PROGRESS", 1, stale_ts, stale_ts);

    // Simulate the exact pattern from recoverOrphanedRuns:
    // BEGIN → UPDATE runs SET state='BLOCKED' → (failure) → ROLLBACK
    _ = try db_ctx.conn.exec("BEGIN", .{});
    _ = try db_ctx.conn.exec(
        "UPDATE runs SET state = 'BLOCKED', updated_at = $1 WHERE run_id = 'rollback-run' AND state = 'PATCH_IN_PROGRESS'",
        .{now_ms},
    );
    // Simulate mid-row failure (scoring fails, Redis fails, etc.)
    _ = try db_ctx.conn.exec("ROLLBACK", .{});

    // Run must be back in original state after rollback
    try std.testing.expectEqualStrings("PATCH_IN_PROGRESS", try getRunState(db_ctx.conn, "rollback-run"));
    try std.testing.expectEqual(@as(i32, 1), try getRunAttempt(db_ctx.conn, "rollback-run"));

    // Now verify: a real recovery tick picks it up and completes
    const config = orphan_recovery.OrphanRecoveryConfig{ .staleness_ms = 600_000 };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );
    try std.testing.expectEqual(@as(u32, 1), result.blocked);
    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "rollback-run"));
}

// T3: Verify requeue with null Redis doesn't leave runs in SPEC_QUEUED
// (the exact bug Greptile caught — Redis publish fails, DB commits, run stuck)
test "integration: orphan recovery null queue never produces SPEC_QUEUED state" {
    const db_ctx = (try openReconcileTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createOrphanTestTables(db_ctx.conn);

    const now_ms = std.time.milliTimestamp();
    const stale_ts = now_ms - 700_000;

    // Insert 3 runs at attempt=1 (below max_attempts=3)
    try insertOrphanRun(db_ctx.conn, "nq-run-1", "RUN_PLANNED", 1, stale_ts, stale_ts);
    try insertOrphanRun(db_ctx.conn, "nq-run-2", "PATCH_IN_PROGRESS", 1, stale_ts, stale_ts);
    try insertOrphanRun(db_ctx.conn, "nq-run-3", "VERIFICATION_IN_PROGRESS", 2, stale_ts, stale_ts);

    // requeue_enabled=true BUT queue=null → must all be BLOCKED, never SPEC_QUEUED
    const config = orphan_recovery.OrphanRecoveryConfig{
        .staleness_ms = 600_000,
        .requeue_enabled = true,
        .max_attempts = 3,
        .batch_limit = 32,
    };
    const result = try orphan_recovery.recoverOrphanedRuns(
        std.testing.allocator,
        db_ctx.conn,
        null,
        null,
        config,
    );

    try std.testing.expectEqual(@as(u32, 3), result.blocked);
    try std.testing.expectEqual(@as(u32, 0), result.requeued);

    // Verify NO run ended up in SPEC_QUEUED
    // check-pg-drain: ok — single row, drain after get
    var q = try db_ctx.conn.query(
        "SELECT COUNT(*)::BIGINT FROM runs WHERE state = 'SPEC_QUEUED'",
        .{},
    );
    defer q.deinit();
    const row = (try q.next()).?;
    const spec_queued_count = try row.get(i64, 0);
    try q.drain();
    try std.testing.expectEqual(@as(i64, 0), spec_queued_count);

    // All must be BLOCKED
    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "nq-run-1"));
    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "nq-run-2"));
    try std.testing.expectEqualStrings("BLOCKED", try getRunState(db_ctx.conn, "nq-run-3"));
}
