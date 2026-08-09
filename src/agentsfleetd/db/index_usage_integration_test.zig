//! Integration tier for the hot-path indexes: each must be CHOSEN BY THE PLANNER
//! for the query that justifies it. They shared retired slot 033; since the
//! M154 rebuild each lives in the slot that owns its table, so this suite is
//! organised by index rather than by slot.
//!
//! WHAT IS UNDER TEST is what our code owns: each index's column list and order,
//! and that the index CAN serve its query. Whether the planner PREFERS it over a
//! sequential scan, and whether it supplies the ordering without a Sort node, are
//! scale-dependent cost-model decisions PostgreSQL owns — reproducing them took
//! tens of thousands of seeded rows per run. So the shape is pinned from the
//! catalog (free) and fitness is checked with `enable_seqscan = off` (size
//! independent). The one genuinely scale-sized memory assertion — that a read
//! relocates onto the composite after slot 034 drops the narrow index — lives in
//! `index_removal_integration_test.zig`, where the claim is load-bearing.
//!
//! The fleet-scoped indexes (affinity, leases, events) are covered by the
//! sibling `index_usage_fleet_integration_test.zig`, which needs a tenant ->
//! workspace -> fleet graph this file's tables do not.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise.

const std = @import("std");
const pg = @import("pg");
const base = @import("test_fixtures.zig");
const schema = @import("schema");
const protocol = @import("contract").protocol;
const fleet_sql = @import("../fleet/sql.zig");
const operator_sql = @import("../http/handlers/fleet/sql.zig");
const retention_sweeper = @import("../fleet/retention_sweeper.zig");
const telemetry_store = @import("../state/fleet_telemetry_store.zig");
const PgQuery = @import("pg_query.zig").PgQuery;

/// A minimal legible fixture. Fitness is checked with `enable_seqscan = off`, so
/// it does not depend on the probe fleet being a selective slice of a large
/// table — a few rows per fleet is enough for the plan to form.
const MEM_SEED_ROWS: u32 = 200;
const PROBE_FLEET_ROWS: i32 = 20;

const FLEET_MEM = "0195b4ba-8d3a-7f13-8abc-0000000b0002";
const MEM_KEY_PREFIX = "idxprobe-mem-";

/// The operator lease read's fixture (slot 041). Same doctrine as the memory
/// probe: fitness is asked with scans disabled, so a couple hundred rows is
/// enough for the plan to form.
const LEASE_SEED_ROWS: i32 = 200;
const WS_LEASE = "0195b4ba-8d3a-7f13-8abc-0000000c0001";
const FLEET_LEASE = "0195b4ba-8d3a-7f13-8abc-0000000c0002";
const RUNNER_LEASE = "0195b4ba-8d3a-7f13-8abc-0000000c0003";
const LEASE_EVENT_PREFIX = "idxprobe-evt-";
/// Slot 041's page index, named once because both the shape assertion and the
/// fitness probe below spell it.
const LEASES_BY_RUNNER_INDEX = "idx_runner_leases_runner_id_created_at_id";

/// The four hot-path indexes this suite plans against — deliberately only the
/// reads whose cost grows without bound. List sorts over runners, fleets and api
/// keys stay unindexed at the ~100-runner scale their slots document.
const IndexRef = struct { schema: []const u8, name: []const u8 };
const COVERED_HOT_PATH_INDEXES = [_]IndexRef{
    .{ .schema = "fleet", .name = "idx_runner_affinity_last_runner_id_leased_until" },
    .{ .schema = "fleet", .name = "idx_runner_leases_fleet_id_status_fencing_token" },
    .{ .schema = "core", .name = "idx_fleet_events_workspace_id_created_at_event_id" },
    .{ .schema = "memory", .name = "idx_memory_entries_fleet_id_updated_at_id" },
};

/// The registered slot's text, or null when nothing claims that version.
fn slotSql(version: i32) ?[]const u8 {
    for (schema.migrations) |m| {
        if (m.version == version) return m.sql;
    }
    return null;
}

// Shared setup + EXPLAIN reader live in test_fixtures.zig (Dimension 6.3).
const TestDb = base.TestDb;
const planOf = base.planOf;

/// The index exists in `schema` and indexes exactly `want_columns`, in that
/// order and those directions — read structurally from the catalog (see
/// `base.indexKeyColumns`), so a reorder or a dropped DESC fails here.
fn expectIndexShape(alloc: std.mem.Allocator, conn: *pg.Conn, schema_name: []const u8, name: []const u8, want_columns: []const u8) !void {
    const got = base.indexKeyColumns(alloc, conn, schema_name, name) catch |err| {
        if (err == error.IndexMissing) std.debug.print("index {s}.{s} does not exist\n", .{ schema_name, name });
        return err;
    };
    defer alloc.free(got);
    if (!std.mem.eql(u8, got, want_columns)) {
        std.debug.print("index {s}.{s} columns:\n  want: {s}\n  got:  {s}\n", .{ schema_name, name, want_columns, got });
        return error.IndexShapeChanged;
    }
}

/// `index_name` CAN serve `sql`'s filter: with sequential scans disabled the
/// planner reaches for it. Size independent — this asks whether the index fits
/// the query, not whether the cost model prefers it at some row count.
fn expectServesFilter(alloc: std.mem.Allocator, conn: *pg.Conn, sql: []const u8, args: anytype, index_name: []const u8) !void {
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
    const plan = try planOf(alloc, conn, sql, args);
    defer alloc.free(plan);
    if (std.mem.indexOf(u8, plan, index_name) == null) {
        std.debug.print("expected index {s} in plan:\n{s}\n", .{ index_name, plan });
        return error.IndexNotChosen;
    }
}

/// The negative variant of `expectServesFilter`. Disabling sequential scans
/// only prices them out — PostgreSQL still emits a Seq Scan when NO index can
/// serve a table's access. So `marker` (e.g. "Seq Scan on runner_leases")
/// surviving in the forced plan means some leg of `sql` can only be answered
/// by walking that table's whole history, which is exactly the shape this
/// helper exists to refuse. Size independent, like the positive variant.
fn expectPlanOmits(alloc: std.mem.Allocator, conn: *pg.Conn, sql: []const u8, args: anytype, marker: []const u8) !void {
    _ = try conn.exec("SET enable_seqscan = off", .{});
    defer _ = conn.exec("RESET enable_seqscan", .{}) catch |err|
        std.log.warn("reset enable_seqscan ignored: {s}", .{@errorName(err)});
    _ = try conn.exec("SET enable_bitmapscan = off", .{});
    defer _ = conn.exec("RESET enable_bitmapscan", .{}) catch |err|
        std.log.warn("reset enable_bitmapscan ignored: {s}", .{@errorName(err)});
    const plan = try planOf(alloc, conn, sql, args);
    defer alloc.free(plan);
    if (std.mem.indexOf(u8, plan, marker) != null) {
        std.debug.print("plan must not contain \"{s}\":\n{s}\n", .{ marker, plan });
        return error.ForcedTableScanInPlan;
    }
}

/// Seed memory across a handful of fleets, the probe fleet among them. Size and
/// selectivity are not load-bearing here (the fitness check forces the index),
/// so this stays small.
fn seedMemory(conn: *pg.Conn, rows: u32) !void {
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries
        \\  (id, key, content, category, fleet_id, created_at, updated_at)
        \\SELECT overlay(gen_random_uuid()::text placing '7' from 15 for 1)::uuid,
        \\       $1 || g, 'content', 'core',
        \\       CASE WHEN g <= $3::int THEN $2::uuid
        \\            ELSE md5((g % 200)::text)::uuid END,
        \\       1750000000000 + g, 1750000000000 + g
        \\FROM generate_series(1, $4::int) g
        \\ON CONFLICT DO NOTHING
    , .{ MEM_KEY_PREFIX, FLEET_MEM, @as(i32, PROBE_FLEET_ROWS), @as(i32, @intCast(rows)) });
    _ = try conn.exec("ANALYZE memory.memory_entries", .{});
}

fn wipeMemory(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM memory.memory_entries WHERE key LIKE $1", .{MEM_KEY_PREFIX ++ "%"}) catch |err|
        std.log.warn("memory wipe ignored: {s}", .{@errorName(err)});
}

/// One runner holding `rows` settled leases against one fleet. `runner_leases`
/// carries real foreign keys, so the tenant → workspace → fleet → runner chain
/// is seeded first (the memory fixture above needs none).
fn seedLeases(conn: *pg.Conn, rows: i32) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WS_LEASE);
    try base.seedFleet(conn, FLEET_LEASE, WS_LEASE, "index-probe-fleet", "{}", "# SKILL");
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'idxprobe-host', 'idxprobe-token-040', 'dev_none',
        \\        'active', '[]'::jsonb, 0, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{RUNNER_LEASE});
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens,
        \\   last_metered_at, fencing_token, lease_expires_at, status,
        \\   created_at, updated_at)
        \\SELECT overlay(gen_random_uuid()::text placing '7' from 15 for 1)::uuid,
        \\       $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5 || g,
        \\       'system', 'chat', g, 'metered', 'anthropic', 'claude',
        \\       0, 0, 0, 0, g, g, 'reported', g, g
        \\FROM generate_series(1, $6::int) g
        \\ON CONFLICT DO NOTHING
    , .{ RUNNER_LEASE, FLEET_LEASE, WS_LEASE, base.TEST_TENANT_ID, LEASE_EVENT_PREFIX, rows });
    _ = try conn.exec("ANALYZE fleet.runner_leases", .{});
}

fn wipeLeases(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM fleet.runner_leases WHERE runner_id = $1::uuid", .{RUNNER_LEASE}) catch |err|
        std.log.warn("lease wipe ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_LEASE}) catch |err|
        std.log.warn("probe runner wipe ignored: {s}", .{@errorName(err)});
    base.teardownFleets(conn, WS_LEASE);
    base.teardownWorkspace(conn, WS_LEASE);
}

test "slot 033 indexes are applied exactly once" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    for (COVERED_HOT_PATH_INDEXES) |entry| {
        const n = try base.indexCount(db.conn, entry.schema, entry.name);
        if (n != 1) {
            std.debug.print("index {s}.{s} present {d} times, want 1\n", .{ entry.schema, entry.name, n });
            return error.IndexNotAppliedOnce;
        }
    }
}

test "every covered index is created exactly once across the schema" {
    // The retired slot 033 grouped these four, so this suite could pin ONE
    // slot's SIZE and catch an index added without a matching plan assertion.
    // Each index now lives in the slot that owns its table, so there is no
    // single slot left to size — that guard is now the every-index-cites-its-
    // reader assertion, which covers ALL indexes rather than these four.
    // What still belongs here is the narrower claim: each index this suite plans
    // against is created, and created once. A duplicate under a second name
    // would be maintained on every write for nothing.
    for (COVERED_HOT_PATH_INDEXES) |entry| {
        var creates: usize = 0;
        for (schema.migrations) |m| {
            var lines = std.mem.splitScalar(u8, m.sql, '\n');
            while (lines.next()) |raw| {
                const line = std.mem.trim(u8, raw, " \t\r");
                if (!std.mem.startsWith(u8, line, "CREATE INDEX")) continue;
                if (std.mem.indexOf(u8, line, entry.name) != null) creates += 1;
            }
        }
        std.testing.expectEqual(@as(usize, 1), creates) catch |err| {
            std.debug.print(
                "index {s} created {d} times across schema/, want exactly 1\n",
                .{ entry.name, creates },
            );
            return err;
        };
    }
}

test "every index in the schema re-applies as a no-op" {
    // Idempotency by construction: a re-run against a provisioned database must
    // change nothing. Generalised from the retired slot 033 to the whole schema,
    // because the covered indexes no longer share a slot — and the broader claim
    // is the one the rebuild actually needs, since every slot is re-applied on
    // every boot. Reading through the registered migration array rather than the
    // files also proves each slot is wired into `schema/embed.zig`.
    var guarded: usize = 0;
    for (schema.migrations) |m| {
        var lines = std.mem.splitScalar(u8, m.sql, '\n');
        while (lines.next()) |raw| {
            // Comment lines discuss DDL without being it -- match statements only.
            const line = std.mem.trim(u8, raw, " \t\r");
            const is_index = std.mem.startsWith(u8, line, "CREATE INDEX") or
                std.mem.startsWith(u8, line, "CREATE UNIQUE INDEX");
            if (!is_index) continue;
            if (std.mem.indexOf(u8, line, "IF NOT EXISTS") == null) {
                std.debug.print("unguarded index in slot v{d}:\n{s}\n", .{ m.version, line });
                return error.MigrationNotIdempotent;
            }
            guarded += 1;
        }
    }
    try std.testing.expect(guarded >= COVERED_HOT_PATH_INDEXES.len);
}

test "memory composite has the right shape and serves the fleet filter" {
    // What our code controls, asserted cheaply. Two things:
    //   - the index indexes exactly (fleet_id, updated_at DESC, id DESC), so a
    //     column reorder or a dropped DESC fails here;
    //   - with sequential scans disabled the planner reaches for it to answer a
    //     fleet-scoped read, proving the index CAN serve that filter.
    //
    // What is deliberately NOT asserted is that the planner supplies the ordering
    // WITHOUT a Sort node, or that it prefers the index over a scan. Both are
    // scale-dependent planner behaviour, not properties of our code: the ordered
    // index scan only beats bitmap-scan-plus-sort once a fleet's rows are a small
    // enough slice of the table, and the crossover for the unbounded `listAll`
    // was measured to sit near 3% — reproducing it took a 40 000-row fixture per
    // run, for a fact PostgreSQL owns rather than we do. See the file header.
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer wipeMemory(db.conn);
    try seedMemory(db.conn, MEM_SEED_ROWS);

    try expectIndexShape(alloc, db.conn, "memory", "idx_memory_entries_fleet_id_updated_at_id", "fleet_id, updated_at DESC, id DESC");
    try expectServesFilter(alloc, db.conn,
        \\SELECT key, content, category
        \\FROM memory.memory_entries
        \\WHERE fleet_id = '0195b4ba-8d3a-7f13-8abc-0000000b0002'::uuid
        \\ORDER BY updated_at DESC, id DESC
        \\LIMIT 50
    , .{}, "idx_memory_entries_fleet_id_updated_at_id");
}

// Slot 040 — why the operator lease read earns two indexes, from the row budget
// rather than from taste.
//
// `fleet.runner_leases` gains one row per lease claim and a second for every
// reclaim of the same event. Nothing prunes it: no retention sweep touches the
// table, and the only deletes are tenant offboarding (`state/account_teardown`)
// and the runner/fleet `ON DELETE CASCADE`s. So a runner's row count is the
// integral of its whole working life, never a window.
//
// The rate follows the lease loop: each of `worker_count` workers runs
// lease → execute → report independently (`runner/daemon/config.zig`, default 1,
// `MAX_WORKER_COUNT` 64), and a lease outliving one `LEASE_TTL_MS` (30 s) renews
// rather than re-claims — so claims track completed events, not ticks. One
// worker turning a short event every ~30 s accrues ~2.9k rows/day, reaching the
// spec's motivating runner (4,021 leases) inside two days; at 64 workers the
// same arithmetic is ~184k rows/day.
//
// Both access paths below scaled with that number before this slot. Measured on
// one runner holding 5,000 leases across 5 fleets, the page read fell
// 15.94 ms → 0.405 ms: the full-history Seq Scan plus top-N heapsort became a
// 25-row Index Scan with no sort node, and the per-row reclaim probe became an
// Index Only Scan instead of ~997 index entries plus heap visits per returned
// row. The point is the shape, not the milliseconds — before, page cost grew
// with history; after, it is flat.
test "runner lease indexes have the right shape and serve the operator read" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer wipeLeases(db.conn);
    try seedLeases(db.conn, LEASE_SEED_ROWS);

    // The page: one runner's history, newest-first over the composite key.
    try expectIndexShape(alloc, db.conn, "fleet", LEASES_BY_RUNNER_INDEX, "runner_id, created_at DESC, id DESC");
    try expectServesFilter(alloc, db.conn,
        \\SELECT id::text, fleet_id::text, event_id
        \\FROM fleet.runner_leases
        \\WHERE runner_id = '0195b4ba-8d3a-7f13-8abc-0000000c0003'::uuid
        \\ORDER BY created_at DESC, id DESC
        \\LIMIT 50
    , .{}, LEASES_BY_RUNNER_INDEX);

    // The per-row is_reclaim probe: a lower-fencing sibling of the same
    // (fleet_id, event_id). Slot 033's fleet index cannot answer it — it
    // carries status, not event_id.
    try expectIndexShape(alloc, db.conn, "fleet", "idx_runner_leases_fleet_id_event_id_fencing_token", "fleet_id, event_id, fencing_token");
    try expectServesFilter(alloc, db.conn,
        \\SELECT 1
        \\FROM fleet.runner_leases p
        \\WHERE p.fleet_id = '0195b4ba-8d3a-7f13-8abc-0000000c0002'::uuid
        \\  AND p.event_id = 'idxprobe-evt-7'
        \\  AND p.fencing_token < 4000
    , .{}, "idx_runner_leases_fleet_id_event_id_fencing_token");
}

// ── Slots 043–045 — lifetime counters, the events read index, delete grants ──
//
// Slot 043 replaces the detail read's whole-history aggregation with a
// write-time tally table; slot 044 lets the type-filtered event feed stop
// walking the per-lease bulk; slot 045 grants the retention sweep its DELETEs.
// The counter maintenance itself is proven in
// `fleet/runner_counters_integration_test.zig`; here the catalog shape and the
// planner fitness of the read paths are pinned.

/// The runner whose seeded event history the slot 044 plan proofs read. Not
/// shared with the lease fixture: `runner_events` needs no fleet graph, so this
/// probe seeds only the runner row.
const RUNNER_EVENTS = "0195b4ba-8d3a-7f13-8abc-0000000d0001";
const EVENT_SEED_ROWS: i32 = 200;
/// Every fiftieth seeded event carries a rare lifecycle tag; the rest are the
/// per-lease bulk the filtered read must be able to skip.
const RARE_EVENT_EVERY: i32 = 50;
const EVENTS_INDEX = "idx_runner_events_runner_id_type_created_at_id";
const EVENT_PAGE_LIMIT: i64 = 25;
/// Any instant works for the detail plan probe — the plan's shape, not the
/// rows it would return, is under test.
const DETAIL_PROBE_NOW_MS: i64 = 1_750_000_000_000;
const SEQ_SCAN_LEASES_MARKER = "Seq Scan on runner_leases";

/// Mixed event history for one runner: the per-lease bulk plus a sprinkle of
/// rare lifecycle tags, mirroring the real table's distribution in miniature.
fn seedRunnerEvents(conn: *pg.Conn, rows: i32) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'idxprobe-events-host', 'idxprobe-token-events', 'dev_none',
        \\        'active', '[]'::jsonb, 0, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{RUNNER_EVENTS});
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_events
        \\  (id, runner_id, event_type, metadata, dedup_key, created_at)
        \\SELECT overlay(gen_random_uuid()::text placing '7' from 15 for 1)::uuid,
        \\       $1::uuid,
        \\       CASE WHEN g % $2::int = 0 THEN $3::text ELSE $4::text END,
        \\       '{}'::jsonb, NULL, $6::bigint + g
        \\FROM generate_series(1, $5::int) g
    , .{
        RUNNER_EVENTS,
        RARE_EVENT_EVERY,
        @tagName(protocol.RunnerEventType.runner_offline),
        @tagName(protocol.RunnerEventType.lease_acquired),
        rows,
        DETAIL_PROBE_NOW_MS,
    });
    _ = try conn.exec("ANALYZE fleet.runner_events", .{});
}

fn wipeRunnerEvents(conn: *pg.Conn) void {
    // The runner delete cascades the seeded events with it.
    _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_EVENTS}) catch |err|
        std.log.warn("events probe wipe ignored: {s}", .{@errorName(err)});
}

test "counter and retention slots are registered in the migration array" {
    // Reading through `schema.migrations` rather than the files proves each
    // slot is wired into `schema/embed.zig` — an unregistered slot never runs
    // at all, and would otherwise fail only at first deploy.
    // Retired slots 043-046 folded into the slots that own their tables:
    // the lifetime counters into 650, the runner-event read and
    // retention sweep indexes into 640, and the lease retention grants and
    // indexes into 610 and 620.
    try std.testing.expect(slotSql(610) != null);
    try std.testing.expect(slotSql(620) != null);
    try std.testing.expect(slotSql(640) != null);
    try std.testing.expect(slotSql(650) != null);
}

/// Slot 046's pair, named once because the shape assertions and the fitness
/// probes below both spell them.
const RETENTION_LEASES_INDEX = "idx_runner_leases_status_updated_at";
const RETENTION_EVENTS_INDEX = "idx_runner_events_type_created_at";
/// Below every seeded row's clock, so NOTHING qualifies — the steady-state
/// cycle, and the only bind that discriminates.
///
/// With a cutoff that matches everything, `LIMIT` short-circuits after the
/// first batch on any index that can reach the rows at all, so slot 018's
/// `(runner_id, status)` looks free and the planner takes it even though it
/// cannot bound the age predicate. That is exactly how the pre-046 sequential
/// scan looked acceptable. Proving emptiness is the work every cycle does once
/// the backlog drains, and only an index leading with the sweep's own predicate
/// can do it without walking the table.
const RETENTION_CUTOFF_PROBE: i64 = 1;
const RETENTION_BATCH_PROBE: i64 = 1000;

test "retention sweep deletes ride their own indexes, not a whole-table scan" {
    // The sweeper's predicates are status/tag + age across ALL runners, so no
    // runner-leading index can serve them: before slot 046 both DELETEs planned
    // as `Seq Scan`, hourly, on every replica. Asked with the production
    // statements verbatim and their real bind shapes — parameter arrays, which
    // is precisely why slot 046 ships full composites and not partial indexes
    // (spec Discovery C2).
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer wipeLeases(db.conn);
    defer wipeRunnerEvents(db.conn);
    try seedLeases(db.conn, LEASE_SEED_ROWS);
    wipeRunnerEvents(db.conn);
    try seedRunnerEvents(db.conn, EVENT_SEED_ROWS);

    try expectIndexShape(alloc, db.conn, "fleet", RETENTION_LEASES_INDEX, "status, updated_at");
    try expectIndexShape(alloc, db.conn, "fleet", RETENTION_EVENTS_INDEX, "event_type, created_at");

    const terminal = [_][]const u8{
        protocol.RUNNER_LEASE_STATUS_REPORTED,
        protocol.RUNNER_LEASE_STATUS_EXPIRED,
    };
    try expectServesFilter(alloc, db.conn, retention_sweeper.DELETE_TERMINAL_LEASES_BATCH, .{
        &terminal, RETENTION_CUTOFF_PROBE, RETENTION_BATCH_PROBE,
    }, RETENTION_LEASES_INDEX);

    try expectServesFilter(alloc, db.conn, retention_sweeper.DELETE_AGED_RUNNER_EVENTS_BATCH, .{
        &retention_sweeper.PER_LEASE_EVENT_TAGS, RETENTION_CUTOFF_PROBE, RETENTION_BATCH_PROBE,
    }, RETENTION_EVENTS_INDEX);

    // The abandoned-lease reaper searches the same way the lease delete does —
    // one status value plus an age bound — so it rides the same composite. It
    // runs on every replica every cycle like its siblings, which is why it is
    // pinned here rather than assumed to inherit their plan.
    // The reaper gets a floor, not a named index, and the difference is
    // deliberate. Its predicate selects `active` — live work plus the rare
    // stranded row, a small set — so either candidate index scans few entries;
    // both were observed on this fixture. Pinning one would encode a planner
    // preference this statement does not depend on, and would fail on a fixture
    // whose status mix differs rather than on a real regression. What must never
    // happen is the whole-table walk the deletes above are pinned against, and
    // that is what this asks.
    const abandoned = [_][]const u8{protocol.RUNNER_LEASE_STATUS_ACTIVE};
    try expectPlanOmits(alloc, db.conn, retention_sweeper.EXPIRE_ABANDONED_ACTIVE_LEASES_BATCH, .{
        &abandoned,
        RETENTION_CUTOFF_PROBE,
        protocol.RUNNER_LEASE_STATUS_EXPIRED,
        RETENTION_BATCH_PROBE,
        RETENTION_CUTOFF_PROBE,
    }, "Seq Scan on runner_leases");
}

test "lifetime counter table keys one bigint tally row per runner" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    // Column list and types read structurally from the catalog, in table
    // order — a rename, a dropped tally, or a type narrowing fails here.
    var q = PgQuery.from(try db.conn.query(
        \\SELECT string_agg(column_name || ' ' || data_type, ', ' ORDER BY ordinal_position)
        \\FROM information_schema.columns
        \\WHERE table_schema = 'fleet' AND table_name = 'runner_lifetime_counters'
    , .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.CounterTableMissing;
    const got = (try row.get(?[]const u8, 0)) orelse return error.CounterTableMissing;
    try std.testing.expectEqualStrings(
        "runner_id uuid, acquired bigint, succeeded bigint, " ++
            "failed bigint, expired bigint, created_at bigint, updated_at bigint",
        got,
    );
}

test "events composite has the right shape and serves the filtered feed" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    wipeRunnerEvents(db.conn);
    try seedRunnerEvents(db.conn, EVENT_SEED_ROWS);

    try std.testing.expectEqual(@as(i64, 1), try base.indexCount(db.conn, "fleet", EVENTS_INDEX));
    try expectIndexShape(alloc, db.conn, "fleet", EVENTS_INDEX, "runner_id, event_type, created_at DESC, id DESC");

    // The production statements verbatim, with the operator page's real bind
    // shape: a rare-tag text[] and open time bounds. Before this slot both
    // reads had to walk the per-lease bulk through the unfiltered composite.
    const rare_tags = [_][]const u8{
        @tagName(protocol.RunnerEventType.runner_offline),
        @tagName(protocol.RunnerEventType.runner_drained),
    };
    const open_bound: ?i64 = null;
    try expectServesFilter(alloc, db.conn, fleet_sql.SELECT_RUNNER_EVENT_COUNT, .{
        RUNNER_EVENTS, &rare_tags, open_bound, open_bound,
    }, EVENTS_INDEX);
    try expectServesFilter(alloc, db.conn, fleet_sql.SELECT_RUNNER_EVENT_KEYSET_FIRST, .{
        RUNNER_EVENTS, &rare_tags, open_bound, open_bound, EVENT_PAGE_LIMIT,
    }, EVENTS_INDEX);

    wipeRunnerEvents(db.conn);
}

test "runner detail read never forces a full lease-history scan" {
    // The detail statement's two lease legs — the live-now summary and (since
    // slot 043) nothing else — must both be index-servable. A Seq Scan on
    // runner_leases surviving the forced plan would mean the read still walks
    // the runner's whole history, the exact cost slot 043 exists to retire.
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    try seedLeases(db.conn, LEASE_SEED_ROWS);

    try expectPlanOmits(alloc, db.conn, operator_sql.SELECT_RUNNER_DETAIL, .{
        RUNNER_LEASE, protocol.RUNNER_LEASE_STATUS_ACTIVE, DETAIL_PROBE_NOW_MS,
    }, SEQ_SCAN_LEASES_MARKER);

    wipeLeases(db.conn);
}

test "the lease pager's exact total never walks the runner's whole history" {
    // The pager keeps an exact count rather than degrading to "load more", so
    // that count must not become the page's cost centre. Both binds of the one
    // production statement are asked: NULL (the operator's default view) and a
    // real workspace id (the §1 filter).
    //
    // Deliberately asked as "no full scan" rather than "uses index X". TWO
    // indexes legitimately cover `(runner_id, …)` here — slot 041's page
    // composite and slot 033's `(runner_id, status)` — and the planner picks
    // the narrower one for a bare count, correctly. Pinning either name would
    // fail the day the other becomes cheaper, which is a planner preference,
    // not a regression. What must never change is that some index serves it.
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer wipeLeases(db.conn);
    try seedLeases(db.conn, LEASE_SEED_ROWS);

    const unfiltered: ?[]const u8 = null;
    try expectPlanOmits(alloc, db.conn, operator_sql.SELECT_RUNNER_LEASE_TOTAL, .{
        RUNNER_LEASE, unfiltered, unfiltered,
    }, SEQ_SCAN_LEASES_MARKER);
    try expectPlanOmits(alloc, db.conn, operator_sql.SELECT_RUNNER_LEASE_TOTAL, .{
        RUNNER_LEASE, WS_LEASE, unfiltered,
    }, SEQ_SCAN_LEASES_MARKER);
    // The fleet filter joins `core.fleets` to match a name. The join must not
    // cost the lease side its index: the runner predicate still selects the rows
    // and the fleets probe rides the primary key.
    try expectPlanOmits(alloc, db.conn, operator_sql.SELECT_RUNNER_LEASE_TOTAL, .{
        RUNNER_LEASE, unfiltered, FLEET_LEASE,
    }, SEQ_SCAN_LEASES_MARKER);
}

/// The tenant charges keyset fixture. Same doctrine as the probes above: scans
/// are forced off, so a small table still forms the plan under test.
const LEDGER_SEED_ROWS: i32 = 200;
const WS_LEDGER = "0195b4ba-8d3a-7f13-8abc-0000000d0001";
const FLEET_LEDGER = "0195b4ba-8d3a-7f13-8abc-0000000d0002";
const LEDGER_EVENT_PREFIX = "idxprobe-ledger-";
const CHARGE_TYPE_STAGE = "stage";
/// Named once: both the fitness probe and the no-sort assertion spell it.
const LEDGER_BY_TENANT_INDEX = "idx_usage_ledger_tenant_id_created_at_id";
/// A `Sort` node here means the tiebreak was resolved after the seek instead of
/// by the index — the exact regression the trailing `id` column exists to stop.
const SORT_NODE_MARKER = "Sort";

/// The cursor branch of `listTelemetryForTenant`, imported rather than copied:
/// this suite asserts a property of the PRODUCTION query text, so a local
/// transcription that drifted from it would assert nothing.
const TENANT_CHARGES_KEYSET_PAGE = telemetry_store.SELECT_TENANT_CHARGES_PAGE_AFTER;
const TENANT_CHARGES_FIRST_PAGE = telemetry_store.SELECT_TENANT_CHARGES_PAGE_FIRST;

fn seedLedger(conn: *pg.Conn, rows: i32) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WS_LEDGER);
    try base.seedFleet(conn, FLEET_LEDGER, WS_LEDGER, "index-probe-ledger-fleet", "{}", "# SKILL");
    _ = try conn.exec(
        \\INSERT INTO billing.usage_ledger
        \\  (id, tenant_id, workspace_id, fleet_id, event_id, charge_type, posture,
        \\   model, credit_deducted_nanos, event_created_at, created_at, last_charged_at)
        \\SELECT overlay(gen_random_uuid()::text placing '7' from 15 for 1)::uuid,
        \\       $1::uuid, $2::uuid, $3::uuid, $4 || g, $5, 'metered',
        \\       'claude', 0, g, g, g
        \\FROM generate_series(1, $6::int) g
        \\ON CONFLICT DO NOTHING
    , .{ base.TEST_TENANT_ID, WS_LEDGER, FLEET_LEDGER, LEDGER_EVENT_PREFIX, CHARGE_TYPE_STAGE, rows });
}

fn wipeLedger(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM billing.usage_ledger WHERE event_id LIKE $1", .{LEDGER_EVENT_PREFIX ++ "%"}) catch |err|
        std.log.warn("ledger wipe ignored: {s}", .{@errorName(err)});
    base.teardownFleets(conn, WS_LEDGER);
    base.teardownWorkspace(conn, WS_LEDGER);
}

test "the tenant charges keyset pages without sorting, because its index carries the tiebreak" {
    // `schema/720_usage_ledger_indexes.sql` states this is "asserted against the
    // plan rather than against the index definition" — this is that assertion.
    // An index definition can carry the column and still be bypassed; only the
    // plan proves the page is one ordered scan.
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer wipeLedger(db.conn);
    try seedLedger(db.conn, LEDGER_SEED_ROWS);

    const boundary_created_at: i64 = LEDGER_SEED_ROWS;
    const boundary_id = "0195b4ba-8d3a-7f13-8abc-0000000dffff";
    const page_limit: i32 = 50;

    // The index serves the page…
    try expectServesFilter(alloc, db.conn, TENANT_CHARGES_KEYSET_PAGE, .{
        base.TEST_TENANT_ID, boundary_created_at, boundary_id, page_limit,
    }, LEDGER_BY_TENANT_INDEX);

    // …and resolves the ORDER BY itself, so no sort node appears. Were the
    // trailing `id` dropped from the index, the seek would still find the rows
    // and the plan would gain a Sort to break ties — passing the fitness check
    // above while paying a sort on every page.
    try expectPlanOmits(alloc, db.conn, TENANT_CHARGES_KEYSET_PAGE, .{
        base.TEST_TENANT_ID, boundary_created_at, boundary_id, page_limit,
    }, SORT_NODE_MARKER);

    // The FIRST page carries the same ORDER BY and the same exposure — and it is
    // the one every reader hits before they page at all, so it is the worse of
    // the two to leave unasserted.
    try expectServesFilter(alloc, db.conn, TENANT_CHARGES_FIRST_PAGE, .{
        base.TEST_TENANT_ID, page_limit,
    }, LEDGER_BY_TENANT_INDEX);
    try expectPlanOmits(alloc, db.conn, TENANT_CHARGES_FIRST_PAGE, .{
        base.TEST_TENANT_ID, page_limit,
    }, SORT_NODE_MARKER);
}

// ── The declared index roster (Dimension 5.1) ───────────────────────────────
//
// Every discretionary index in the schema, listed once. "Discretionary" means
// created by a bare `CREATE INDEX`: a constraint-backed index (primary key,
// unique constraint) is justified by the constraint that owns it and is excluded
// below, because dropping it is not a tuning decision.
//
// The roster exists because a dead index is silent. When a milestone deletes a
// reader — as this one deleted the operator accrual surface — the index that
// served it keeps being maintained on every write and nothing fails. On an
// unbounded, never-pruned table like `billing.usage_ledger` that is a permanent
// tax returning nothing. Requiring a new index to land here forces the author to
// state the query it serves in the slot's own comment, where a reviewer sees it.
//
// This asserts the roster, not the comments: a catalogue cannot read prose. What
// it guarantees is that no index appears or disappears WITHOUT a deliberate edit
// here, which is the enforceable half of "no index without a named reader".
const DeclaredIndex = struct { schema: []const u8, name: []const u8 };
const DECLARED_INDEXES = [_]DeclaredIndex{
    .{ .schema = "billing", .name = "idx_usage_ledger_fleet_id_workspace_id_last_charged_at" },
    .{ .schema = "billing", .name = "idx_usage_ledger_tenant_id_created_at_id" },
    .{ .schema = "billing", .name = "idx_usage_ledger_workspace_id" },
    .{ .schema = "core", .name = "idx_api_keys_tenant_id_active" },
    .{ .schema = "core", .name = "idx_connector_channels_fleet_id" },
    .{ .schema = "core", .name = "idx_connector_installs_workspace_id" },
    .{ .schema = "core", .name = "idx_fleet_approval_gates_action_id" },
    // Reader: the write-mint approval check — the newest gate for (fleet, event).
    .{ .schema = "core", .name = "idx_fleet_approval_gates_fleet_id_event_id" },
    .{ .schema = "core", .name = "idx_fleet_approval_gates_fleet_id_status" },
    .{ .schema = "core", .name = "idx_fleet_approval_gates_timeout_at_pending" },
    .{ .schema = "core", .name = "idx_fleet_approval_gates_workspace_id_status_created_at" },
    // Reader: the deploy-stamp webhook arm, keyed by the repair branch.
    .{ .schema = "core", .name = "idx_repair_pr_links_fleet_id_branch" },
    .{ .schema = "core", .name = "idx_fleet_events_fleet_id_created_at_event_id" },
    .{ .schema = "core", .name = "idx_fleet_events_fleet_id_resumes_event_id" },
    .{ .schema = "core", .name = "idx_fleet_events_workspace_id_created_at_event_id" },
    .{ .schema = "core", .name = "idx_fleet_schedules_fleet_id_created_at" },
    .{ .schema = "core", .name = "idx_fleets_required_tags_gin" },
    .{ .schema = "core", .name = "idx_fleets_workspace_id_created_at_id" },
    .{ .schema = "core", .name = "idx_memberships_user_id" },
    .{ .schema = "core", .name = "idx_tenant_fleet_library_workspace_id_created_at" },
    .{ .schema = "core", .name = "idx_tenant_model_entries_tenant_id_created_at" },
    .{ .schema = "core", .name = "idx_users_tenant_id" },
    .{ .schema = "core", .name = "idx_workspaces_tenant_id_created_at_id" },
    .{ .schema = "core", .name = "uq_users_oidc_subject" },
    .{ .schema = "core", .name = "uq_workspaces_tenant_id_name" },
    .{ .schema = "fleet", .name = "idx_runner_affinity_last_runner_id_leased_until" },
    .{ .schema = "fleet", .name = "idx_runner_events_runner_id_created_at_id" },
    .{ .schema = "fleet", .name = "idx_runner_events_runner_id_type_created_at_id" },
    .{ .schema = "fleet", .name = "idx_runner_events_type_created_at" },
    .{ .schema = "fleet", .name = "idx_runner_leases_fleet_id_event_id_fencing_token" },
    .{ .schema = "fleet", .name = "idx_runner_leases_fleet_id_status_fencing_token" },
    .{ .schema = "fleet", .name = "idx_runner_leases_runner_id_created_at_id" },
    .{ .schema = "fleet", .name = "idx_runner_leases_runner_id_status" },
    .{ .schema = "fleet", .name = "idx_runner_leases_status_updated_at" },
    .{ .schema = "fleet", .name = "uq_runner_events_runner_id_dedup_key_offline" },
    .{ .schema = "memory", .name = "idx_memory_entries_fleet_id_category_updated_at" },
    .{ .schema = "memory", .name = "idx_memory_entries_fleet_id_created_at_key" },
    .{ .schema = "memory", .name = "idx_memory_entries_fleet_id_updated_at_id" },
};

/// Discretionary indexes only: `pg_constraint.conindid` excludes the index a
/// primary key or unique CONSTRAINT owns. A `CREATE UNIQUE INDEX` that backs no
/// constraint stays in scope — it is still a tuning decision someone made.
const SELECT_DISCRETIONARY_INDEXES =
    \\SELECT n.nspname, c.relname
    \\FROM pg_class c
    \\JOIN pg_namespace n ON n.oid = c.relnamespace
    \\JOIN pg_index i ON i.indexrelid = c.oid
    \\WHERE c.relkind = 'i'
    \\  AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    \\  AND NOT EXISTS (SELECT 1 FROM pg_constraint con WHERE con.conindid = c.oid)
    \\ORDER BY n.nspname, c.relname
;

fn isDeclared(schema_name: []const u8, name: []const u8) bool {
    for (DECLARED_INDEXES) |declared| {
        if (std.mem.eql(u8, declared.schema, schema_name) and std.mem.eql(u8, declared.name, name)) return true;
    }
    return false;
}

test "every index in the schema is declared, and every declared index still exists" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    var live: std.ArrayList(DeclaredIndex) = .empty;
    defer {
        for (live.items) |item| {
            alloc.free(item.schema);
            alloc.free(item.name);
        }
        live.deinit(alloc);
    }

    var q = PgQuery.from(try db.conn.query(SELECT_DISCRETIONARY_INDEXES, .{}));
    defer q.deinit();
    while (try q.next()) |row| {
        const schema_name = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(schema_name);
        const name = try alloc.dupe(u8, try row.get([]const u8, 1));
        errdefer alloc.free(name);
        try live.append(alloc, .{ .schema = schema_name, .name = name });
    }

    // Direction 1 — an index the catalogue has that the roster does not. Either
    // it is new and its slot must state the query it serves, or its reader was
    // deleted and the index should have gone with it.
    var undeclared: usize = 0;
    for (live.items) |item| {
        if (!isDeclared(item.schema, item.name)) {
            undeclared += 1;
            std.debug.print(
                "\nUNDECLARED INDEX: {s}.{s} — add it to DECLARED_INDEXES with the reader its slot names, or drop it\n",
                .{ item.schema, item.name },
            );
        }
    }

    // Direction 2 — a roster entry the catalogue lacks. An index that vanished
    // without this list changing means a read lost its support silently.
    var missing: usize = 0;
    for (DECLARED_INDEXES) |declared| {
        var found = false;
        for (live.items) |item| {
            if (std.mem.eql(u8, declared.schema, item.schema) and std.mem.eql(u8, declared.name, item.name)) {
                found = true;
                break;
            }
        }
        if (!found) {
            missing += 1;
            std.debug.print(
                "\nMISSING INDEX: {s}.{s} is declared here but absent from the catalogue\n",
                .{ declared.schema, declared.name },
            );
        }
    }

    try std.testing.expectEqual(@as(usize, 0), undeclared);
    try std.testing.expectEqual(@as(usize, 0), missing);
}
