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
const AGENTSFLEET_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000004";

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
        _ = conn.exec(s, .{AGENTSFLEET_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    }
    _ = conn.exec("DELETE FROM core.workspaces WHERE workspace_id = $1::uuid", .{WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.memberships WHERE user_id = $1::uuid", .{USER_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.users WHERE user_id = $1::uuid", .{USER_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.tenants WHERE tenant_id = $1::uuid", .{TENANT_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

// Rollback-test fixture — its OWN id family (`c...003x`) so it never collides
// with the happy-path victim above. The failure injection is a test-created
// BEFORE DELETE trigger on core.users that raises unconditionally — it fires
// AFTER the purge already deleted telemetry/memory/gates in the same
// transaction, which is what makes the restored rows the rollback proof. The
// injection is mechanism-agnostic: it does not depend on any gap in the purge
// order, so it survives future purge-order changes.
const RB_OIDC: []const u8 = "oidc-account-teardown-rollback-01";
const RB_TENANT_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000031";
const RB_USER_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000032";
const RB_WORKSPACE_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000033";
const RB_AGENTSFLEET_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000034";
const RB_GATE_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000035";
const RB_MEMORY_UID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000036";
const RB_RUNNER_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000037";
const RB_AFFINITY_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000038";
const RB_LEASE_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000039";
const RB_EVENT_ID: []const u8 = "evt-teardown-rollback-1";

fn seedRollbackAccount(conn: *pg.Conn) !void {
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
    try base.seedZombie(conn, RB_AGENTSFLEET_ID, RB_WORKSPACE_ID, "teardown-rollback", "{}", "# z");
    // An approval gate on the victim's own zombie — exercises the purge's
    // append-only bypass in both the rollback and the success tests.
    _ = try conn.exec(
        \\INSERT INTO core.zombie_approval_gates
        \\  (id, zombie_id, workspace_id, action_id, tool_name, action_name, gate_kind,
        \\   proposed_action, evidence, blast_radius, timeout_at, resolved_by, status,
        \\   detail, requested_at, created_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 'act-rollback-test', 'bash', 'rm',
        \\        'destructive_action', 'n/a', '{}'::jsonb, 'n/a', 9999999999999, '',
        \\        'pending', '', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RB_GATE_ID, RB_AGENTSFLEET_ID, RB_WORKSPACE_ID });
    // A memory row the purge deletes BEFORE it reaches the injected failure —
    // its survival after the error is the rollback proof.
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries (uid, id, key, content, category, zombie_id, created_at, updated_at)
        \\VALUES ($1::uuid, 'teardown-rollback-canary', 'canary', 'must survive the rollback', 'core', $2::uuid, 1700000000000, 1700000000000)
        \\ON CONFLICT (uid) DO NOTHING
    , .{ RB_MEMORY_UID, RB_AGENTSFLEET_ID });
    // Fleet rows: no FK into core, swept only by the purge's explicit fleet
    // DELETEs. The runner row is shared host infrastructure and must SURVIVE.
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'teardown-rb-host', 'teardown-rb-hash', 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{RB_RUNNER_ID});
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, zombie_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 1, 0, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (zombie_id) DO NOTHING
    , .{ RB_AFFINITY_ID, RB_AGENTSFLEET_ID, RB_RUNNER_ID });
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, zombie_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6,
        \\        'steer:test', 'chat', '{"message":"hi"}', 0, 'platform',
        \\        'test-provider', 'test-model', 0, 0, 0, 0, 1, 0, 'reported', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RB_LEASE_ID, RB_RUNNER_ID, RB_AGENTSFLEET_ID, RB_WORKSPACE_ID, RB_TENANT_ID, RB_EVENT_ID });
    _ = try conn.exec(
        \\INSERT INTO fleet.metering_periods
        \\  (uid, event_id, slice_seq, d_input_tokens, d_cached_tokens, d_output_tokens,
        \\   run_ms, run_fee_nanos, token_cost_nanos, charged_nanos, created_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-c0000000003a'::uuid, $1, 1, 0, 0, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (event_id, slice_seq) DO NOTHING
    , .{RB_EVENT_ID});
}

/// Full unwind: the production purge itself (gates included via its bypass),
/// then belt-and-braces sweeps for partially-seeded state.
fn cleanupRollbackAccount(conn: *pg.Conn) void {
    _ = teardown.purgeByOidcSubject(conn, std.testing.allocator, RB_OIDC) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
    execIgnoreTd(conn, "DELETE FROM memory.memory_entries WHERE zombie_id = $1::uuid", RB_AGENTSFLEET_ID);
    execIgnoreTd(conn, "DELETE FROM fleet.metering_periods WHERE event_id = $1", RB_EVENT_ID);
    execIgnoreTd(conn, "DELETE FROM fleet.runner_leases WHERE id = $1::uuid", RB_LEASE_ID);
    execIgnoreTd(conn, "DELETE FROM fleet.runner_affinity WHERE zombie_id = $1::uuid", RB_AGENTSFLEET_ID);
    execIgnoreTd(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", RB_RUNNER_ID);
    execIgnoreTd(conn, "DELETE FROM core.tenants WHERE tenant_id = $1::uuid", RB_TENANT_ID);
}

fn execIgnoreTd(conn: *pg.Conn, sql: []const u8, id: []const u8) void {
    _ = conn.exec(sql, .{id}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

fn dropUserDeleteInjection(conn: *pg.Conn) void {
    _ = conn.exec("DROP TRIGGER IF EXISTS trg_test_block_user_delete ON core.users", .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DROP FUNCTION IF EXISTS core.test_block_user_delete()", .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

test "integration: a mid-purge failure rolls back — no partial deletes, conn stays healthy" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    dropUserDeleteInjection(conn);
    cleanupRollbackAccount(conn);
    try seedRollbackAccount(conn);
    defer cleanupRollbackAccount(conn);
    try std.testing.expectEqual(@as(i64, 1), try countMemory(conn, RB_AGENTSFLEET_ID));

    // Deterministic mid-purge failure: every DELETE on core.users raises until
    // the trigger is dropped. The purge deletes telemetry, memory, and gates
    // BEFORE it reaches core.users, so those deletes are in-flight when the
    // statement fails and the errdefer must roll them back on the
    // FAIL-state-safe path.
    _ = try conn.exec("CREATE OR REPLACE FUNCTION core.test_block_user_delete() RETURNS trigger AS $$ BEGIN RAISE EXCEPTION 'injected test failure'; END; $$ LANGUAGE plpgsql", .{});
    _ = try conn.exec("CREATE TRIGGER trg_test_block_user_delete BEFORE DELETE ON core.users FOR EACH ROW EXECUTE FUNCTION core.test_block_user_delete()", .{});
    defer dropUserDeleteInjection(conn);

    try std.testing.expectError(error.PG, teardown.purgeByOidcSubject(conn, std.testing.allocator, RB_OIDC));

    // No partial deletes: the memory row deleted before the failure is back,
    // and the user row was never reached.
    try std.testing.expectEqual(@as(i64, 1), try countMemory(conn, RB_AGENTSFLEET_ID));
    try std.testing.expectEqual(@as(i64, 1), try countUsers(conn, RB_OIDC));

    // Conn healthy: not stuck in an aborted transaction — with the old
    // exec("ROLLBACK") errdefer the driver short-circuits in FAIL state and
    // every later statement on this conn errors out.
    _ = try conn.exec("SELECT 1", .{});
}

test "integration: purge succeeds for an account with approval gates (append-only bypass)" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    dropUserDeleteInjection(conn); // stale injection from an aborted prior run must not block this purge
    cleanupRollbackAccount(conn);
    try seedRollbackAccount(conn);
    defer cleanupRollbackAccount(conn);

    // The gate row would abort the workspace delete on its FK if the purge
    // could not remove it — the append-only trigger raises on DELETE unless
    // the purge transaction sets the bypass.
    const purged = try teardown.purgeByOidcSubject(conn, std.testing.allocator, RB_OIDC);
    try std.testing.expect(purged);

    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, RB_OIDC));
    try std.testing.expectEqual(@as(i64, 0), try countByUuid(conn, "SELECT COUNT(*)::BIGINT FROM core.zombie_approval_gates WHERE id = $1::uuid", RB_GATE_ID));
    // Fleet sweep: lease + affinity + metering rows are gone; the shared
    // runner row survives (host infrastructure, not tenant data).
    try std.testing.expectEqual(@as(i64, 0), try countByUuid(conn, "SELECT COUNT(*)::BIGINT FROM fleet.runner_leases WHERE id = $1::uuid", RB_LEASE_ID));
    try std.testing.expectEqual(@as(i64, 0), try countByUuid(conn, "SELECT COUNT(*)::BIGINT FROM fleet.runner_affinity WHERE zombie_id = $1::uuid", RB_AGENTSFLEET_ID));
    try std.testing.expectEqual(@as(i64, 0), try countByUuid(conn, "SELECT COUNT(*)::BIGINT FROM fleet.metering_periods WHERE event_id = $1", RB_EVENT_ID));
    try std.testing.expectEqual(@as(i64, 1), try countByUuid(conn, "SELECT COUNT(*)::BIGINT FROM fleet.runners WHERE id = $1::uuid", RB_RUNNER_ID));
}

fn countByUuid(conn: *pg.Conn, sql: []const u8, id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(sql, .{id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    return row.get(i64, 0);
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
    try base.seedZombie(conn, AGENTSFLEET_ID, WORKSPACE_ID, "teardown-victim", "{}", "# z");

    // Seed one memory row for the zombie. No FK to core.zombies, so only the
    // teardown's explicit DELETE removes it.
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries (uid, id, key, content, category, zombie_id, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-c00000000011'::uuid, $1, 'canary', 'should not survive teardown', 'core', $2::uuid, 1700000000000, 1700000000000)
    , .{ "account-teardown-canary", AGENTSFLEET_ID });
    try std.testing.expectEqual(@as(i64, 1), try countMemory(conn, AGENTSFLEET_ID));

    // Purge the account by its oidc_subject.
    const purged = try teardown.purgeByOidcSubject(conn, std.testing.allocator, OIDC);
    try std.testing.expect(purged);

    // The memory row is gone (regression target) and the cascade reached the
    // user — proving every statement before the user delete (incl. memory) ran.
    try std.testing.expectEqual(@as(i64, 0), try countMemory(conn, AGENTSFLEET_ID));
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, OIDC));
}
