//! Integration: the runner-fleet schema migrations land `fleet.runners` (`021`)
//! `fleet.runner_leases` (`022`), and `fleet.runner_events` (`025`) with their
//! columns and constraints in a migrated database.
//!
//! DB-gated — skips when `TEST_DATABASE_URL` is unset; the live test DB has the
//! migrations applied by `_reset-test-db`. The migration array's ordering and
//! SQL parseability are unit-tested in `cmd/common.zig`; both are idempotent by
//! construction (`CREATE SCHEMA/TABLE IF NOT EXISTS`).

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const parseUrl = @import("../db/pool.zig").parseUrl;
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const base = @import("../db/test_fixtures.zig");

// `uid id host_id token_hash sandbox_tier admin_state labels tenant_id last_seen_at
//  created_at updated_at` — the frozen `fleet.runners` column set.
const EXPECTED_COLUMN_COUNT: i64 = 11;
const EXPECTED_NAMED_CONSTRAINTS: i64 = 2;
const EXPECTED_CORE_KEY_CONSTRAINTS: i64 = 6;

// `uid id runner_id fleet_id workspace_id tenant_id event_id actor event_type
//  request_json event_created_at posture provider model metered_input_tokens
//  metered_cached_tokens metered_output_tokens last_metered_at_ms fencing_token
//  lease_expires_at status created_at updated_at` — the `fleet.runner_leases`
//  column set. The actor/event_type/request_json/event_created_at envelope is
//  stored so a reclaim can re-lease the event from Postgres alone (no Redis
//  re-read); `provider` keys the composite rate lookup; the `metered_*` +
//  `last_metered_at_ms` cursor backs the incremental renewal metering.
const EXPECTED_LEASE_COLUMN_COUNT: i64 = 23;
const EXPECTED_EVENT_COLUMN_COUNT: i64 = 8;

fn openConnOrSkip(alloc: std.mem.Allocator) !?struct { pool: *pg.Pool, conn: *pg.Conn } {
    const url = common.env.testLiveValue("TEST_DATABASE_URL") orelse return null;
    // parseUrl allocates host/auth strings that must outlive the pool.
    const opts = try parseUrl(std.heap.page_allocator, url);
    const pool = pg.Pool.init(common.globalIo(), alloc, opts) catch return null;
    errdefer pool.deinit();
    const conn = pool.acquire() catch {
        pool.deinit();
        return null;
    };
    return .{ .pool = pool, .conn = conn };
}

/// Run a scalar `bigint` query and fully drain it before returning, so the
/// caller can issue the next query on the same connection without
/// `error.ConnectionBusy` — a deferred `deinit` leaves the prior result in
/// flight until the test exits.
fn scalarI64(conn: *pg.Conn, sql: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(sql, .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return row.get(i64, 0);
}

/// Drain-safe `to_regclass(...) IS NOT NULL` probe (see `scalarI64`).
fn regclassExists(conn: *pg.Conn, sql: []const u8) !bool {
    var q = PgQuery.from(try conn.query(sql, .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return (try row.get(?[]const u8, 0)) != null;
}

test "runner schema: fleet.runners is migrated with its columns and constraints" {
    const alloc = std.testing.allocator;
    const ctx = (try openConnOrSkip(alloc)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);

    // Table present in the `fleet` control-plane schema.
    try std.testing.expect(try regclassExists(ctx.conn, "SELECT to_regclass('fleet.runners')::text"));

    // Full column set.
    try std.testing.expectEqual(EXPECTED_COLUMN_COUNT, try scalarI64(
        ctx.conn,
        "SELECT count(*)::bigint FROM information_schema.columns WHERE table_schema = 'fleet' AND table_name = 'runners'",
    ));

    // Named constraints: token-hash uniqueness + the Universally Unique Identifier version 7
    // (UUIDv7) Unique Identifier (UID) check. Pin test: constraint names are the schema rule.
    try std.testing.expectEqual(EXPECTED_NAMED_CONSTRAINTS, try scalarI64(
        ctx.conn,
        "SELECT count(*)::bigint FROM pg_constraint WHERE conname IN ('uq_runners_token_hash', 'ck_runners_uid_uuidv7')",
    ));
}

test "runner schema: fleet.runner_leases is migrated with its columns and constraint" {
    const alloc = std.testing.allocator;
    const ctx = (try openConnOrSkip(alloc)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);

    // Table present in the `fleet` control-plane schema.
    try std.testing.expect(try regclassExists(ctx.conn, "SELECT to_regclass('fleet.runner_leases')::text"));

    // Full column set.
    try std.testing.expectEqual(EXPECTED_LEASE_COLUMN_COUNT, try scalarI64(
        ctx.conn,
        "SELECT count(*)::bigint FROM information_schema.columns WHERE table_schema = 'fleet' AND table_name = 'runner_leases'",
    ));

    // Named UUIDv7 UID check constraint.
    // Pin test: constraint name is the schema rule.
    try std.testing.expectEqual(@as(i64, 1), try scalarI64(
        ctx.conn,
        "SELECT count(*)::bigint FROM pg_constraint WHERE conname = 'ck_runner_leases_uid_uuidv7'",
    ));
}

test "core key schemas: public text ids have explicit UUIDv7 constraints" {
    const alloc = std.testing.allocator;
    const ctx = (try openConnOrSkip(alloc)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);

    try std.testing.expectEqual(EXPECTED_CORE_KEY_CONSTRAINTS, try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint
        \\FROM pg_constraint c
        \\JOIN pg_class rel ON rel.oid = c.conrelid
        \\JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
        \\WHERE nsp.nspname = 'core'
        \\  AND rel.relname IN ('fleet_keys', 'integration_grants')
        \\  AND c.conname IN (
        \\    'ck_fleet_keys_uid_uuidv7',
        \\    'ck_fleet_keys_fleet_key_id_uuidv7',
        \\    'ck_fleet_keys_uid_matches_fleet_key_id',
        \\    'ck_integration_grants_uid_uuidv7',
        \\    'ck_integration_grants_grant_id_uuidv7',
        \\    'ck_integration_grants_uid_matches_grant_id'
        \\  )
    ));

    // pin test: index name is the schema promise.
    try std.testing.expectEqual(@as(i64, 1), try scalarI64(
        ctx.conn,
        "SELECT count(*)::bigint FROM pg_indexes WHERE schemaname = 'fleet' AND indexname = 'idx_runner_leases_runner_id_status'",
    ));
}

test "runner schema: fleet.runner_events is migrated append-only" {
    const alloc = std.testing.allocator;
    const ctx = (try openConnOrSkip(alloc)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);

    try std.testing.expect(try regclassExists(ctx.conn, "SELECT to_regclass('fleet.runner_events')::text"));
    try std.testing.expectEqual(EXPECTED_EVENT_COLUMN_COUNT, try scalarI64(
        ctx.conn,
        "SELECT count(*)::bigint FROM information_schema.columns WHERE table_schema = 'fleet' AND table_name = 'runner_events'",
    ));
    // Pin test: constraint name is the schema rule.
    try std.testing.expectEqual(@as(i64, 1), try scalarI64(
        ctx.conn,
        "SELECT count(*)::bigint FROM pg_constraint WHERE conname = 'ck_runner_events_uid_uuidv7'",
    ));
    // pin test: index name is the schema promise.
    try std.testing.expectEqual(@as(i64, 1), try scalarI64(
        ctx.conn,
        "SELECT count(*)::bigint FROM pg_indexes WHERE schemaname = 'fleet' AND indexname = 'runner_events_offline_dedup_idx'",
    ));
}

// ── fleet_id foreign-key behaviour ──────────────────────────────────────────
// runner_leases.fleet_id and runner_affinity.fleet_id REFERENCES core.fleets(id)
// ON DELETE CASCADE (schema/022, 023). Proven against the live DB: deleting a
// fleet cascades its lease/affinity rows away (Dim 1.1), and inserting either row
// against a non-existent fleet is rejected with SQLSTATE 23503 (Dim 1.2).

// Distinct UUIDv7 literals (version nibble 7) — no collision with sibling tests.
const FK_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fa011";
const FK_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fac01";
const FK_RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0faa01";
const FK_LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0faf01";
const FK_AFFINITY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fae01";
// A fleet id with NO core.fleets row — the orphan the FK must reject.
const FK_ORPHAN_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fad99";

// Params: $1 id, $2 runner_id, $3 fleet_id, $4 workspace_id, $5 tenant_id. Only
// runner_id and fleet_id carry FKs; workspace_id/tenant_id are unconstrained UUIDs.
const FK_LEASE_INSERT =
    \\INSERT INTO fleet.runner_leases
    \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
    \\   event_type, request_json, event_created_at, posture, provider, model,
    \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
    \\   fencing_token, lease_expires_at, status, created_at, updated_at)
    \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, 'evt-fk-1', 'steer:test',
    \\        'chat', '{"message":"hi"}', 0, 'platform', 'p', 'm', 0, 0, 0, 0,
    \\        1, 0, 'active', 0, 0)
;

// Params: $1 id, $2 fleet_id. last_runner_id NULL isolates the fleet_id FK.
const FK_AFFINITY_INSERT =
    \\INSERT INTO fleet.runner_affinity
    \\  (id, fleet_id, last_runner_id, fencing_seq, leased_until,
    \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
    \\   created_at, updated_at)
    \\VALUES ($1::uuid, $2::uuid, NULL, 1, 0, 0, 0, 0, 0, 0, 0)
;

// Count probes, keyed on the row's own id so the cascade proof stays valid even
// if either FK were ever changed to ON DELETE SET NULL (a fleet_id-keyed affinity
// probe would read 0 on a nulled-but-surviving row and false-pass).
const FK_LEASE_COUNT = "SELECT count(*)::bigint FROM fleet.runner_leases WHERE id = '" ++ FK_LEASE_ID ++ "'::uuid";
const FK_AFFINITY_COUNT = "SELECT count(*)::bigint FROM fleet.runner_affinity WHERE id = '" ++ FK_AFFINITY_ID ++ "'::uuid";

fn seedFkRunner(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'fk-host', 'fk-hash', 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{FK_RUNNER_ID});
}

fn fkExecIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("fk cleanup ignored: {s}", .{@errorName(err)});
}

/// Best-effort teardown on a FRESH connection — an orphan-reject exec leaves its
/// own connection in a pg-error state, so cleanup cannot run on that connection.
fn fkCleanup(alloc: std.mem.Allocator) void {
    const db = (openConnOrSkip(alloc) catch return) orelse return;
    defer db.pool.deinit();
    defer db.pool.release(db.conn);
    const c = db.conn;
    fkExecIgnore(c, "DELETE FROM fleet.runner_leases WHERE id = $1::uuid", .{FK_LEASE_ID});
    fkExecIgnore(c, "DELETE FROM fleet.runner_affinity WHERE id = $1::uuid", .{FK_AFFINITY_ID});
    fkExecIgnore(c, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{FK_RUNNER_ID});
    base.teardownFleets(c, FK_WORKSPACE_ID); // fleet delete cascades any residual lease/affinity
    base.teardownWorkspace(c, FK_WORKSPACE_ID);
}

/// Assert the connection's last error was a foreign-key violation (SQLSTATE 23503).
fn expectFkViolation(conn: *pg.Conn) !void {
    const pg_err = conn.err orelse return error.ExpectedPgError;
    try std.testing.expectEqualStrings("23503", pg_err.code);
}

test "fleet FK: deleting a core.fleets row cascades its runner_leases and runner_affinity rows" {
    const alloc = std.testing.allocator;
    fkCleanup(alloc);
    defer fkCleanup(alloc);
    const db = (try openConnOrSkip(alloc)) orelse return error.SkipZigTest;
    defer db.pool.deinit();
    defer db.pool.release(db.conn);
    const conn = db.conn;

    try base.seedTenant(conn);
    try base.seedWorkspace(conn, FK_WORKSPACE_ID);
    try base.seedFleet(conn, FK_FLEET_ID, FK_WORKSPACE_ID, "fk-cascade", "{}", "# z");
    try seedFkRunner(conn);
    _ = try conn.exec(FK_LEASE_INSERT, .{ FK_LEASE_ID, FK_RUNNER_ID, FK_FLEET_ID, FK_WORKSPACE_ID, base.TEST_TENANT_ID });
    _ = try conn.exec(FK_AFFINITY_INSERT, .{ FK_AFFINITY_ID, FK_FLEET_ID });

    // Children present before the delete.
    try std.testing.expectEqual(@as(i64, 1), try scalarI64(conn, FK_LEASE_COUNT));
    try std.testing.expectEqual(@as(i64, 1), try scalarI64(conn, FK_AFFINITY_COUNT));

    // Delete the fleet → ON DELETE CASCADE drops both child rows.
    _ = try conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{FK_FLEET_ID});

    try std.testing.expectEqual(@as(i64, 0), try scalarI64(conn, FK_LEASE_COUNT));
    try std.testing.expectEqual(@as(i64, 0), try scalarI64(conn, FK_AFFINITY_COUNT));
}

test "fleet FK: an orphan fleet_id on runner_lease / runner_affinity is rejected (23503)" {
    const alloc = std.testing.allocator;
    fkCleanup(alloc);
    defer fkCleanup(alloc);

    // affinity — last_runner_id NULL, so the fleet_id FK is the only constraint
    // that can fire; the non-existent fleet is rejected.
    {
        const db = (try openConnOrSkip(alloc)) orelse return error.SkipZigTest;
        defer db.pool.deinit();
        defer db.pool.release(db.conn);
        try std.testing.expectError(error.PG, db.conn.exec(FK_AFFINITY_INSERT, .{ FK_AFFINITY_ID, FK_ORPHAN_FLEET_ID }));
        try expectFkViolation(db.conn);
    }
    // lease — a valid runner satisfies runner_id; the bogus fleet_id fires the FK.
    {
        const db = (try openConnOrSkip(alloc)) orelse return error.SkipZigTest;
        defer db.pool.deinit();
        defer db.pool.release(db.conn);
        try seedFkRunner(db.conn);
        try std.testing.expectError(error.PG, db.conn.exec(FK_LEASE_INSERT, .{ FK_LEASE_ID, FK_RUNNER_ID, FK_ORPHAN_FLEET_ID, FK_WORKSPACE_ID, base.TEST_TENANT_ID }));
        try expectFkViolation(db.conn);
    }
}
