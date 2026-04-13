//! Step 2 — Role isolation integration tests for M14_001.
//!
//! Verifies the two load-bearing invariants of the RULE CTX process boundary:
//!
//!   NEGATIVE: memory_runtime has zero access to core.* tables (SQLSTATE 42501).
//!   POSITIVE: memory_runtime can SELECT from memory.memory_entries.
//!
//! Requires: LIVE_DB=1 and TEST_DATABASE_URL=postgres://... (set by make test-integration-db).
//! The test user (usezombie) is a superuser in local dev, so SET ROLE succeeds.

const std = @import("std");
const pg = @import("pg");
const base = @import("../db/test_fixtures.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

// ─── negative: core.* is forbidden ──────────────────────────────────────────

test "integration: memory_runtime has no access to core.zombies (RULE CTX)" {
    if (!std.process.hasEnvVarConstant("LIVE_DB")) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const conn = db_ctx.conn;

    // Switch to memory_runtime. Succeeds because usezombie is a superuser.
    _ = try conn.exec("SET ROLE memory_runtime", .{});

    // SELECT on core.zombies must be rejected with SQLSTATE 42501 (insufficient_privilege).
    const result = conn.query("SELECT id FROM core.zombies LIMIT 1", .{});
    try std.testing.expectError(error.PG, result);

    const pg_err = conn.err orelse {
        return error.ExpectedPgError;
    };
    // SQLSTATE 42501 = insufficient_privilege (permission denied for table zombies).
    try std.testing.expectEqualStrings("42501", pg_err.code);
}

// ─── positive: memory.memory_entries is accessible ───────────────────────────

test "integration: memory_runtime can SELECT from memory.memory_entries (RULE CTX)" {
    if (!std.process.hasEnvVarConstant("LIVE_DB")) return error.SkipZigTest;

    const alloc = std.testing.allocator;
    // Use a fresh pool — the negative test leaves its connection in a pg-error state.
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const conn = db_ctx.conn;

    _ = try conn.exec("SET ROLE memory_runtime", .{});

    // SELECT must not error — empty result set is fine (the table may have no rows).
    var q = PgQuery.from(try conn.query(
        "SELECT id FROM memory.memory_entries LIMIT 1",
        .{},
    ));
    defer q.deinit(); // drains remaining rows, satisfies RULE FLS

    // Drain — zero rows is fine. The test asserts the query succeeds, not that rows exist.
    while (try q.next()) |_| {}
}
