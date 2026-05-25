//! Integration: the runner-fleet schema migrations land `fleet.runners` (`021`)
//! and `fleet.runner_leases` (`022`) with their columns and constraints in a
//! migrated database.
//!
//! DB-gated — skips when `TEST_DATABASE_URL` is unset; the live test DB has the
//! migrations applied by `_reset-test-db`. The migration array's ordering and
//! SQL parseability are unit-tested in `cmd/common.zig`; both are idempotent by
//! construction (`CREATE SCHEMA/TABLE IF NOT EXISTS`).

const std = @import("std");
const pg = @import("pg");
const parseUrl = @import("../db/pool.zig").parseUrl;
const PgQuery = @import("../db/pg_query.zig").PgQuery;

// `id host_id token_hash sandbox_tier status labels tenant_id last_seen_at
//  created_at updated_at` — the frozen `fleet.runners` column set.
const EXPECTED_COLUMN_COUNT: i64 = 10;
const EXPECTED_NAMED_CONSTRAINTS: i64 = 2;

// `id runner_id zombie_id workspace_id tenant_id event_id posture model
//  fencing_token lease_expires_at status created_at updated_at` — the frozen
//  `fleet.runner_leases` column set.
const EXPECTED_LEASE_COLUMN_COUNT: i64 = 13;

fn openConnOrSkip(alloc: std.mem.Allocator) !?struct { pool: *pg.Pool, conn: *pg.Conn } {
    const url = std.process.getEnvVarOwned(alloc, "TEST_DATABASE_URL") catch return null;
    defer alloc.free(url);
    // parseUrl allocates host/auth strings that must outlive the pool.
    const opts = try parseUrl(std.heap.page_allocator, url);
    const pool = pg.Pool.init(alloc, opts) catch return null;
    errdefer pool.deinit();
    const conn = pool.acquire() catch {
        pool.deinit();
        return null;
    };
    return .{ .pool = pool, .conn = conn };
}

test "runner schema: fleet.runners is migrated with its columns and constraints" {
    const alloc = std.testing.allocator;
    const ctx = (try openConnOrSkip(alloc)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);

    // Table present in the `fleet` control-plane schema.
    var treg = PgQuery.from(try ctx.conn.query("SELECT to_regclass('fleet.runners')::text", .{}));
    defer treg.deinit();
    const trow = (try treg.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expect((try trow.get(?[]const u8, 0)) != null);

    // Full column set.
    var cols = PgQuery.from(try ctx.conn.query(
        "SELECT count(*)::bigint FROM information_schema.columns WHERE table_schema = 'fleet' AND table_name = 'runners'",
        .{},
    ));
    defer cols.deinit();
    const crow = (try cols.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(EXPECTED_COLUMN_COUNT, try crow.get(i64, 0));

    // Named constraints: token-hash uniqueness + the UUIDv7 id check.
    // pin test: constraint names are the schema contract.
    var cons = PgQuery.from(try ctx.conn.query(
        "SELECT count(*)::bigint FROM pg_constraint WHERE conname IN ('uq_runners_token_hash', 'ck_runners_id_uuidv7')",
        .{},
    ));
    defer cons.deinit();
    const conrow = (try cons.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(EXPECTED_NAMED_CONSTRAINTS, try conrow.get(i64, 0));
}

test "runner schema: fleet.runner_leases is migrated with its columns and constraint" {
    const alloc = std.testing.allocator;
    const ctx = (try openConnOrSkip(alloc)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);

    // Table present in the `fleet` control-plane schema.
    var treg = PgQuery.from(try ctx.conn.query("SELECT to_regclass('fleet.runner_leases')::text", .{}));
    defer treg.deinit();
    const trow = (try treg.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expect((try trow.get(?[]const u8, 0)) != null);

    // Full column set.
    var cols = PgQuery.from(try ctx.conn.query(
        "SELECT count(*)::bigint FROM information_schema.columns WHERE table_schema = 'fleet' AND table_name = 'runner_leases'",
        .{},
    ));
    defer cols.deinit();
    const crow = (try cols.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(EXPECTED_LEASE_COLUMN_COUNT, try crow.get(i64, 0));

    // Named UUIDv7 id check constraint.
    // pin test: constraint name is the schema contract.
    var cons = PgQuery.from(try ctx.conn.query(
        "SELECT count(*)::bigint FROM pg_constraint WHERE conname = 'ck_runner_leases_id_uuidv7'",
        .{},
    ));
    defer cons.deinit();
    const conrow = (try cons.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(i64, 1), try conrow.get(i64, 0));
}
