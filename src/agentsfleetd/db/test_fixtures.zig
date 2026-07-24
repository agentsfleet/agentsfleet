/// test_fixtures.zig — shared base helpers for all DB integration tests.
///
/// Usage pattern (every integration test):
///
///   const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
///   defer db_ctx.pool.deinit();
///   defer db_ctx.pool.release(db_ctx.conn);
///
///   try base.seedTenant(db_ctx.conn);
///   defer base.teardownTenant(db_ctx.conn);
///   try base.seedWorkspace(db_ctx.conn, workspace_id);
///   defer base.teardownWorkspace(db_ctx.conn, workspace_id);
///
/// Teardown order matters — workspace must be deleted before tenant
/// (FK: workspaces.tenant_id → tenants.tenant_id, NO ACTION).
/// Deleting the workspace cascades most child tables automatically; see the
/// FK cascade map in docs/spec for the full list.
///
/// All seed inserts use ON CONFLICT DO NOTHING — safe to call multiple times
/// even if a prior test run panicked before teardown.
const std = @import("std");
const common = @import("common");
const env = common.env;
const pg = @import("pg");
const db = @import("pool.zig");
const PgQuery = @import("pg_query.zig").PgQuery;

const IGNORED_ERROR_FMT = "ignored: {s}";

/// Canonical test tenant shared across all integration test UCs.
pub const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-000000000001";

/// Fault-injection CHECK constraints the reclaim suite installs to force a
/// reclaim/release error (`event_lifecycle_reclaim_integration_test.zig`). Named
/// here so `dropInjectedFaultConstraints` — invoked from every test-DB conn
/// opener — can clear them for ANY later test, not just the reclaim suite's own.
pub const RECLAIM_FAIL_CONSTRAINT = "ck_test_reclaim_fail";
pub const RELEASE_FAIL_CONSTRAINT = "ck_test_release_fail";

/// Drop the reclaim suite's fault-injection constraints if a prior run leaked
/// them. Called from `openTestConn` and `TestHarness.start` — the shared conn
/// entry points every fleet DB test passes through — so a run killed between an
/// `arm*` ADD and its deferred DROP cannot wedge the shared test DB for
/// unrelated fleet tests (a kill skips deferred teardown, so cleanup must live
/// at the next run's conn acquisition, before it touches those tables). A
/// `DROP ... IF EXISTS` on an unconstrained table is a no-op.
pub fn dropInjectedFaultConstraints(conn: *pg.Conn) void {
    _ = conn.exec("ALTER TABLE fleet.runner_leases DROP CONSTRAINT IF EXISTS " ++ RECLAIM_FAIL_CONSTRAINT, .{}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("ALTER TABLE fleet.runner_affinity DROP CONSTRAINT IF EXISTS " ++ RELEASE_FAIL_CONSTRAINT, .{}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
}

/// Insert the canonical test tenant. Idempotent via ON CONFLICT DO NOTHING.
pub fn seedTenant(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO core.tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'scrooge-mcduck', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{TEST_TENANT_ID});
}

/// Insert a test workspace. Requires seedTenant to have run first.
/// Idempotent via ON CONFLICT DO NOTHING.
pub fn seedWorkspace(conn: *pg.Conn, workspace_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO core.workspaces
        \\  (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, 0)
        \\ON CONFLICT DO NOTHING
    , .{ workspace_id, TEST_TENANT_ID });
}

/// Delete workspace. `core.fleets` is NOT cascade-backed on `workspace_id` — a
/// lingering fleet (and, through the fleet_id FK, its runner_leases /
/// runner_affinity cascade children) blocks this DELETE, so call `teardownFleets`
/// first. Other cascade-backed workspace children drop with it.
pub fn teardownWorkspace(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec(
        "DELETE FROM core.workspaces WHERE workspace_id = $1::uuid",
        .{workspace_id},
    ) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
}

/// Delete the canonical test tenant.
/// Call only after all workspaces for this tenant have been removed.
pub fn teardownTenant(conn: *pg.Conn) void {
    teardownTenantById(conn, TEST_TENANT_ID);
}

// ── Multi-tenant helpers ────────────────────────────────────────────────

/// Insert a tenant with a custom ID. Idempotent via ON CONFLICT DO NOTHING.
pub fn seedTenantById(conn: *pg.Conn, tenant_id: []const u8, name: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO core.tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, $2, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ tenant_id, name });
}

/// Insert a workspace under a specific tenant. Idempotent.
pub fn seedWorkspaceWithTenant(conn: *pg.Conn, workspace_id: []const u8, tenant_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO core.workspaces
        \\  (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, 0)
        \\ON CONFLICT DO NOTHING
    , .{ workspace_id, tenant_id });
}

/// Insert a workspace with an explicit `created_by`. Used by tests that
/// exercise owner-override / creator-check logic (isWorkspaceCreator).
/// Name is left NULL — the partial unique index `uq_workspaces_tenant_name`
/// only fires on non-null names, so the row slots in without touching it.
pub fn seedWorkspaceWithCreator(
    conn: *pg.Conn,
    workspace_id: []const u8,
    tenant_id: []const u8,
    created_by: ?[]const u8,
) !void {
    // UPSERT rather than ON CONFLICT DO NOTHING: if a prior test's cleanup
    // failed silently (FK-blocked delete for a workspace with dependent rows),
    // ON CONFLICT DO NOTHING would preserve the stale created_by and the
    // creator-scoping assertions would read the wrong owner. UPDATE on
    // conflict makes the seed idempotent in the "latest seed wins" sense.
    _ = try conn.exec(
        \\INSERT INTO core.workspaces
        \\  (workspace_id, tenant_id, name, created_by, created_at)
        \\VALUES ($1::uuid, $2::uuid, NULL, $3, 0)
        \\ON CONFLICT (workspace_id) DO UPDATE SET created_by = EXCLUDED.created_by
    , .{ workspace_id, tenant_id, created_by });
}

/// Delete a tenant by custom ID.
pub fn teardownTenantById(conn: *pg.Conn, tenant_id: []const u8) void {
    _ = conn.exec(
        "DELETE FROM core.tenants WHERE tenant_id = $1::uuid",
        .{tenant_id},
    ) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
}

/// Delete a tenant's billing row so a test that asserts an exact balance starts
/// from a known-clean slate. Grants (`insertStarterGrant`/`provision`) are
/// idempotent (`ON CONFLICT DO NOTHING`), so a balance left behind — by an
/// earlier grant in the same suite, or by a crashed run whose FK-blocked tenant
/// teardown stranded the CASCADE-linked billing row — silently survives into
/// the next grant (double-debit / exhausted-carry under seed-randomized order).
/// A direct DELETE sidesteps the FK chain, making balance assertions
/// order-independent. Call before ANY grant/provision whose balance is asserted.
pub fn resetBillingFor(conn: *pg.Conn, tenant_id: []const u8) void {
    _ = conn.exec(
        "DELETE FROM billing.tenant_billing WHERE tenant_id = $1::uuid",
        .{tenant_id},
    ) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
}

// M10_001: seedSpec, seedRun, teardownRuns, teardownSpecs removed.
// Tables core.specs and core.runs were dropped in pipeline v1 removal.

// ── Fleet helpers (event loop integration tests) ─────────────

/// Insert a minimal fleet row. Workspace must exist. Idempotent.
pub fn seedFleet(
    conn: *pg.Conn,
    fleet_id: []const u8,
    workspace_id: []const u8,
    name: []const u8,
    config_json: []const u8,
    source_markdown: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ fleet_id, workspace_id, name, source_markdown, config_json });
}

/// Insert a minimal fleet row in an explicit status (e.g. `installing` for the
/// install-worker lifecycle test — the guarded installing→active flip matches
/// only that status). Workspace must exist. Idempotent. The SQL lives here so
/// the fleet-seed shape keeps one home instead of leaking into test bodies.
pub fn seedFleetWithStatus(
    conn: *pg.Conn,
    fleet_id: []const u8,
    workspace_id: []const u8,
    name: []const u8,
    status: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, $3, '', '{}', $4, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ fleet_id, workspace_id, name, status });
}

/// Read a fleet row's status into an owned copy (caller frees with `alloc`).
/// Null when the row does not exist.
pub fn fleetStatusOwned(conn: *pg.Conn, alloc: std.mem.Allocator, fleet_id: []const u8) !?[]u8 {
    var q = PgQuery.from(try conn.query(
        "SELECT status FROM core.fleets WHERE id = $1::uuid",
        .{fleet_id},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    return try alloc.dupe(u8, try row.get([]const u8, 0));
}

/// Insert a fleet session checkpoint. Fleet must exist. Idempotent.
pub fn seedFleetSession(
    conn: *pg.Conn,
    session_id: []const u8,
    fleet_id: []const u8,
    context_json: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleet_sessions
        \\  (id, fleet_id, context_json, checkpoint_at, created_at, updated_at)
        \\VALUES ($1, $2, $3, 0, 0, 0)
        \\ON CONFLICT (fleet_id) DO UPDATE
        \\  SET context_json = EXCLUDED.context_json
    , .{ session_id, fleet_id, context_json });
}

/// Delete fleets for a workspace. Cascades to fleet_sessions (FK).
pub fn teardownFleets(conn: *pg.Conn, workspace_id: []const u8) void {
    // Sessions first (FK to fleets), then fleets.
    _ = conn.exec(
        \\DELETE FROM core.fleet_sessions s
        \\USING core.fleets z
        \\WHERE s.fleet_id = z.id
        \\  AND z.workspace_id = $1
    , .{workspace_id}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec(
        "DELETE FROM core.fleets WHERE workspace_id = $1",
        .{workspace_id},
    ) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
}

// ── Provider + KEK fixtures ─────────────────────────────────────────────
// Extracted to test_fixtures_provider.zig (file-length split); re-exported
// here so every existing `base.seedPlatformProvider(...)` call site keeps
// working — this module stays the fixtures' public API.

const provider_fixtures = @import("test_fixtures_provider.zig");
pub const setTestEncryptionKey = provider_fixtures.setTestEncryptionKey;
pub const seedPlatformProvider = provider_fixtures.seedPlatformProvider;
pub const seedPlatformProviderWithKey = provider_fixtures.seedPlatformProviderWithKey;
pub const teardownPlatformProvider = provider_fixtures.teardownPlatformProvider;
pub const TEST_PROVIDER_NAME = provider_fixtures.TEST_PROVIDER_NAME;
pub const TEST_PLATFORM_MODEL = provider_fixtures.TEST_PLATFORM_MODEL;
pub const TEST_PLATFORM_CAP_TOKENS = provider_fixtures.TEST_PLATFORM_CAP_TOKENS;

// ── Shared DB connection ────────────────────────────────────────────────

/// Open a test DB connection. Returns null when TEST_DATABASE_URL / DATABASE_URL
/// is unset, causing the test to be skipped via `return error.SkipZigTest`.
///
/// Uses page_allocator for URL parse results so they outlive the pool. pg.Pool
/// stores shallow references to host/auth strings — if parsed via an arena that
/// is freed first, pool.release() crashes on non-idle connections.
pub fn openTestConn(alloc: std.mem.Allocator) !?struct { pool: *pg.Pool, conn: *pg.Conn } {
    const url = env.testLiveValue("TEST_DATABASE_URL") orelse
        env.testLiveValue("DATABASE_URL") orelse return null;

    const opts = try db.parseUrl(std.heap.page_allocator, url);
    const pool = try pg.Pool.init(common.globalIo(), alloc, opts);

    errdefer pool.deinit();
    const conn = try pool.acquire();
    dropInjectedFaultConstraints(conn);
    return .{ .pool = pool, .conn = conn };
}

/// Validate + canonical-stringify a JSON object and persist it through the
/// production vault write path. Test-only composition — the production
/// writer (the credentials handler) stringifies once itself and calls
/// `vault.storeJsonPlaintext` directly.
pub fn storeVaultJson(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    value: std.json.Value,
) !void {
    const vault = @import("../state/vault.zig");
    try vault.validateObject(value);
    const plaintext = try std.json.Stringify.valueAlloc(alloc, value, .{});
    defer alloc.free(plaintext);
    try vault.storeJsonPlaintext(alloc, conn, workspace_id, key_name, plaintext);
}

// ── Index-catalog assertions (shared by the db/index_*_integration_test suites) ──
// These read index shape and existence STRUCTURALLY from the system catalog,
// schema-qualified. The earlier per-suite copies keyed on `pg_indexes.indexname`
// alone (not unique across schemas) and parsed the final parenthesised segment
// of the rendered `indexdef` (skewed by INCLUDE columns or a parenthesised
// predicate). One catalog-truth helper each, used by every suite, closes both.

/// Canonical `"col1, col2 DESC"` for `schema.name`'s KEY columns, in index
/// order, read from `pg_index`/`pg_attribute` — INCLUDE columns (`indnkeyatts`
/// bounds the key), a `WHERE` predicate (`indpred`, not `indkey`), and a
/// same-named index in another schema (`nspname` filter) cannot skew it.
/// Ascending is implicit; only DESC is suffixed, matching how the suites spell
/// their expected shape. Caller owns the returned slice; `error.IndexMissing`
/// when no such index exists in `schema`.
pub fn indexKeyColumns(alloc: std.mem.Allocator, conn: *pg.Conn, schema: []const u8, name: []const u8) ![]u8 {
    var q = PgQuery.from(try conn.query(
        \\SELECT a.attname, (idx.indoption[k.i] & 1) = 1 AS is_desc
        \\FROM pg_index idx
        \\JOIN pg_class ic ON ic.oid = idx.indexrelid
        \\JOIN pg_namespace n ON n.oid = ic.relnamespace
        \\CROSS JOIN generate_subscripts(idx.indkey, 1) AS k(i)
        \\JOIN pg_attribute a ON a.attrelid = idx.indrelid AND a.attnum = idx.indkey[k.i]
        \\WHERE ic.relname = $1 AND n.nspname = $2 AND k.i < idx.indnkeyatts
        \\ORDER BY k.i
    , .{ name, schema }));
    defer q.deinit();
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    var count: usize = 0;
    while (try q.next()) |row| : (count += 1) {
        if (count > 0) try out.appendSlice(alloc, ", ");
        try out.appendSlice(alloc, try row.get([]const u8, 0));
        if (try row.get(bool, 1)) try out.appendSlice(alloc, " DESC");
    }
    if (count == 0) return error.IndexMissing;
    return out.toOwnedSlice(alloc);
}

/// Count of indexes named `name` in `schema`. Schema-qualified because
/// `pg_indexes.indexname` is not unique across schemas — a suite asserting
/// "applied exactly once" or "removed" needs the count scoped to the one schema
/// that owns the object, or a same-named index elsewhere flips the result.
pub fn indexCount(conn: *pg.Conn, schema: []const u8, name: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::bigint FROM pg_indexes WHERE indexname = $1 AND schemaname = $2",
        .{ name, schema },
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.DbRowShape;
    return try row.get(i64, 0);
}
