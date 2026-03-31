const std = @import("std");
const worker = @import("../../../pipeline/worker.zig");
const common = @import("../common.zig");
const start_dedup = @import("start_dedup.zig");

test "integration: beginApiRequest enforces max in-flight limit" {
    var ws = worker.WorkerState.init();
    var ctx = common.Context{
        .pool = undefined,
        .queue = undefined,
        .alloc = std.testing.allocator,
        .api_keys = "",
        .oidc = null,
        .worker_state = &ws,
        .api_in_flight_requests = std.atomic.Value(u32).init(0),
        .api_max_in_flight_requests = 2,
        .ready_max_queue_depth = null,
        .ready_max_queue_age_ms = null,
    };

    try std.testing.expect(common.beginApiRequest(&ctx));
    try std.testing.expect(common.beginApiRequest(&ctx));
    try std.testing.expect(!common.beginApiRequest(&ctx));
    try std.testing.expectEqual(@as(u32, 2), ctx.api_in_flight_requests.load(.acquire));
}

test "integration: endApiRequest decrements in-flight counter deterministically" {
    var ws = worker.WorkerState.init();
    var ctx = common.Context{
        .pool = undefined,
        .queue = undefined,
        .alloc = std.testing.allocator,
        .api_keys = "",
        .oidc = null,
        .worker_state = &ws,
        .api_in_flight_requests = std.atomic.Value(u32).init(0),
        .api_max_in_flight_requests = 1,
        .ready_max_queue_depth = null,
        .ready_max_queue_age_ms = null,
    };

    try std.testing.expect(common.beginApiRequest(&ctx));
    common.endApiRequest(&ctx);
    try std.testing.expectEqual(@as(u32, 0), ctx.api_in_flight_requests.load(.acquire));
}

test "integration: start-run queue failure compensation removes only SPEC_QUEUED row" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE runs (
            \\  run_id TEXT PRIMARY KEY,
            \\  state TEXT NOT NULL,
            \\  updated_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }

    const now_ms = std.time.milliTimestamp();
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO runs (run_id, state, updated_at) VALUES ($1, 'SPEC_QUEUED', $2)",
            .{ "run-delete", now_ms },
        );
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO runs (run_id, state, updated_at) VALUES ($1, 'RUN_PLANNED', $2)",
            .{ "run-keep", now_ms },
        );
        q.deinit();
    }

    common.compensateStartRunQueueFailure(db_ctx.conn, "run-delete");
    common.compensateStartRunQueueFailure(db_ctx.conn, "run-keep");

    {
        var q = try db_ctx.conn.query("SELECT COUNT(*)::BIGINT FROM runs WHERE run_id = 'run-delete'", .{});
        defer q.deinit();
        const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqual(@as(i64, 0), row.get(i64, 0) catch -1);
    }
    {
        var q = try db_ctx.conn.query("SELECT COUNT(*)::BIGINT FROM runs WHERE run_id = 'run-keep'", .{});
        defer q.deinit();
        const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqual(@as(i64, 1), row.get(i64, 0) catch -1);
    }
}

test "integration: retry queue failure compensation restores state and removes retry transition" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE runs (
            \\  run_id TEXT PRIMARY KEY,
            \\  state TEXT NOT NULL,
            \\  updated_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE run_transitions (
            \\  run_id TEXT NOT NULL,
            \\  reason_code TEXT NOT NULL,
            \\  ts BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }

    const now_ms = std.time.milliTimestamp();
    const transition_ts = now_ms + 1;
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO runs (run_id, state, updated_at) VALUES ($1, 'SPEC_QUEUED', $2)",
            .{ "run-retry", now_ms },
        );
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO run_transitions (run_id, reason_code, ts) VALUES ($1, 'MANUAL_RETRY', $2)",
            .{ "run-retry", transition_ts },
        );
        q.deinit();
    }

    common.compensateRetryQueueFailure(db_ctx.conn, "run-retry", "RUN_FAILED", transition_ts);

    {
        var q = try db_ctx.conn.query("SELECT state FROM runs WHERE run_id = $1", .{"run-retry"});
        defer q.deinit();
        const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings("RUN_FAILED", row.get([]const u8, 0) catch "");
    }
    {
        var q = try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM run_transitions WHERE run_id = $1 AND reason_code = 'MANUAL_RETRY' AND ts = $2",
            .{ "run-retry", transition_ts },
        );
        defer q.deinit();
        const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqual(@as(i64, 0), row.get(i64, 0) catch -1);
    }
}

// ── §3.2 Dedup integration tests ─────────────────────────────────────────────

// 3.2.3: dedup_key column has unique index — direct DB insert of duplicate key fails.
test "3.2.3: integration: dedup_key unique index rejects duplicate non-null value" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Create a minimal temp runs table with dedup_key and unique partial index.
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE runs_dedup_test (
            \\  run_id TEXT PRIMARY KEY,
            \\  dedup_key TEXT
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "CREATE UNIQUE INDEX ON runs_dedup_test(dedup_key) WHERE dedup_key IS NOT NULL",
            .{},
        );
        q.deinit();
    }

    const alloc = std.testing.allocator;
    const dedup_key = try start_dedup.computeDedupKey(alloc, "spec", "ws-dedup-test", "sha1");
    defer alloc.free(dedup_key);

    // First insert succeeds.
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO runs_dedup_test (run_id, dedup_key) VALUES ($1, $2)",
            .{ "run-dedup-1", dedup_key },
        );
        q.deinit();
    }

    // Second insert with same dedup_key must fail (unique constraint).
    const second = db_ctx.conn.query(
        "INSERT INTO runs_dedup_test (run_id, dedup_key) VALUES ($1, $2)",
        .{ "run-dedup-2", dedup_key },
    );
    try std.testing.expect(if (second) |*q| blk: {
        q.deinit();
        break :blk false; // should not succeed
    } else |_| true); // expected: error from duplicate key
}

// 3.2.3 (null exemption): two NULL dedup_keys do not conflict.
test "3.2.3: integration: dedup_key NULL rows are exempt from unique constraint" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE runs_dedup_null (
            \\  run_id TEXT PRIMARY KEY,
            \\  dedup_key TEXT
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "CREATE UNIQUE INDEX ON runs_dedup_null(dedup_key) WHERE dedup_key IS NOT NULL",
            .{},
        );
        q.deinit();
    }

    // Two rows with NULL dedup_key must both succeed.
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO runs_dedup_null (run_id, dedup_key) VALUES ($1, NULL)",
            .{"run-null-1"},
        );
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO runs_dedup_null (run_id, dedup_key) VALUES ($1, NULL)",
            .{"run-null-2"},
        );
        q.deinit();
    }

    var q = try db_ctx.conn.query("SELECT COUNT(*)::BIGINT FROM runs_dedup_null", .{});
    defer q.deinit();
    const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(i64, 2), row.get(i64, 0) catch -1);
}

// 3.2.1: computeDedupKey is stable — same spec/workspace/sha always produces the same key.
// (Pure unit — no DB required; validates the dedup lookup precondition.)
test "3.2.1: unit: identical submissions produce the same dedup key" {
    const alloc = std.testing.allocator;
    const k1 = try start_dedup.computeDedupKey(alloc, "spec markdown", "ws-stable", "sha-abc");
    defer alloc.free(k1);
    const k2 = try start_dedup.computeDedupKey(alloc, "spec markdown", "ws-stable", "sha-abc");
    defer alloc.free(k2);
    try std.testing.expectEqualStrings(k1, k2);
}

// 3.2.2: different sha → different key → new run allowed (no false dedup).
test "3.2.2: unit: different base_commit_sha produces different key — no false dedup" {
    const alloc = std.testing.allocator;
    const k_old = try start_dedup.computeDedupKey(alloc, "spec markdown", "ws-1", "sha-old");
    defer alloc.free(k_old);
    const k_new = try start_dedup.computeDedupKey(alloc, "spec markdown", "ws-1", "sha-new");
    defer alloc.free(k_new);
    try std.testing.expect(!std.mem.eql(u8, k_old, k_new));
}
