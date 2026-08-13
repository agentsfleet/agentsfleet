//! Integration tier for schema slot 034: the three retired indexes are gone, and
//! the queries that used them still plan against an index.
//!
//! A removal is only safe if the work relocates rather than disappears, so
//! absence alone is not the assertion — each test also reads the plan of the
//! query the dropped index used to serve.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise.

const std = @import("std");
const pg = @import("pg");
const base = @import("test_fixtures.zig");

const IndexRef = struct { schema: []const u8, name: []const u8 };

/// Indexes slot 034 retires. All three are defined in shipped slots (010, 012,
/// 015), so their absence proves the migration ran, not merely that they were
/// never made. Schema-qualified so the absence check reads the one object that
/// owns the name, not a same-named index in another schema.
const RETIRED_INDEXES = [_]IndexRef{
    .{ .schema = "core", .name = "idx_api_keys_key_hash_active" },
    .{ .schema = "memory", .name = "idx_memory_entries_fleet_id" },
    .{ .schema = "core", .name = "idx_fleet_events_workspace_id_created_at" },
};

/// Deliberately KEPT: it recorded scans under the workload, so it failed the
/// zero-scan bar. Pinned here so a later cleanup cannot quietly widen the drop.
const KEPT_INDEX = IndexRef{ .schema = "memory", .name = "idx_memory_entries_fleet_id_category_updated_at" };

const KEY_PREFIX = "rmprobe-key-";
const MEM_KEY_PREFIX = "rmprobe-mem-";
const FLEET_PROBE = "0195b4ba-8d3a-7f13-8abc-0000000e0001";
/// `memory_entries.fleet_id` REFERENCES `core.fleets`, so the spread below needs
/// a parent row per distinct id. This workspace owns them and nothing else; its
/// teardown cascades to the fleets and their memory rows in one step.
const WS_MEM = "0195b4ba-8d3a-7f13-8abc-0000000e0000";
const MEM_FLEET_NAME_PREFIX = "rmprobe-fleet-";
const SEED_ROWS: i32 = 200;
const MEM_SEED_ROWS: i32 = 200;
const PROBE_FLEET_ROWS: i32 = 20;

// Shared setup + EXPLAIN reader live in test_fixtures.zig (Dimension 6.3).
const TestDb = base.TestDb;
const planOf = base.planOf;

fn indexExists(conn: *pg.Conn, ref: IndexRef) !bool {
    return (try base.indexCount(conn, ref.schema, ref.name)) > 0;
}

/// The index exists in `schema` and indexes exactly `want_columns` — read
/// structurally from the catalog (see `base.indexKeyColumns`), so a reorder or
/// dropped direction fails here.
fn expectIndexShape(alloc: std.mem.Allocator, conn: *pg.Conn, schema: []const u8, name: []const u8, want_columns: []const u8) !void {
    const got = base.indexKeyColumns(alloc, conn, schema, name) catch |err| {
        if (err == error.IndexMissing) std.debug.print("index {s}.{s} does not exist\n", .{ schema, name });
        return err;
    };
    defer alloc.free(got);
    if (!std.mem.eql(u8, got, want_columns)) {
        std.debug.print("index {s}.{s} columns:\n  want: {s}\n  got:  {s}\n", .{ schema, name, want_columns, got });
        return error.IndexShapeChanged;
    }
}

/// `index_name` CAN serve `sql`'s filter: with sequential scans disabled the
/// planner reaches for it. Size independent — asks whether the index fits the
/// query, which is what "the drop is safe" reduces to once the old index is gone.
fn expectServesFilter(alloc: std.mem.Allocator, conn: *pg.Conn, sql: []const u8, index_name: []const u8) !void {
    _ = try conn.exec("SET enable_seqscan = off", .{});
    defer _ = conn.exec("RESET enable_seqscan", .{}) catch |err|
        std.log.warn("reset enable_seqscan ignored: {s}", .{@errorName(err)});
    // Bitmap scans answer a bare fleet_id filter through ANY fleet-prefixed
    // index, so a sibling index (the keyset composite pages ride) makes the
    // planner's bitmap pick arbitrary. Disabling them leaves only ordered
    // index scans, and the one index supplying the ORDER BY wins — the
    // fitness question this helper asks, now deterministic among siblings.
    _ = try conn.exec("SET enable_bitmapscan = off", .{});
    defer _ = conn.exec("RESET enable_bitmapscan", .{}) catch |err|
        std.log.warn("reset enable_bitmapscan ignored: {s}", .{@errorName(err)});
    const plan = try planOf(alloc, conn, sql, .{});
    defer alloc.free(plan);
    if (std.mem.indexOf(u8, plan, index_name) == null) {
        std.debug.print("expected index {s} in plan:\n{s}\n", .{ index_name, plan });
        return error.IndexNotChosen;
    }
}

test "slot 034 retired exactly the three approved indexes" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    for (RETIRED_INDEXES) |ref| {
        if (try indexExists(db.conn, ref)) {
            std.debug.print("index {s}.{s} still present after slot 034\n", .{ ref.schema, ref.name });
            return error.IndexNotRetired;
        }
    }
    // idx_memory_entries_category stays. Its 4 recorded scans failed the
    // zero-scan bar, and widening the drop on reasoning alone is what the guard
    // exists to stop.
    try std.testing.expect(try indexExists(db.conn, KEPT_INDEX));
}

test "api key auth lookup survives the drop" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    try base.seedTenant(db.conn);
    defer _ = db.conn.exec("DELETE FROM core.api_keys WHERE key_name LIKE $1", .{KEY_PREFIX ++ "%"}) catch |err|
        std.log.warn("api key teardown ignored: {s}", .{@errorName(err)});

    _ = try db.conn.exec(
        \\INSERT INTO core.api_keys
        \\  (id, tenant_id, key_name, description, key_hash, created_by, active,
        \\   revoked_at, last_used_at, created_at, updated_at)
        \\SELECT overlay(md5('rk' || g)::uuid::text placing '7' from 15 for 1)::uuid,
        \\       $1::uuid, $2 || g, '', $2 || g, 'seed', TRUE, NULL, NULL, g, 0
        \\FROM generate_series(1, $3::int) g
        \\ON CONFLICT DO NOTHING
    , .{ base.TEST_TENANT_ID, KEY_PREFIX, SEED_ROWS });
    _ = try db.conn.exec("ANALYZE core.api_keys", .{});

    // The dropped index was partial on `active`; the auth path never filters on
    // it, so the unique index on key_hash alone is the one that must serve this.
    try expectServesFilter(alloc, db.conn,
        \\SELECT id FROM core.api_keys WHERE key_hash = 'rmprobe-key-100'
    , "uq_api_keys_key_hash");
}

test "the composite still covers the fleet filter after the drop" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer _ = db.conn.exec("DELETE FROM memory.memory_entries WHERE key LIKE $1", .{MEM_KEY_PREFIX ++ "%"}) catch |err|
        std.log.warn("memory teardown ignored: {s}", .{@errorName(err)});
    // Drop the parent fleets with their workspace, then re-analyze: deleting the
    // rows leaves the planner's estimates behind, and sibling suites assert which
    // index `core.fleets` reads pick.
    defer _ = db.conn.exec("ANALYZE core.fleets", .{}) catch |err|
        std.log.warn("analyze ignored: {s}", .{@errorName(err)});
    defer base.teardownWorkspace(db.conn, WS_MEM);

    // Parents first. The spread below derives its fleet ids arithmetically, so
    // the parents derive theirs from the same expression over the same series —
    // a hand-listed set would drift the moment the spread's modulus changed.
    //
    // Both spell the id through the same `overlay(... placing '7' from 15 for 1)`
    // because `core.fleets` carries `ck_fleets_id_uuidv7`: a bare md5 digest is
    // not UUIDv7-shaped and the check refuses it. The two expressions must stay
    // character-identical or the child rows point at parents that do not exist.
    try base.seedTenant(db.conn);
    try base.seedWorkspace(db.conn, WS_MEM);
    try base.seedFleet(db.conn, FLEET_PROBE, WS_MEM, MEM_FLEET_NAME_PREFIX ++ "probe", "{}", "");
    _ = try db.conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\SELECT overlay(md5((g % 200)::text)::uuid::text placing '7' from 15 for 1)::uuid,
        \\       w.id, w.tenant_id,
        \\       $2 || (g % 200)::text, '', '{}'::jsonb, 'active', 0, 0
        \\FROM generate_series(1, $3::int) g
        \\CROSS JOIN core.workspaces w WHERE w.id = $1::uuid
        \\ON CONFLICT DO NOTHING
    , .{ WS_MEM, MEM_FLEET_NAME_PREFIX, MEM_SEED_ROWS });

    _ = try db.conn.exec(
        \\INSERT INTO memory.memory_entries
        \\  (id, key, content, category, fleet_id, created_at, updated_at)
        \\SELECT overlay(md5('rm' || g)::uuid::text placing '7' from 15 for 1)::uuid,
        \\       $1 || g, 'content', 'core',
        \\       CASE WHEN g <= $3::int THEN $2::uuid
        \\            ELSE overlay(md5((g % 200)::text)::uuid::text placing '7' from 15 for 1)::uuid END,
        \\       g, g
        \\FROM generate_series(1, $4::int) g
        \\ON CONFLICT DO NOTHING
    , .{ MEM_KEY_PREFIX, FLEET_PROBE, PROBE_FLEET_ROWS, MEM_SEED_ROWS });
    _ = try db.conn.exec("ANALYZE memory.memory_entries", .{});

    // "The drop is safe" reduces to: with the narrow index gone, an index still
    // covers the fleet_id filter. The composite does — it leads with fleet_id, so
    // anything the dropped index could answer it answers too. Proven by shape plus
    // a forced-index plan, both size independent. Whether the planner PREFERS it
    // over a sequential scan for the unbounded `listAll` is a scale-dependent
    // cost-model call (crossover measured near 3%), out of scope for this test.
    try expectIndexShape(alloc, db.conn, "memory", "idx_memory_entries_fleet_id_updated_at_id", "fleet_id, updated_at DESC, id DESC");
    try expectServesFilter(alloc, db.conn,
        \\SELECT key, content, category FROM memory.memory_entries
        \\WHERE fleet_id = '0195b4ba-8d3a-7f13-8abc-0000000e0001'::uuid
        \\ORDER BY updated_at DESC, id DESC
    , "idx_memory_entries_fleet_id_updated_at_id");
}
