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
const pg = @import("pg");
const db = @import("pool.zig");

/// Canonical test tenant shared across all integration test UCs.
pub const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-000000000001";

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

/// Delete workspace. CASCADE removes everything that FKs `core.workspaces` —
/// vault.secrets, integration_grants, agent_keys, memory_entries, zombies,
/// and downstream telemetry / event rows.
pub fn teardownWorkspace(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec(
        "DELETE FROM core.workspaces WHERE workspace_id = $1::uuid",
        .{workspace_id},
    ) catch {};
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
    ) catch {};
}

// M10_001: seedSpec, seedRun, teardownRuns, teardownSpecs removed.
// Tables core.specs and core.runs were dropped in pipeline v1 removal.

// ── Zombie helpers (M1_001 — event loop integration tests) ─────────────

/// Insert a minimal zombie row. Workspace must exist. Idempotent.
pub fn seedZombie(
    conn: *pg.Conn,
    zombie_id: []const u8,
    workspace_id: []const u8,
    name: []const u8,
    config_json: []const u8,
    source_markdown: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.zombies
        \\  (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ zombie_id, workspace_id, name, source_markdown, config_json });
}

/// Insert a zombie session checkpoint. Zombie must exist. Idempotent.
pub fn seedZombieSession(
    conn: *pg.Conn,
    session_id: []const u8,
    zombie_id: []const u8,
    context_json: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.zombie_sessions
        \\  (id, zombie_id, context_json, checkpoint_at, created_at, updated_at)
        \\VALUES ($1, $2, $3, 0, 0, 0)
        \\ON CONFLICT (zombie_id) DO UPDATE
        \\  SET context_json = EXCLUDED.context_json
    , .{ session_id, zombie_id, context_json });
}

/// Delete zombies for a workspace. Cascades to zombie_sessions (FK).
pub fn teardownZombies(conn: *pg.Conn, workspace_id: []const u8) void {
    // Sessions first (FK to zombies), then zombies.
    _ = conn.exec(
        \\DELETE FROM core.zombie_sessions WHERE zombie_id IN
        \\  (SELECT id FROM core.zombies WHERE workspace_id = $1)
    , .{workspace_id}) catch {};
    _ = conn.exec(
        "DELETE FROM core.zombies WHERE workspace_id = $1",
        .{workspace_id},
    ) catch {};
}

// ── Provider + KEK fixtures ─────────────────────────────────────────────
// Tests that exercise the worker write path now hit the resolver per event,
// which needs a vault row reachable through `core.platform_llm_keys`. These
// helpers set up the minimum config that lets `tenant_provider.resolveActive
// Provider` succeed for the canonical TEST_TENANT_ID.

const ENCRYPTION_KEY_HEX_TEST = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const TEST_PROVIDER_NAME = "test_fireworks";
const TEST_PROVIDER_API_KEY = "fw_test_stub_not_real";

/// Set ENCRYPTION_MASTER_KEY in the process env so vault.storeJson /
/// crypto_store.load can wrap/unwrap DEKs in tests. Idempotent; safe to
/// call from every test that touches the vault.
pub fn setTestEncryptionKey() void {
    const c = @cImport(@cInclude("stdlib.h"));
    var z: [65]u8 = undefined;
    @memcpy(z[0..64], ENCRYPTION_KEY_HEX_TEST);
    z[64] = 0;
    _ = c.setenv("ENCRYPTION_MASTER_KEY", &z, 1);
}

/// Seed the minimum state for `tenant_provider.resolveActiveProvider` to
/// succeed under platform mode for TEST_TENANT_ID, and provision the
/// tenant_billing row with the starter grant. Calls `setTestEncryptionKey`
/// up front. Idempotent (uses ON CONFLICT DO UPDATE / DO NOTHING).
pub fn seedPlatformProvider(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
) !void {
    return seedPlatformProviderWithKey(alloc, conn, workspace_id, TEST_PROVIDER_API_KEY);
}

/// Variant of seedPlatformProvider that lets the caller pin the api_key
/// the platform credential resolves to. Used by the redaction-harness
/// pub/sub test (M42_002) to seed `SYNTHETIC_SECRET` so the worker's
/// resolveFirstCredential returns the exact bytes the redactor must
/// scrub from outbound frames.
pub fn seedPlatformProviderWithKey(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    api_key: []const u8,
) !void {
    setTestEncryptionKey();

    const vault = @import("../state/vault.zig");
    const tenant_billing = @import("../state/tenant_billing.zig");
    const id_format = @import("../types/id_format.zig");
    const model_rate_cache = @import("../state/model_rate_cache.zig");

    // Populate the process-global model rate cache from core.model_caps so
    // computeStageCharge() can resolve the platform default model. The
    // production server boots this from serve.zig; integration tests don't
    // hit that path. Use page_allocator: the cache is process-global and
    // outlives any single test, so a per-test allocator would be flagged as
    // leaked. populate() deinits any prior cache before reseating.
    try model_rate_cache.populate(std.heap.page_allocator, conn);

    // Vault credential at (workspace_id, TEST_PROVIDER_NAME).
    var obj = std.json.ObjectMap.init(alloc);
    defer obj.deinit();
    try obj.put("provider", .{ .string = TEST_PROVIDER_NAME });
    try obj.put("api_key", .{ .string = api_key });
    try vault.storeJson(alloc, conn, workspace_id, TEST_PROVIDER_NAME, .{ .object = obj });

    // platform_llm_keys row pointing at the seeded vault credential.
    const key_id = try id_format.generateZombieId(alloc);
    defer alloc.free(key_id);
    const now_ms: i64 = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO core.platform_llm_keys (id, provider, source_workspace_id, active, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, true, $4, $4)
        \\ON CONFLICT (provider) DO UPDATE
        \\SET source_workspace_id = EXCLUDED.source_workspace_id,
        \\    active = true,
        \\    updated_at = EXCLUDED.updated_at
    , .{ key_id, TEST_PROVIDER_NAME, workspace_id, now_ms });

    // Starter grant — funds the receive + stage debits the writepath fires.
    try tenant_billing.insertStarterGrant(conn, TEST_TENANT_ID);
}

/// Counterpart to seedPlatformProvider — drops the platform key + vault row
/// for the workspace. Tenant_billing row is NOT touched here (some tests
/// override balance_cents and want to control teardown explicitly).
pub fn teardownPlatformProvider(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec("DELETE FROM core.platform_llm_keys WHERE provider = $1", .{TEST_PROVIDER_NAME}) catch {};
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2", .{ workspace_id, TEST_PROVIDER_NAME }) catch {};
    _ = conn.exec("DELETE FROM billing.tenant_billing WHERE tenant_id = $1::uuid", .{TEST_TENANT_ID}) catch {};
    _ = conn.exec("DELETE FROM core.tenant_providers WHERE tenant_id = $1::uuid", .{TEST_TENANT_ID}) catch {};
    _ = conn.exec("DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1", .{workspace_id}) catch {};
}

// ── Shared DB connection ────────────────────────────────────────────────

/// Open a test DB connection. Returns null when TEST_DATABASE_URL / DATABASE_URL
/// is unset, causing the test to be skipped via `return error.SkipZigTest`.
///
/// Uses page_allocator for URL parse results so they outlive the pool. pg.Pool
/// stores shallow references to host/auth strings — if parsed via an arena that
/// is freed first, pool.release() crashes on non-idle connections.
pub fn openTestConn(alloc: std.mem.Allocator) !?struct { pool: *pg.Pool, conn: *pg.Conn } {
    const url = std.process.getEnvVarOwned(alloc, "TEST_DATABASE_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);

    const opts = try db.parseUrl(std.heap.page_allocator, url);
    const pool = try pg.Pool.init(alloc, opts);
    errdefer pool.deinit();
    const conn = try pool.acquire();
    return .{ .pool = pool, .conn = conn };
}
