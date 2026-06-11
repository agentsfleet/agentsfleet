//! Integration test for account_teardown.zig — the Clerk `user.deleted`
//! hard-purge (`purgeByOidcSubject`).
//!
//! Skips when TEST_DATABASE_URL / DATABASE_URL is unset (the shared
//! `openTestConn` gate). The test conn is the DB superuser, so the purge's
//! cross-schema DELETEs and the memory seed/count run directly without SET ROLE.
//!
//! Regression target: `memory.memory_entries` carries no FK to `core.zombies`,
//! so a seeded memory row survives the workspace/zombie deletes and is removed
//! ONLY by the teardown's explicit `DELETE ... WHERE zombie_id IN (...)`. That
//! DELETE keys on `zombie_id` (UUID) after the `instance_id` column was dropped;
//! a stale-column DELETE would error the whole purge (returns error, not true).

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const base = @import("../db/test_fixtures.zig");
const teardown = @import("account_teardown.zig");

// Distinct `c...` suffixes so this fixture never collides with the signup /
// clerk integration tests (which use `b...` / fixed canonical IDs). The
// `0195b4ba-8d3a-7f13-8abc-` prefix keeps every id a valid UUIDv7.
const OIDC: []const u8 = "oidc-account-teardown-purge-01";
const TENANT_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000001";
const USER_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000002";
const WORKSPACE_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000003";
const ZOMBIE_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000004";

fn countMemory(conn: *pg.Conn, zombie_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::BIGINT FROM memory.memory_entries WHERE zombie_id = $1::uuid",
        .{zombie_id},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return 0;
    return row.get(i64, 0);
}

fn countUsers(conn: *pg.Conn, oidc_subject: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::BIGINT FROM core.users WHERE oidc_subject = $1",
        .{oidc_subject},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return 0;
    return row.get(i64, 0);
}

/// Best-effort teardown of the fixture. FK-safe order: memory (no FK) and
/// zombie first, then workspace, then membership/user, then tenant.
fn cleanup(conn: *pg.Conn) void {
    const stmts = [_][]const u8{
        "DELETE FROM memory.memory_entries WHERE zombie_id = $1::uuid",
        "DELETE FROM core.zombies WHERE id = $1::uuid",
    };
    inline for (stmts) |s| {
        _ = conn.exec(s, .{ZOMBIE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    }
    _ = conn.exec("DELETE FROM core.workspaces WHERE workspace_id = $1::uuid", .{WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.memberships WHERE user_id = $1::uuid", .{USER_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.users WHERE user_id = $1::uuid", .{USER_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.tenants WHERE tenant_id = $1::uuid", .{TENANT_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

// Rollback-test fixture — its OWN id family (`c...003x`) so it never collides
// with the happy-path victim above. The failure injection is the append-only
// trigger on `core.zombie_approval_gates`: any gate row makes the purge's gate
// DELETE raise mid-transaction. Because that trigger forbids DELETE outright,
// these rows are deliberately PERSISTENT (every seed is ON CONFLICT DO NOTHING
// and the test asserts state, not row creation); `_reset-test-db` is what
// clears them between suite runs.
const RB_OIDC: []const u8 = "oidc-account-teardown-rollback-01";
const RB_TENANT_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000031";
const RB_USER_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000032";
const RB_WORKSPACE_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000033";
const RB_ZOMBIE_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000034";
const RB_GATE_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000035";
const RB_MEMORY_UID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000036";

test "integration: a mid-purge failure rolls back — no partial deletes, conn stays healthy" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    // Idempotent seeds — reruns find the rows already present and assert the
    // same post-rollback state.
    _ = try conn.exec(
        \\INSERT INTO core.tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'teardown-rollback', 0, 0)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{RB_TENANT_ID});
    _ = try conn.exec(
        \\INSERT INTO core.users (user_id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'teardown-rollback@test.zombie', 0, 0)
        \\ON CONFLICT (user_id) DO NOTHING
    , .{ RB_USER_ID, RB_TENANT_ID, RB_OIDC });
    try base.seedWorkspaceWithTenant(conn, RB_WORKSPACE_ID, RB_TENANT_ID);
    try base.seedZombie(conn, RB_ZOMBIE_ID, RB_WORKSPACE_ID, "teardown-rollback", "{}", "# z");
    _ = try conn.exec(
        \\INSERT INTO core.zombie_approval_gates
        \\  (id, zombie_id, workspace_id, action_id, tool_name, action_name, gate_kind,
        \\   proposed_action, evidence, blast_radius, timeout_at, resolved_by, status,
        \\   detail, requested_at, created_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 'act-rollback-test', 'bash', 'rm',
        \\        'destructive_action', 'n/a', '{}'::jsonb, 'n/a', 9999999999999, '',
        \\        'pending', '', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RB_GATE_ID, RB_ZOMBIE_ID, RB_WORKSPACE_ID });
    // A memory row the purge deletes BEFORE it reaches the failing gate
    // statement — its survival after the error is the rollback proof.
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries (uid, id, key, content, category, zombie_id, session_id, created_at, updated_at)
        \\VALUES ($1::uuid, 'teardown-rollback-canary', 'canary', 'must survive the rollback', 'core', $2::uuid, NULL, '1700000000', '1700000000')
        \\ON CONFLICT (uid) DO NOTHING
    , .{ RB_MEMORY_UID, RB_ZOMBIE_ID });
    try std.testing.expectEqual(@as(i64, 1), try countMemory(conn, RB_ZOMBIE_ID));

    // The purge fails mid-transaction: its gate DELETE trips the append-only
    // trigger AFTER the telemetry + memory deletes already ran in the same
    // transaction. The errdefer must roll back on the FAIL-state-safe path.
    try std.testing.expectError(error.PG, teardown.purgeByOidcSubject(conn, std.testing.allocator, RB_OIDC));

    // No partial deletes: the memory row deleted before the failure is back,
    // and the user row was never reached.
    try std.testing.expectEqual(@as(i64, 1), try countMemory(conn, RB_ZOMBIE_ID));
    try std.testing.expectEqual(@as(i64, 1), try countUsers(conn, RB_OIDC));

    // Conn healthy: not stuck in an aborted transaction — with the old
    // exec("ROLLBACK") errdefer the driver short-circuits in FAIL state and
    // every later statement on this conn errors out.
    _ = try conn.exec("SELECT 1", .{});
}

test "integration: purgeByOidcSubject removes the account's memory entries" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    cleanup(conn); // start clean even if a prior run aborted mid-test
    defer cleanup(conn);

    // Seed a full account: tenant -> user -> workspace -> zombie.
    _ = try conn.exec(
        \\INSERT INTO core.tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'teardown-victim', 0, 0)
    , .{TENANT_ID});
    _ = try conn.exec(
        \\INSERT INTO core.users (user_id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'teardown@test.zombie', 0, 0)
    , .{ USER_ID, TENANT_ID, OIDC });
    try base.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    try base.seedZombie(conn, ZOMBIE_ID, WORKSPACE_ID, "teardown-victim", "{}", "# z");

    // Seed one memory row for the zombie. No FK to core.zombies, so only the
    // teardown's explicit DELETE removes it.
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries (uid, id, key, content, category, zombie_id, session_id, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-c00000000011'::uuid, $1, 'canary', 'should not survive teardown', 'core', $2::uuid, NULL, '1700000000', '1700000000')
    , .{ "account-teardown-canary", ZOMBIE_ID });
    try std.testing.expectEqual(@as(i64, 1), try countMemory(conn, ZOMBIE_ID));

    // Purge the account by its oidc_subject.
    const purged = try teardown.purgeByOidcSubject(conn, std.testing.allocator, OIDC);
    try std.testing.expect(purged);

    // The memory row is gone (regression target) and the cascade reached the
    // user — proving every statement before the user delete (incl. memory) ran.
    try std.testing.expectEqual(@as(i64, 0), try countMemory(conn, ZOMBIE_ID));
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, OIDC));
}
