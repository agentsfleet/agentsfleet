/// test_fixtures_provider.zig — platform-provider + KEK fixtures, extracted
/// from test_fixtures.zig (which re-exports every pub here; import that).
///
/// Tests that exercise the worker write path hit the resolver per event,
/// which needs a vault row reachable through `core.platform_llm_keys`. These
/// helpers set up the minimum config that lets
/// `tenant_provider.resolveActiveProvider` succeed for the workspace's tenant.
const std = @import("std");
const common = @import("common");
const clock = common.clock;
const crypto_primitives = @import("../secrets/crypto_primitives.zig");
const pg = @import("pg");
const base = @import("test_fixtures.zig");

const IGNORED_ERROR_FMT = "ignored: {s}";

const TEST_PROVIDER_NAME = "test_fireworks";
const TEST_PROVIDER_API_KEY = "fw_test_stub_not_real";
/// The platform default's model + context cap the seeded platform_llm_keys row
/// carries. M100 sources these from the row (the old PLATFORM_DEFAULT_MODEL /
/// PLATFORM_DEFAULT_CAP_TOKENS constants were deleted), so a row without them
/// resolves to PlatformKeyMissing → tenant_resolve_failed. These values match
/// what the pre-M100 constants resolved to, keeping every lease-path test stable.
/// A matching core.model_library row (zero token rates) is seeded first so the
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
/// succeed under platform mode, and provision the workspace's tenant billing
/// row with the starter grant. Calls `setTestEncryptionKey` up front.
/// Idempotent (uses ON CONFLICT DO UPDATE / DO NOTHING).
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
        \\INSERT INTO core.model_library
        \\  (uid, model_id, provider, context_cap_tokens,
        \\   input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok,
        \\   created_at_ms, updated_at_ms)
        \\VALUES ($1::uuid, $2, $3, $4, 0, 0, 0, $5, $5)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{ caps_uid, TEST_PLATFORM_MODEL, TEST_PROVIDER_NAME, TEST_PLATFORM_CAP_TOKENS, clock.nowMillis() });

    // Populate the process-global model rate cache from core.model_library so
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
    try base.storeVaultJson(alloc, conn, workspace_id, TEST_PROVIDER_NAME, .{ .object = obj });

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
    // Granted to the workspace's OWNING tenant (not the shared TEST_TENANT_ID)
    // so per-suite-tenant callers fund the tenant the debit path resolves to.
    const tenant_id = try tenant_billing.resolveTenantFromWorkspace(conn, alloc, workspace_id);
    defer alloc.free(tenant_id);
    try tenant_billing.insertStarterGrant(conn, tenant_id);
}

/// SQL fragment resolving a workspace's owning tenant — keeps the teardown
/// helpers workspace-derived so they follow per-suite tenants automatically.
/// Run while the workspace row still exists (defer order: provider teardown
/// fires before the workspace teardown declared above it).
const TENANT_OF_WORKSPACE_SUBQ = "(SELECT tenant_id FROM core.workspaces WHERE workspace_id = $1::uuid)";

/// Counterpart to seedPlatformProvider — drops the platform key + vault row
/// for the workspace, resets the owning tenant's billing row (the seed's
/// starter grant landed on it), and clears provider + telemetry rows.
pub fn teardownPlatformProvider(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec("DELETE FROM core.platform_llm_keys WHERE provider = $1", .{TEST_PROVIDER_NAME}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2", .{ workspace_id, TEST_PROVIDER_NAME }) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM billing.tenant_billing WHERE tenant_id = " ++ TENANT_OF_WORKSPACE_SUBQ, .{workspace_id}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.tenant_providers WHERE tenant_id = " ++ TENANT_OF_WORKSPACE_SUBQ, .{workspace_id}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleet_execution_telemetry WHERE workspace_id = $1", .{workspace_id}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
}
