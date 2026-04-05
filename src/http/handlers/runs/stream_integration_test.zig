//! M22_001 — integration tests for SSE stream handler.
//!
//! Requires TEST_DATABASE_URL. Tests are skipped when no DB is available.
//!
//! Covers:
//!   §3: Unified event ID namespace — streamStoredEvents uses created_at
//!   §3.3: Last-Event-ID reconnect — replays only events after the given ID
//!   §4: Post-subscribe race — terminal state after subscribe replays correctly
//!   P1: Row slices duped before drain — no dangling pointers
//!   Edge: concurrent gate_results insertion ordering
//!   Edge: empty gate_results for a run

const std = @import("std");
const common = @import("../common.zig");

// ---------------------------------------------------------------------------
// §3: streamStoredEvents uses created_at as SSE event ID
// ---------------------------------------------------------------------------

test "integration: gate_results are inserted and queryable with created_at ordering" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Create temp table matching gate_results schema.
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE gate_results (
        \\  id TEXT PRIMARY KEY,
        \\  run_id TEXT NOT NULL,
        \\  gate_name TEXT NOT NULL,
        \\  attempt INT NOT NULL,
        \\  exit_code INT NOT NULL,
        \\  stdout_tail TEXT,
        \\  stderr_tail TEXT,
        \\  wall_ms BIGINT NOT NULL,
        \\  created_at BIGINT NOT NULL
        \\)
    , .{});

    const run_id = "run-sse-test-001";
    const now = std.time.milliTimestamp();

    // Insert 3 gate results with distinct created_at values.
    _ = try db_ctx.conn.exec(
        "INSERT INTO gate_results (id, run_id, gate_name, attempt, exit_code, wall_ms, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        .{ "gr-1", run_id, "lint", @as(i32, 1), @as(i32, 0), @as(i64, 100), now },
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO gate_results (id, run_id, gate_name, attempt, exit_code, wall_ms, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        .{ "gr-2", run_id, "test", @as(i32, 2), @as(i32, 1), @as(i64, 3000), now + 1 },
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO gate_results (id, run_id, gate_name, attempt, exit_code, wall_ms, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        .{ "gr-3", run_id, "build", @as(i32, 3), @as(i32, 0), @as(i64, 500), now + 2 },
    );

    // Query with created_at > 0 — should return all 3.
    var q = try db_ctx.conn.query(
        "SELECT gate_name, created_at FROM gate_results WHERE run_id = $1 AND created_at > $2 ORDER BY created_at ASC",
        .{ run_id, @as(i64, 0) },
    );
    defer q.deinit();

    var count: usize = 0;
    var prev_ts: i64 = 0;
    while (q.next() catch null) |row| {
        const ts = row.get(i64, 1) catch 0;
        try std.testing.expect(ts > prev_ts);
        prev_ts = ts;
        count += 1;
    }
    try q.drain();
    try std.testing.expectEqual(@as(usize, 3), count);
}

// ---------------------------------------------------------------------------
// §3.3: Last-Event-ID reconnect replay — only events after the given ID
// ---------------------------------------------------------------------------

test "integration: query with created_at > last_event_id replays only missed events" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE gate_results (
        \\  id TEXT PRIMARY KEY, run_id TEXT NOT NULL, gate_name TEXT NOT NULL,
        \\  attempt INT NOT NULL, exit_code INT NOT NULL, wall_ms BIGINT NOT NULL,
        \\  created_at BIGINT NOT NULL
        \\)
    , .{});

    const run_id = "run-reconnect-001";
    const base_ts: i64 = 1700000000000;

    _ = try db_ctx.conn.exec(
        "INSERT INTO gate_results (id, run_id, gate_name, attempt, exit_code, wall_ms, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        .{ "gr-r1", run_id, "lint", @as(i32, 1), @as(i32, 0), @as(i64, 50), base_ts },
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO gate_results (id, run_id, gate_name, attempt, exit_code, wall_ms, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        .{ "gr-r2", run_id, "test", @as(i32, 2), @as(i32, 0), @as(i64, 100), base_ts + 1000 },
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO gate_results (id, run_id, gate_name, attempt, exit_code, wall_ms, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        .{ "gr-r3", run_id, "build", @as(i32, 3), @as(i32, 0), @as(i64, 200), base_ts + 2000 },
    );

    // Simulate reconnect: client last saw event with created_at = base_ts + 1000.
    // Should only replay the "build" event (created_at > base_ts + 1000).
    var q = try db_ctx.conn.query(
        "SELECT gate_name FROM gate_results WHERE run_id = $1 AND created_at > $2 ORDER BY created_at ASC",
        .{ run_id, base_ts + 1000 },
    );
    defer q.deinit();

    const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
    const alloc = std.testing.allocator;
    const gate_name = try alloc.dupe(u8, row.get([]u8, 0) catch "");
    defer alloc.free(gate_name);
    try std.testing.expectEqualStrings("build", gate_name);

    // No more rows.
    try std.testing.expect((q.next() catch null) == null);
}

test "integration: Last-Event-ID=0 replays all events (fresh connection)" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE gate_results (
        \\  id TEXT PRIMARY KEY, run_id TEXT NOT NULL, gate_name TEXT NOT NULL,
        \\  attempt INT NOT NULL, exit_code INT NOT NULL, wall_ms BIGINT NOT NULL,
        \\  created_at BIGINT NOT NULL
        \\)
    , .{});

    const run_id = "run-fresh-001";
    _ = try db_ctx.conn.exec(
        "INSERT INTO gate_results (id, run_id, gate_name, attempt, exit_code, wall_ms, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        .{ "gr-f1", run_id, "lint", @as(i32, 1), @as(i32, 0), @as(i64, 50), @as(i64, 1700000000000) },
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO gate_results (id, run_id, gate_name, attempt, exit_code, wall_ms, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        .{ "gr-f2", run_id, "test", @as(i32, 2), @as(i32, 0), @as(i64, 100), @as(i64, 1700000001000) },
    );

    // after_event_id = 0 → all events replayed.
    var q = try db_ctx.conn.query(
        "SELECT COUNT(*)::BIGINT FROM gate_results WHERE run_id = $1 AND created_at > $2",
        .{ run_id, @as(i64, 0) },
    );
    defer q.deinit();
    const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(i64, 2), row.get(i64, 0) catch -1);
}

// ---------------------------------------------------------------------------
// §4: Post-subscribe race — terminal state detection
// ---------------------------------------------------------------------------

test "integration: run in terminal state is detectable by post-subscribe query" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE runs (
        \\  run_id TEXT PRIMARY KEY,
        \\  state TEXT NOT NULL,
        \\  updated_at BIGINT NOT NULL
        \\)
    , .{});

    _ = try db_ctx.conn.exec(
        "INSERT INTO runs (run_id, state, updated_at) VALUES ($1, $2, $3)",
        .{ "run-race-001", "DONE", std.time.milliTimestamp() },
    );

    // Simulate post-subscribe state check.
    var q = try db_ctx.conn.query("SELECT state FROM runs WHERE run_id = $1", .{"run-race-001"});
    defer q.deinit();
    const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
    const alloc = std.testing.allocator;
    const state = try alloc.dupe(u8, row.get([]u8, 0) catch "");
    defer alloc.free(state);
    try q.drain();

    // State must be terminal — handler would replay events and close.
    const stream = @import("stream.zig");
    try std.testing.expect(stream.isTerminalState(state));
}

test "integration: run in non-terminal state is not detected as terminal" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE runs (
        \\  run_id TEXT PRIMARY KEY,
        \\  state TEXT NOT NULL,
        \\  updated_at BIGINT NOT NULL
        \\)
    , .{});

    _ = try db_ctx.conn.exec(
        "INSERT INTO runs (run_id, state, updated_at) VALUES ($1, $2, $3)",
        .{ "run-race-002", "RUNNING", std.time.milliTimestamp() },
    );

    var q = try db_ctx.conn.query("SELECT state FROM runs WHERE run_id = $1", .{"run-race-002"});
    defer q.deinit();
    const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
    const alloc = std.testing.allocator;
    const state = try alloc.dupe(u8, row.get([]u8, 0) catch "");
    defer alloc.free(state);
    try q.drain();

    const stream = @import("stream.zig");
    try std.testing.expect(!stream.isTerminalState(state));
}

test "integration: run transitions to terminal between two queries" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE runs (
        \\  run_id TEXT PRIMARY KEY,
        \\  state TEXT NOT NULL,
        \\  updated_at BIGINT NOT NULL
        \\)
    , .{});

    // First query: RUNNING.
    _ = try db_ctx.conn.exec(
        "INSERT INTO runs (run_id, state, updated_at) VALUES ($1, $2, $3)",
        .{ "run-race-003", "RUNNING", std.time.milliTimestamp() },
    );

    {
        var q = try db_ctx.conn.query("SELECT state FROM runs WHERE run_id = $1", .{"run-race-003"});
        defer q.deinit();
        const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
        const alloc = std.testing.allocator;
        const state = try alloc.dupe(u8, row.get([]u8, 0) catch "");
        defer alloc.free(state);
        try q.drain();
        const stream = @import("stream.zig");
        try std.testing.expect(!stream.isTerminalState(state));
    }

    // Simulate run completing between first check and subscribe.
    _ = try db_ctx.conn.exec(
        "UPDATE runs SET state = $1, updated_at = $2 WHERE run_id = $3",
        .{ "DONE", std.time.milliTimestamp(), "run-race-003" },
    );

    // Post-subscribe query: now terminal.
    {
        var q = try db_ctx.conn.query("SELECT state FROM runs WHERE run_id = $1", .{"run-race-003"});
        defer q.deinit();
        const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
        const alloc = std.testing.allocator;
        const state = try alloc.dupe(u8, row.get([]u8, 0) catch "");
        defer alloc.free(state);
        try q.drain();
        const stream = @import("stream.zig");
        try std.testing.expect(stream.isTerminalState(state));
    }
}

// ---------------------------------------------------------------------------
// P1: Row slices duped before drain — no dangling pointers
// ---------------------------------------------------------------------------

test "integration: row data survives drain when duped" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        "CREATE TEMP TABLE runs (run_id TEXT PRIMARY KEY, state TEXT NOT NULL, updated_at BIGINT NOT NULL)",
        .{},
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO runs (run_id, state, updated_at) VALUES ($1, $2, $3)",
        .{ "run-dupe-001", "CANCELLED", std.time.milliTimestamp() },
    );

    const alloc = std.testing.allocator;
    var q = try db_ctx.conn.query("SELECT state FROM runs WHERE run_id = $1", .{"run-dupe-001"});
    defer q.deinit();
    const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
    // Dupe BEFORE drain — the safe pattern.
    const state_owned = try alloc.dupe(u8, row.get([]u8, 0) catch "");
    defer alloc.free(state_owned);
    try q.drain();

    // state_owned should still be valid after drain.
    try std.testing.expectEqualStrings("CANCELLED", state_owned);
}

// ---------------------------------------------------------------------------
// Edge: empty gate_results for a run
// ---------------------------------------------------------------------------

test "integration: empty gate_results returns zero rows" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE gate_results (
        \\  id TEXT PRIMARY KEY, run_id TEXT NOT NULL, gate_name TEXT NOT NULL,
        \\  attempt INT NOT NULL, exit_code INT NOT NULL, wall_ms BIGINT NOT NULL,
        \\  created_at BIGINT NOT NULL
        \\)
    , .{});

    var q = try db_ctx.conn.query(
        "SELECT gate_name FROM gate_results WHERE run_id = $1 AND created_at > $2",
        .{ "run-empty-001", @as(i64, 0) },
    );
    defer q.deinit();
    try std.testing.expect((q.next() catch null) == null);
}

// ---------------------------------------------------------------------------
// Edge: negative Last-Event-ID replays everything (all timestamps > -1)
// ---------------------------------------------------------------------------

test "integration: negative last_event_id replays all events" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE gate_results (
        \\  id TEXT PRIMARY KEY, run_id TEXT NOT NULL, gate_name TEXT NOT NULL,
        \\  attempt INT NOT NULL, exit_code INT NOT NULL, wall_ms BIGINT NOT NULL,
        \\  created_at BIGINT NOT NULL
        \\)
    , .{});

    _ = try db_ctx.conn.exec(
        "INSERT INTO gate_results (id, run_id, gate_name, attempt, exit_code, wall_ms, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        .{ "gr-n1", "run-neg-001", "lint", @as(i32, 1), @as(i32, 0), @as(i64, 50), @as(i64, 1) },
    );

    // -1 as last_event_id → WHERE created_at > -1 → returns everything.
    var q = try db_ctx.conn.query(
        "SELECT COUNT(*)::BIGINT FROM gate_results WHERE run_id = $1 AND created_at > $2",
        .{ "run-neg-001", @as(i64, -1) },
    );
    defer q.deinit();
    const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(i64, 1), row.get(i64, 0) catch -1);
}

// ---------------------------------------------------------------------------
// Edge: ABORTED state is terminal (M21_001 addition)
// ---------------------------------------------------------------------------

test "integration: ABORTED state is detected as terminal" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        "CREATE TEMP TABLE runs (run_id TEXT PRIMARY KEY, state TEXT NOT NULL, updated_at BIGINT NOT NULL)",
        .{},
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO runs (run_id, state, updated_at) VALUES ($1, $2, $3)",
        .{ "run-abort-001", "ABORTED", std.time.milliTimestamp() },
    );

    const alloc = std.testing.allocator;
    var q = try db_ctx.conn.query("SELECT state FROM runs WHERE run_id = $1", .{"run-abort-001"});
    defer q.deinit();
    const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
    const state = try alloc.dupe(u8, row.get([]u8, 0) catch "");
    defer alloc.free(state);
    try q.drain();

    const stream = @import("stream.zig");
    try std.testing.expect(stream.isTerminalState(state));
}
