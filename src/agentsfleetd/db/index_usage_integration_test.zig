//! Integration tier for schema slot 033: every index it adds must be CHOSEN BY
//! THE PLANNER for the query that justifies it.
//!
//! Asserting the index exists would pass on a merely-created index that the
//! planner never picks -- indistinguishable from a fix, in a green suite. So
//! every test here reads an `EXPLAIN` plan and asserts on the node, and seeds
//! enough rows that an index scan is genuinely the cheaper plan. Under-seeding
//! is the failure mode to watch: on a small table a sequential scan IS correct,
//! and the test would fail for the wrong reason.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const base = @import("test_fixtures.zig");
const schema = @import("schema");
const PgQuery = @import("pg_query.zig").PgQuery;

/// The migration slot this suite covers.
const SLOT_VERSION: i32 = 33;

/// The registered slot's text, or null when nothing claims that version.
fn slotSql(version: i32) ?[]const u8 {
    for (schema.migrations) |m| {
        if (m.version == version) return m.sql;
    }
    return null;
}

/// Row count that makes an index scan cheaper than a sequential scan on these
/// tables. Measured, not guessed: at 20k rows the sweeper read plans as an
/// index scan in 6 buffer hits; the same query at default table size plans as
/// a seq scan + sort.
const SEED_ROWS: u32 = 20_000;

/// Distinct synthetic scopes so a re-run, or a parallel suite, never collides.
const FLEET_MEM = "0195b4ba-8d3a-7f13-8abc-0000000b0002";
const HOST_PREFIX = "idxprobe-";
const MEM_ID_PREFIX = "idxprobe-mem-";
const KEY_PREFIX = "idxprobe-key-";

/// Memory fixture: the probe fleet is a selective slice of a table holding many
/// fleets, mirroring production. These exact proportions are the ones the plan
/// assertions were measured against -- shrinking them flips the planner back to
/// a bitmap scan, which is correct for a small table and would fail the test for
/// the wrong reason.
const MEM_SEED_ROWS: u32 = 40_000;
const PROBE_FLEET_ROWS: i32 = 4_000;

/// Every index slot 033 creates, in file order.
const SLOT_033_INDEXES = [_][]const u8{
    "idx_runner_affinity_last_runner_id_leased_until",
    "idx_runners_updated_at_id",
    "idx_runner_leases_fleet_id_status_fencing_token",
    "idx_memory_entries_fleet_id_updated_at_id",
    "idx_fleet_events_workspace_id_created_at_event_id",
    "idx_fleets_workspace_id_created_at_id",
    "idx_api_keys_tenant_id_created_at_uid",
    "idx_api_keys_tenant_id_key_name_uid",
    "idx_runners_created_at_id",
    "idx_runners_host_id_id",
};

const TestDb = struct {
    pool: *pg.Pool,
    conn: *pg.Conn,

    fn open(alloc: std.mem.Allocator) !?TestDb {
        if (common.env.testLiveValue("LIVE_DB") == null) return null;
        const ctx = (try base.openTestConn(alloc)) orelse return null;
        return .{ .pool = ctx.pool, .conn = ctx.conn };
    }

    fn close(self: TestDb) void {
        self.pool.release(self.conn);
        self.pool.deinit();
    }
};

/// Read an `EXPLAIN` plan back as one text blob. Caller owns the result.
fn planOf(alloc: std.mem.Allocator, conn: *pg.Conn, sql: []const u8) ![]u8 {
    const explain = try std.fmt.allocPrint(alloc, "EXPLAIN (COSTS OFF) {s}", .{sql});
    defer alloc.free(explain);
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    var q = PgQuery.from(try conn.query(explain, .{}));
    defer q.deinit();
    while (try q.next()) |row| {
        try out.appendSlice(alloc, try row.get([]const u8, 0));
        try out.append(alloc, '\n');
    }
    return out.toOwnedSlice(alloc);
}

/// The planner chose `index_name` -- not merely that the index exists.
fn expectIndex(plan: []const u8, index_name: []const u8) !void {
    if (std.mem.indexOf(u8, plan, index_name) == null) {
        std.debug.print("expected index {s} in plan:\n{s}\n", .{ index_name, plan });
        return error.IndexNotChosen;
    }
}

/// No Sort node: the index supplied the ordering rather than the executor.
fn expectNoSort(plan: []const u8) !void {
    if (std.mem.indexOf(u8, plan, "Sort") != null) {
        std.debug.print("expected no Sort node in plan:\n{s}\n", .{plan});
        return error.PlanSorts;
    }
}

/// Bulk-seed runners. `generate_series` keeps this ~1s at SEED_ROWS; the
/// version nibble at text position 15 satisfies ck_runners_uid_uuidv7.
fn seedRunners(conn: *pg.Conn, rows: u32) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels,
        \\   last_seen_at, created_at, updated_at)
        \\SELECT overlay(gen_random_uuid()::text placing '7' from 15 for 1)::uuid,
        \\       $1 || g, $1 || g, 'standard',
        \\       CASE WHEN g % 100 = 0 THEN 'draining' ELSE 'active' END,
        \\       '[]'::jsonb, 1750000000000 + g, 1750000000000 + g, 1750000000000 + g
        \\FROM generate_series(1, $2::int) g
        \\ON CONFLICT DO NOTHING
    , .{ HOST_PREFIX, @as(i32, @intCast(rows)) });
    _ = try conn.exec("ANALYZE fleet.runners", .{});
}

fn wipeRunners(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM fleet.runners WHERE host_id LIKE $1", .{HOST_PREFIX ++ "%"}) catch |err|
        std.log.warn("runner wipe ignored: {s}", .{@errorName(err)});
}

/// Seed memory across MANY fleets, with the probe fleet a small slice of the
/// table. A single-fleet seed would be the wrong shape: if every row matches
/// the filter, a sequential scan really is the cheaper plan and the planner is
/// right to take it. Hydration is per-fleet on a table holding every fleet, so
/// the fixture has to reproduce that selectivity to prove anything.
fn seedMemory(conn: *pg.Conn, rows: u32) !void {
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries
        \\  (uid, id, key, content, category, fleet_id, created_at, updated_at)
        \\SELECT overlay(gen_random_uuid()::text placing '7' from 15 for 1)::uuid,
        \\       $1 || g, 'k' || g, 'content', 'core',
        \\       CASE WHEN g <= $3::int THEN $2::uuid
        \\            ELSE md5((g % 200)::text)::uuid END,
        \\       1750000000000 + g, 1750000000000 + g
        \\FROM generate_series(1, $4::int) g
        \\ON CONFLICT DO NOTHING
    , .{ MEM_ID_PREFIX, FLEET_MEM, @as(i32, PROBE_FLEET_ROWS), @as(i32, @intCast(rows)) });
    _ = try conn.exec("ANALYZE memory.memory_entries", .{});
}

fn wipeMemory(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM memory.memory_entries WHERE id LIKE $1", .{MEM_ID_PREFIX ++ "%"}) catch |err|
        std.log.warn("memory wipe ignored: {s}", .{@errorName(err)});
}

/// Api-keys for one tenant. `api_keys_revoked_iff_inactive` ties `active` to
/// `revoked_at`, so every seeded row is active with a null revocation.
fn seedApiKeys(conn: *pg.Conn, rows: u32) !void {
    try base.seedTenant(conn);
    _ = try conn.exec(
        \\INSERT INTO core.api_keys
        \\  (uid, tenant_id, key_name, description, key_hash, created_by, active,
        \\   revoked_at, last_used_at, created_at, updated_at)
        \\SELECT overlay(md5('k' || g)::uuid::text placing '7' from 15 for 1)::uuid,
        \\       $1::uuid, $2 || g, '', $2 || g, 'seed', TRUE,
        \\       NULL, NULL, 1750000000000 + g, 0
        \\FROM generate_series(1, $3::int) g
        \\ON CONFLICT DO NOTHING
    , .{ base.TEST_TENANT_ID, KEY_PREFIX, @as(i32, @intCast(rows)) });
    _ = try conn.exec("ANALYZE core.api_keys", .{});
}

fn wipeApiKeys(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.api_keys WHERE key_name LIKE $1", .{KEY_PREFIX ++ "%"}) catch |err|
        std.log.warn("api key wipe ignored: {s}", .{@errorName(err)});
}

test "api key list sorts are served by an index" {
    // Both sort columns, one index each. Direction does not need its own index:
    // `tenant_id` leads as an equality, so a btree serves ascending forward and
    // descending backward.
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer wipeApiKeys(db.conn);
    try seedApiKeys(db.conn, SEED_ROWS);

    const by_created = try planOf(alloc, db.conn,
        \\SELECT uid, key_name FROM core.api_keys
        \\WHERE tenant_id = '0195b4ba-8d3a-7f13-8abc-000000000001'::uuid
        \\ORDER BY created_at DESC, uid DESC LIMIT 25 OFFSET 0
    );
    defer alloc.free(by_created);
    try expectIndex(by_created, "idx_api_keys_tenant_id_created_at_uid");
    try expectNoSort(by_created);

    const by_name = try planOf(alloc, db.conn,
        \\SELECT uid, key_name FROM core.api_keys
        \\WHERE tenant_id = '0195b4ba-8d3a-7f13-8abc-000000000001'::uuid
        \\ORDER BY key_name ASC, uid ASC LIMIT 25 OFFSET 0
    );
    defer alloc.free(by_name);
    try expectIndex(by_name, "idx_api_keys_tenant_id_key_name_uid");
    try expectNoSort(by_name);
}

test "slot 033 indexes are applied exactly once" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    for (SLOT_033_INDEXES) |name| {
        var q = PgQuery.from(try db.conn.query(
            "SELECT COUNT(*)::bigint FROM pg_indexes WHERE indexname = $1",
            .{name},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.DbRowShape;
        const n = try row.get(i64, 0);
        if (n != 1) {
            std.debug.print("index {s} present {d} times, want 1\n", .{ name, n });
            return error.IndexNotAppliedOnce;
        }
    }
}

test "slot 033 re-applies as a no-op" {
    // Idempotency by construction: every statement in the slot is guarded, so a
    // re-run against a provisioned database changes nothing. Reading it back
    // through the registered migration array rather than the file also proves
    // the slot is wired into `schema/embed.zig` -- an unregistered slot never
    // runs at all, and would otherwise fail only at first deploy.
    const slot = slotSql(SLOT_VERSION) orelse return error.SlotNotRegistered;
    var lines = std.mem.splitScalar(u8, slot, '\n');
    var guarded: usize = 0;
    while (lines.next()) |raw| {
        // Comment lines discuss DDL without being it -- match statements only.
        const line = std.mem.trim(u8, raw, " \t\r");
        if (!std.mem.startsWith(u8, line, "CREATE INDEX")) continue;
        if (std.mem.indexOf(u8, line, "IF NOT EXISTS") == null) {
            std.debug.print("unguarded statement in slot 033:\n{s}\n", .{line});
            return error.MigrationNotIdempotent;
        }
        guarded += 1;
    }
    try std.testing.expectEqual(SLOT_033_INDEXES.len, guarded);
}

test "due-runner read is index-ordered" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer wipeRunners(db.conn);
    try seedRunners(db.conn, SEED_ROWS);

    const plan = try planOf(alloc, db.conn,
        \\SELECT r.id::text, r.last_seen_at, r.admin_state
        \\FROM fleet.runners r
        \\WHERE (r.last_seen_at <> -1 AND (1750000099999::bigint - r.last_seen_at) > 90000)
        \\   OR r.admin_state = 'draining'
        \\   OR (r.admin_state <> 'active' AND EXISTS (
        \\        SELECT 1 FROM fleet.runner_leases l
        \\        WHERE l.runner_id = r.id AND l.status = 'active'))
        \\ORDER BY r.updated_at ASC, r.id ASC
        \\LIMIT 200
    );
    defer alloc.free(plan);
    try expectIndex(plan, "idx_runners_updated_at_id");
    try expectNoSort(plan);
}

test "runner list sorts are served by an index" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer wipeRunners(db.conn);
    try seedRunners(db.conn, SEED_ROWS);

    const by_created = try planOf(alloc, db.conn,
        \\SELECT r.id::text FROM fleet.runners r
        \\ORDER BY r.created_at DESC, r.id DESC LIMIT 25 OFFSET 0
    );
    defer alloc.free(by_created);
    try expectIndex(by_created, "idx_runners_created_at_id");
    try expectNoSort(by_created);

    const by_host = try planOf(alloc, db.conn,
        \\SELECT r.id::text FROM fleet.runners r
        \\ORDER BY r.host_id ASC, r.id ASC LIMIT 25 OFFSET 0
    );
    defer alloc.free(by_host);
    try expectIndex(by_host, "idx_runners_host_id_id");
    try expectNoSort(by_host);
}

test "bounded memory read is index-ordered" {
    // The composite index earns its ordering only where the plan can exit early.
    // `fleet_memory.listAll` fetches a fleet's WHOLE memory set with no LIMIT, and
    // for an unbounded fetch PostgreSQL correctly prefers bitmap-scan + sort: an
    // ordered index scan buys nothing when every row is returned anyway, and costs
    // random heap access. Measured on a 4000-of-40000 fixture, both with and
    // without the narrow `idx_memory_entries_fleet_id` present.
    //
    // So this asserts the bounded shape, which is what the index actually serves.
    // Spec Dimension 1.4 claims hydration itself plans without a sort; that is not
    // achievable without giving `listAll` a LIMIT, which changes behaviour and is
    // out of scope here. Recorded in the spec's Discovery.
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer wipeMemory(db.conn);
    try seedMemory(db.conn, MEM_SEED_ROWS);

    const plan = try planOf(alloc, db.conn,
        \\SELECT key, content, category
        \\FROM memory.memory_entries
        \\WHERE fleet_id = '0195b4ba-8d3a-7f13-8abc-0000000b0002'::uuid
        \\ORDER BY updated_at DESC, id DESC
        \\LIMIT 50
    );
    defer alloc.free(plan);
    try expectIndex(plan, "idx_memory_entries_fleet_id_updated_at_id");
    try expectNoSort(plan);
}
