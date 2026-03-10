const std = @import("std");
const worker = @import("../../../pipeline/worker.zig");
const common = @import("../common.zig");

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
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

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
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

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
