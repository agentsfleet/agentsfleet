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

// `uid id host_id token_hash sandbox_tier admin_state labels tenant_id last_seen_at
//  created_at updated_at` — the frozen `fleet.runners` column set.
const EXPECTED_COLUMN_COUNT: i64 = 11;
const EXPECTED_NAMED_CONSTRAINTS: i64 = 2;
const EXPECTED_CORE_KEY_CONSTRAINTS: i64 = 6;

// `uid id runner_id zombie_id workspace_id tenant_id event_id actor event_type
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
        \\  AND rel.relname IN ('agent_keys', 'integration_grants')
        \\  AND c.conname IN (
        \\    'ck_agent_keys_uid_uuidv7',
        \\    'ck_agent_keys_agent_id_uuidv7',
        \\    'ck_agent_keys_uid_matches_agent_id',
        \\    'ck_integration_grants_uid_uuidv7',
        \\    'ck_integration_grants_grant_id_uuidv7',
        \\    'ck_integration_grants_uid_matches_grant_id'
        \\  )
    ));

    // pin test: index name is the schema promise.
    try std.testing.expectEqual(@as(i64, 1), try scalarI64(
        ctx.conn,
        "SELECT count(*)::bigint FROM pg_indexes WHERE schemaname = 'fleet' AND indexname = 'runner_leases_runner_status_idx'",
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
