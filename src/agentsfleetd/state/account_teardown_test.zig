//! Integration test for account_teardown.zig — the Clerk `user.deleted`
//! hard-purge (`purgeByOidcSubject`).
//!
//! Skips when TEST_DATABASE_URL / DATABASE_URL is unset (the shared
//! `openTestConn` gate). The test conn is the DB superuser, so the purge's
//! cross-schema DELETEs and the memory seed/count run directly without SET ROLE.
//!
//! Regression target: `memory.memory_entries` carries no FK to `core.fleets`,
//! so a seeded memory row survives the workspace/fleet deletes and is removed
//! ONLY by the teardown's explicit `DELETE ... WHERE fleet_id IN (...)`. That
//! DELETE keys on `fleet_id` (UUID) after the `instance_id` column was dropped;
//! a stale-column DELETE would error the whole purge (returns error, not true).

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const base = @import("../db/test_fixtures.zig");
const teardown = @import("account_teardown.zig");
const store = @import("fleet_telemetry_store.zig");

// Distinct `c...` suffixes so this fixture never collides with the signup /
// clerk integration tests (which use `b...` / fixed canonical IDs). The
// `0195b4ba-8d3a-7f13-8abc-` prefix keeps every id a valid UUIDv7.
const OIDC: []const u8 = "oidc-account-teardown-purge-01";
const TENANT_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000001";
const USER_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000002";
const WORKSPACE_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000003";
const FLEET_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000004";
/// The fleet the caller never enumerated — a latecomer to the same tenant.
const RACE_FLEET_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000005";

fn countMemory(conn: *pg.Conn, fleet_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::BIGINT FROM memory.memory_entries WHERE fleet_id = $1::uuid",
        .{fleet_id},
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
/// fleet first, then workspace, then membership/user, then tenant.
fn cleanup(conn: *pg.Conn) void {
    const stmts = [_][]const u8{
        "DELETE FROM memory.memory_entries WHERE fleet_id = $1::uuid",
        "DELETE FROM core.fleets WHERE id = $1::uuid",
    };
    // Both fleets: a test that fails between seeding the latecomer and purging
    // it would otherwise strand a row the next run's workspace delete trips on.
    inline for (.{ FLEET_ID, RACE_FLEET_ID }) |fleet_id| {
        inline for (stmts) |s| {
            _ = conn.exec(s, .{fleet_id}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
        }
    }
    _ = conn.exec("DELETE FROM core.workspaces WHERE id = $1::uuid", .{WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.memberships WHERE user_id = $1::uuid", .{USER_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.users WHERE id = $1::uuid", .{USER_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.tenants WHERE id = $1::uuid", .{TENANT_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
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
const RB_FLEET_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000034";
const RB_GATE_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000035";
const RB_MEMORY_UID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000036";
const RB_RUNNER_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000037";
const RB_LEASE_ID: []const u8 = "0195b4ba-8d3a-7f13-8abc-c00000000039";
const RB_EVENT_ID: []const u8 = "evt-teardown-rollback-1";
const RB_MODEL: []const u8 = "teardown-rollback-model";

fn seedRollbackAccount(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO core.tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'teardown-rollback', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{RB_TENANT_ID});
    _ = try conn.exec(
        \\INSERT INTO core.users (id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'teardown-rollback@test.fleet', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RB_USER_ID, RB_TENANT_ID, RB_OIDC });
    try base.seedWorkspaceWithTenant(conn, RB_WORKSPACE_ID, RB_TENANT_ID);
    try base.seedFleet(conn, RB_FLEET_ID, RB_WORKSPACE_ID, "teardown-rollback", "{}", "# z");
    // An approval gate on the victim's own fleet — exercises the purge's
    // append-only bypass in both the rollback and the success tests.
    _ = try conn.exec(
        \\INSERT INTO core.fleet_approval_gates
        \\  (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_kind,
        \\   proposed_action, evidence, blast_radius, timeout_at, resolved_by, status,
        \\   detail, created_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 'act-rollback-test', 'bash', 'rm',
        \\        'destructive_action', 'n/a', '{}'::jsonb, 'n/a', 9999999999999, '',
        \\        'pending', '', 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RB_GATE_ID, RB_FLEET_ID, RB_WORKSPACE_ID });
    // A memory row the purge deletes BEFORE it reaches the injected failure —
    // its survival after the error is the rollback proof.
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries (id, key, content, category, fleet_id, created_at, updated_at)
        \\VALUES ($1::uuid, 'canary', 'must survive the rollback', 'core', $2::uuid, 1700000000000, 1700000000000)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RB_MEMORY_UID, RB_FLEET_ID });
    // The runner row is shared host infrastructure (tenant_id NULL, no per-account
    // FK) and must SURVIVE the purge — it is not swept by any fleet/tenant DELETE.
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'teardown-rb-host', 'teardown-rb-hash', 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{RB_RUNNER_ID});
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (fleet_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 1, 0, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (fleet_id) DO NOTHING
    , .{ RB_FLEET_ID, RB_RUNNER_ID });
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6,
        \\        'steer:test', 'chat', 0, 'platform',
        \\        'test-provider', 'test-model', 0, 0, 0, 0, 1, 0, 'reported', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RB_LEASE_ID, RB_RUNNER_ID, RB_FLEET_ID, RB_WORKSPACE_ID, RB_TENANT_ID, RB_EVENT_ID });
    // A charge for this account, written through the production writer. It
    // replaces a `fleet.metering_periods` slice seed: that table is gone, and
    // the ledger row that succeeded it is the stronger erasure subject anyway —
    // `tenant_id` is NOT NULL and cascades (schema/710), so Dimension 3.3's
    // "erasing an account leaves zero rows" is a claim about THIS table.
    try store.insertTelemetry(conn, std.testing.allocator, .{
        .tenant_id = RB_TENANT_ID,
        .workspace_id = RB_WORKSPACE_ID,
        .fleet_id = RB_FLEET_ID,
        .event_id = RB_EVENT_ID,
        .charge_type = .stage,
        .posture = .platform,
        .model = RB_MODEL,
        .credit_deducted_nanos = 0,
        .event_created_at = 0,
        .created_at = 0,
    });
}

/// Full unwind: the production purge itself (gates included via its bypass),
/// then belt-and-braces sweeps for partially-seeded state.
fn cleanupRollbackAccount(conn: *pg.Conn) void {
    _ = teardown.purgeByOidcSubject(conn, std.testing.allocator, RB_OIDC, &.{}) catch |err|
        std.log.warn("ignored: {s}", .{@errorName(err)});
    execIgnoreTd(conn, "DELETE FROM memory.memory_entries WHERE fleet_id = $1::uuid", RB_FLEET_ID);
    execIgnoreTd(conn, "DELETE FROM billing.usage_ledger WHERE event_id = $1", RB_EVENT_ID);
    execIgnoreTd(conn, "DELETE FROM fleet.runner_leases WHERE id = $1::uuid", RB_LEASE_ID);
    execIgnoreTd(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", RB_FLEET_ID);
    execIgnoreTd(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", RB_RUNNER_ID);
    execIgnoreTd(conn, "DELETE FROM core.tenants WHERE id = $1::uuid", RB_TENANT_ID);
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
    try std.testing.expectEqual(@as(i64, 1), try countMemory(conn, RB_FLEET_ID));

    // Deterministic mid-purge failure: every DELETE on core.users raises until
    // the trigger is dropped. The purge deletes telemetry, memory, and gates
    // BEFORE it reaches core.users, so those deletes are in-flight when the
    // statement fails and the errdefer must roll them back on the
    // FAIL-state-safe path.
    _ = try conn.exec("CREATE OR REPLACE FUNCTION core.test_block_user_delete() RETURNS trigger AS $$ BEGIN RAISE EXCEPTION 'injected test failure'; END; $$ LANGUAGE plpgsql", .{});
    _ = try conn.exec("CREATE TRIGGER trg_test_block_user_delete BEFORE DELETE ON core.users FOR EACH ROW EXECUTE FUNCTION core.test_block_user_delete()", .{});
    defer dropUserDeleteInjection(conn);

    try std.testing.expectError(error.PG, teardown.purgeByOidcSubject(conn, std.testing.allocator, RB_OIDC, &.{}));

    // No partial deletes: the memory row deleted before the failure is back,
    // and the user row was never reached.
    try std.testing.expectEqual(@as(i64, 1), try countMemory(conn, RB_FLEET_ID));
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
    const purged = try teardown.purgeByOidcSubject(conn, std.testing.allocator, RB_OIDC, &.{});
    try std.testing.expect(purged.purged);

    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, RB_OIDC));
    try std.testing.expectEqual(@as(i64, 0), try countByUuid(conn, "SELECT COUNT(*)::BIGINT FROM core.fleet_approval_gates WHERE id = $1::uuid", RB_GATE_ID));
    // Fleet sweep: lease + affinity + ledger rows are gone; the shared runner
    // row survives (host infrastructure, not tenant data). The ledger row goes
    // by the tenant cascade rather than by a hand-maintained delete order —
    // which is the whole point of typing `tenant_id` as a real reference.
    try std.testing.expectEqual(@as(i64, 0), try countByUuid(conn, "SELECT COUNT(*)::BIGINT FROM fleet.runner_leases WHERE id = $1::uuid", RB_LEASE_ID));
    try std.testing.expectEqual(@as(i64, 0), try countByUuid(conn, "SELECT COUNT(*)::BIGINT FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", RB_FLEET_ID));
    try std.testing.expectEqual(@as(i64, 0), try countByUuid(conn, "SELECT COUNT(*)::BIGINT FROM billing.usage_ledger WHERE event_id = $1", RB_EVENT_ID));
    try std.testing.expectEqual(@as(i64, 1), try countByUuid(conn, "SELECT COUNT(*)::BIGINT FROM fleet.runners WHERE id = $1::uuid", RB_RUNNER_ID));
}

fn countByUuid(conn: *pg.Conn, sql: []const u8, id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(sql, .{id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    return row.get(i64, 0);
}

test "integration: a fleet the caller never enumerated is reported, not absorbed" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    cleanup(conn);
    defer cleanup(conn);

    _ = try conn.exec(
        \\INSERT INTO core.tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'teardown-race', 0, 0)
    , .{TENANT_ID});
    _ = try conn.exec(
        \\INSERT INTO core.users (id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'teardown-race@test.fleet', 0, 0)
    , .{ USER_ID, TENANT_ID, OIDC });
    try base.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    // Two fleets, but the caller only ever saw one — the shape a fleet created
    // between the enumeration and the purge leaves behind. Its upstream timer
    // was never retired and the row that named it is about to be deleted.
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "teardown-race-known", "{}", "# z");
    try base.seedFleet(conn, RACE_FLEET_ID, WORKSPACE_ID, "teardown-race-latecomer", "{}", "# z");

    const enumerated = [_][]const u8{FLEET_ID};
    const result = try teardown.purgeByOidcSubject(conn, std.testing.allocator, OIDC, &enumerated);

    try std.testing.expect(result.purged);
    // Identity, not cardinality. The count of fleets at purge time (2) exceeds
    // the enumerated count (1) here, so a count comparison would also fire —
    // but it answers by naming which fleets went unhandled, which is what stays
    // correct when a create and a delete offset each other.
    try std.testing.expectEqual(@as(i64, 1), result.unenumerated_fleets);
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, OIDC));
}

test "integration: a fully enumerated tenant reports no unhandled fleets" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    cleanup(conn);
    defer cleanup(conn);

    _ = try conn.exec(
        \\INSERT INTO core.tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'teardown-clean', 0, 0)
    , .{TENANT_ID});
    _ = try conn.exec(
        \\INSERT INTO core.users (id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'teardown-clean@test.fleet', 0, 0)
    , .{ USER_ID, TENANT_ID, OIDC });
    try base.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "teardown-clean", "{}", "# z");

    // The ordinary path: everything the purge erases was handled upstream
    // first. Without this the race arm could fire on every deletion and the
    // signal would mean nothing.
    const enumerated = [_][]const u8{FLEET_ID};
    const result = try teardown.purgeByOidcSubject(conn, std.testing.allocator, OIDC, &enumerated);

    try std.testing.expect(result.purged);
    try std.testing.expectEqual(@as(i64, 0), result.unenumerated_fleets);
}

test "integration: purgeByOidcSubject removes the account's memory entries" {
    const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    cleanup(conn); // start clean even if a prior run aborted mid-test
    defer cleanup(conn);

    // Seed a full account: tenant -> user -> workspace -> fleet.
    _ = try conn.exec(
        \\INSERT INTO core.tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'teardown-victim', 0, 0)
    , .{TENANT_ID});
    _ = try conn.exec(
        \\INSERT INTO core.users (id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'teardown@test.fleet', 0, 0)
    , .{ USER_ID, TENANT_ID, OIDC });
    try base.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "teardown-victim", "{}", "# z");

    // Seed one memory row for the fleet. No FK to core.fleets, so only the
    // teardown's explicit DELETE removes it.
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries (id, key, content, category, fleet_id, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-c00000000011'::uuid, 'canary', 'should not survive teardown', 'core', $1::uuid, 1700000000000, 1700000000000)
    , .{FLEET_ID});
    try std.testing.expectEqual(@as(i64, 1), try countMemory(conn, FLEET_ID));

    // Purge the account by its oidc_subject.
    const purged = try teardown.purgeByOidcSubject(conn, std.testing.allocator, OIDC, &.{});
    try std.testing.expect(purged.purged);

    // The memory row is gone (regression target) and the cascade reached the
    // user — proving every statement before the user delete (incl. memory) ran.
    try std.testing.expectEqual(@as(i64, 0), try countMemory(conn, FLEET_ID));
    try std.testing.expectEqual(@as(i64, 0), try countUsers(conn, OIDC));
}
