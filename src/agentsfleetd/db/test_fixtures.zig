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
const clock = common.clock;
const env = common.env;
const crypto_primitives = @import("../secrets/crypto_primitives.zig");
const pg = @import("pg");
const db = @import("pool.zig");

const IGNORED_ERROR_FMT = "ignored: {s}";

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
/// vault.secrets, integration_grants, fleet_keys, memory_entries, fleets,
/// and downstream telemetry / event rows.
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

/// Delete the shared test tenant's billing row so a test that asserts an exact
/// balance starts from a known-clean slate. TEST_TENANT_ID is shared across
/// suites, and `insertStarterGrant`/`provision` are idempotent (`ON CONFLICT DO
/// NOTHING`), so a balance a prior test left behind survives into this one: that
/// test's `teardownTenant` is silently FK-blocked by some leaked tenant-scoped row
/// (`core.workspaces`, `core.users`, … are NO ACTION references to `core.tenants`),
/// which strands the CASCADE-linked billing row with its dirtied balance. A later
/// billing test's idempotent grant then no-ops onto that balance (double-debit /
/// exhausted-carry under seed-randomized order). A direct DELETE here sidesteps the
/// whole FK chain, making balance assertions order-independent. Billing tests that
/// establish their own grant call this at setup; the platform edge tests re-grant
/// via `seedPlatformProvider`, so they are unaffected.
pub fn resetBilling(conn: *pg.Conn) void {
    _ = conn.exec(
        "DELETE FROM billing.tenant_billing WHERE tenant_id = $1::uuid",
        .{TEST_TENANT_ID},
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
// Tests that exercise the worker write path now hit the resolver per event,
// which needs a vault row reachable through `core.platform_llm_keys`. These
// helpers set up the minimum config that lets `tenant_provider.resolveActive
// Provider` succeed for the canonical TEST_TENANT_ID.

const TEST_PROVIDER_NAME = "test_fireworks";
const TEST_PROVIDER_API_KEY = "fw_test_stub_not_real";
/// The platform default's model + context cap the seeded platform_llm_keys row
/// carries. M100 sources these from the row (the old PLATFORM_DEFAULT_MODEL /
/// PLATFORM_DEFAULT_CAP_TOKENS constants were deleted), so a row without them
/// resolves to PlatformKeyMissing → tenant_resolve_failed. These values match
/// what the pre-M100 constants resolved to, keeping every lease-path test stable.
/// A matching core.model_caps row (zero token rates) is seeded first so the
/// fk_platform_llm_keys_model FK is satisfied. Zero rates keep the lease billed
/// run-fee-only (the cache resolves run-fee + 0 token nanos) — identical $ to the
/// pre-FK rate-cache-MISS behaviour, minus the latent lease-issue panic. A
/// token-tier billing assertion still seeds its OWN (provider, model) pair with
/// real rates (see service_token_splits_wire_test), not this default.
const TEST_PLATFORM_MODEL = "accounts/fireworks/models/kimi-k2.6";
const TEST_PLATFORM_CAP_TOKENS: i32 = 256_000;

/// Set ENCRYPTION_MASTER_KEY in the process env so vault.storeJson /
/// crypto_store.load can wrap/unwrap DEKs in tests. Idempotent; safe to
/// call from every test that touches the vault.
pub fn setTestEncryptionKey() void {
    crypto_primitives.setTestKek();
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
/// the platform credential resolves to. Used by the control-plane
/// integration tests to seed a known key so resolveFirstCredential
/// returns the exact bytes the assertion expects.
pub fn seedPlatformProviderWithKey(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    api_key: []const u8,
) !void {
    setTestEncryptionKey();

    const tenant_billing = @import("../state/tenant_billing.zig");
    const id_format = @import("../types/id_format.zig");
    const model_rate_cache = @import("../state/model_rate_cache.zig");

    // The catalogue row the platform default points at — required by
    // fk_platform_llm_keys_model. Zero token rates keep the lease run-fee-only
    // (cache resolves run-fee + 0 token nanos), matching the pre-FK MISS path.
    // Seeded BEFORE populate() so the cache picks it up (no lease-issue panic).
    const caps_uid = try id_format.generateFleetId(alloc);
    defer alloc.free(caps_uid);
    _ = try conn.exec(
        \\INSERT INTO core.model_caps
        \\  (uid, model_id, provider, context_cap_tokens,
        \\   input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok,
        \\   created_at_ms, updated_at_ms)
        \\VALUES ($1::uuid, $2, $3, $4, 0, 0, 0, $5, $5)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{ caps_uid, TEST_PLATFORM_MODEL, TEST_PROVIDER_NAME, TEST_PLATFORM_CAP_TOKENS, clock.nowMillis() });

    // Populate the process-global model rate cache from core.model_caps so
    // computeStageCharge() can resolve the platform default model. The
    // production server boots this from serve.zig; integration tests don't
    // hit that path. populate() owns the cache's process-lifetime memory
    // internally and deinits any prior cache before reseating.
    try model_rate_cache.populate(conn);

    // Vault credential at (workspace_id, TEST_PROVIDER_NAME).
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "provider", .{ .string = TEST_PROVIDER_NAME });
    try obj.put(alloc, "api_key", .{ .string = api_key });
    try storeVaultJson(alloc, conn, workspace_id, TEST_PROVIDER_NAME, .{ .object = obj });

    // platform_llm_keys row pointing at the seeded vault credential.
    const key_id = try id_format.generateFleetId(alloc);
    defer alloc.free(key_id);
    const now_ms: i64 = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.platform_llm_keys
        \\  (id, provider, source_workspace_id, model, context_cap_tokens, active, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, $4, $5, true, $6, $6)
        \\ON CONFLICT (provider) DO UPDATE
        \\SET source_workspace_id = EXCLUDED.source_workspace_id,
        \\    model = EXCLUDED.model,
        \\    context_cap_tokens = EXCLUDED.context_cap_tokens,
        \\    active = true,
        \\    updated_at = EXCLUDED.updated_at
    , .{ key_id, TEST_PROVIDER_NAME, workspace_id, TEST_PLATFORM_MODEL, TEST_PLATFORM_CAP_TOKENS, now_ms });

    // Starter grant — funds the receive + stage debits the writepath fires.
    try tenant_billing.insertStarterGrant(conn, TEST_TENANT_ID);
}

/// Counterpart to seedPlatformProvider — drops the platform key + vault row
/// for the workspace. Tenant_billing row is NOT touched here (some tests
/// override balance_nanos and want to control teardown explicitly).
pub fn teardownPlatformProvider(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec("DELETE FROM core.platform_llm_keys WHERE provider = $1", .{TEST_PROVIDER_NAME}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2", .{ workspace_id, TEST_PROVIDER_NAME }) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    resetBilling(conn);
    _ = conn.exec("DELETE FROM core.tenant_providers WHERE tenant_id = $1::uuid", .{TEST_TENANT_ID}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleet_execution_telemetry WHERE workspace_id = $1", .{workspace_id}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
}

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
