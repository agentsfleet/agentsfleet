/// test_fixtures_provider.zig — platform-provider + KEK fixtures, extracted
/// from test_fixtures.zig (which re-exports every pub here; import that).
///
/// Tests that exercise the worker write path hit the resolver per event,
/// which needs a vault row reachable through `core.platform_provider_defaults`. These
/// helpers set up the minimum config that lets
/// `tenant_provider.resolveActiveProvider` succeed for the workspace's tenant.
const std = @import("std");
const common = @import("common");
const clock = common.clock;
const crypto_primitives = @import("../secrets/crypto_primitives.zig");
const id_format = @import("../types/id_format.zig");
const pg = @import("pg");
const base = @import("test_fixtures.zig");

const IGNORED_ERROR_FMT = "ignored: {s}";

/// Pub: integration asserts compare wire output against the exact identity
/// this module seeds (one constant, no re-typed literals in tests).
pub const TEST_PROVIDER_NAME = "test_fireworks";
const TEST_PROVIDER_API_KEY = "fw_test_stub_not_real";
/// The platform default's model + context cap the seeded platform_provider_defaults row
/// carries. M100 sources these from the row (the old PLATFORM_DEFAULT_MODEL /
/// PLATFORM_DEFAULT_CAP_TOKENS constants were deleted), so a row without them
/// resolves to PlatformKeyMissing → tenant_resolve_failed. These values match
/// what the pre-M100 constants resolved to, keeping every lease-path test stable.
/// A matching core.model_library row (zero token rates) is seeded first so the
/// fk_platform_provider_defaults_model FK is satisfied. Zero rates keep the lease billed
/// run-fee-only (the cache resolves run-fee + 0 token nanos) — identical $ to the
/// pre-FK rate-cache-MISS behaviour, minus the latent lease-issue panic. A
/// token-tier billing assertion still seeds its OWN (provider, model) pair with
/// real rates (see service_token_splits_wire_integration_test), not this default.
pub const TEST_PLATFORM_MODEL = "accounts/fireworks/models/kimi-k2.6";
pub const TEST_PLATFORM_CAP_TOKENS: i32 = 256_000;

/// A REAL catalogue row, for the tests that need a non-zero estimate.
///
/// `TEST_PLATFORM_MODEL` above prices at zero on purpose, so it can never size a
/// token floor — and a gate that must prove it BLOCKS a drained tenant needs one.
/// These are the live Fireworks rates for GLM 5.2, copied from the product seed
/// (`samples/fixtures/model-library/seed.sql`).
///
/// Seeded by `seedPricedModel` rather than read from the product catalogue,
/// which the migrations do NOT install: those rows exist only once
/// `model_library_seed_integration_test` has applied the fixture, so a test that
/// merely ASSUMED them would pass or fail on suite ordering.
pub const TEST_PRICED_PROVIDER = "fireworks";
pub const TEST_PRICED_MODEL = "accounts/fireworks/models/glm-5p2";
pub const TEST_PRICED_INPUT_NANOS_PER_MTOK: i64 = 1_400_000_000;
pub const TEST_PRICED_CACHED_INPUT_NANOS_PER_MTOK: i64 = 140_000_000;
pub const TEST_PRICED_OUTPUT_NANOS_PER_MTOK: i64 = 4_400_000_000;
pub const TEST_PRICED_CAP_TOKENS: i32 = 1_048_576;

/// Install the priced catalogue row above. Idempotent, and independent of every
/// other fixture — a caller needs only this to price a non-zero estimate.
pub fn seedPricedModel(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    const row_id = try id_format.generateFleetId(alloc);
    defer alloc.free(row_id);
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (id, model_id, provider, context_cap_tokens,
        \\   input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8)
        \\ON CONFLICT (provider, model_id) DO UPDATE
        \\SET input_nanos_per_mtok = EXCLUDED.input_nanos_per_mtok,
        \\    cached_input_nanos_per_mtok = EXCLUDED.cached_input_nanos_per_mtok,
        \\    output_nanos_per_mtok = EXCLUDED.output_nanos_per_mtok
    , .{
        row_id,
        TEST_PRICED_MODEL,
        TEST_PRICED_PROVIDER,
        TEST_PRICED_CAP_TOKENS,
        TEST_PRICED_INPUT_NANOS_PER_MTOK,
        TEST_PRICED_CACHED_INPUT_NANOS_PER_MTOK,
        TEST_PRICED_OUTPUT_NANOS_PER_MTOK,
        clock.nowMillis(),
    });
}

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

    // The catalogue row the platform default points at — required by
    // fk_platform_provider_defaults_model. Zero token rates keep the lease run-fee-only
    // (cache resolves run-fee + 0 token nanos), matching the pre-FK MISS path.
    // No cache warm needed: rates load on first use straight from this row.
    const caps_id = try id_format.generateFleetId(alloc);
    defer alloc.free(caps_id);
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (id, model_id, provider, context_cap_tokens,
        \\   input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, 0, 0, 0, $5, $5)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{ caps_id, TEST_PLATFORM_MODEL, TEST_PROVIDER_NAME, TEST_PLATFORM_CAP_TOKENS, clock.nowMillis() });

    // Vault credential at (workspace_id, TEST_PROVIDER_NAME).
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "provider", .{ .string = TEST_PROVIDER_NAME });
    try obj.put(alloc, "api_key", .{ .string = api_key });
    try base.storeVaultJson(alloc, conn, workspace_id, TEST_PROVIDER_NAME, .{ .object = obj });

    // platform_provider_defaults row pointing at the seeded vault credential.
    const now_ms: i64 = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.platform_provider_defaults
        \\  (provider, source_workspace_id, model, context_cap_tokens, active, created_at, updated_at)
        \\VALUES ($1, $2::uuid, $3, $4, true, $5, $5)
        \\ON CONFLICT (provider) DO UPDATE
        \\SET source_workspace_id = EXCLUDED.source_workspace_id,
        \\    model = EXCLUDED.model,
        \\    context_cap_tokens = EXCLUDED.context_cap_tokens,
        \\    active = true,
        \\    updated_at = EXCLUDED.updated_at
    , .{ TEST_PROVIDER_NAME, workspace_id, TEST_PLATFORM_MODEL, TEST_PLATFORM_CAP_TOKENS, now_ms });

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
const TENANT_OF_WORKSPACE_SUBQ = "(SELECT tenant_id FROM core.workspaces WHERE id = $1::uuid)";

/// Counterpart to seedPlatformProvider — drops the platform key + vault row
/// for the workspace, resets the owning tenant's billing row (the seed's
/// starter grant landed on it), and clears provider + telemetry rows.
pub fn teardownPlatformProvider(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec("DELETE FROM core.platform_provider_defaults WHERE provider = $1", .{TEST_PROVIDER_NAME}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2", .{ workspace_id, TEST_PROVIDER_NAME }) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM billing.tenant_wallet WHERE tenant_id = " ++ TENANT_OF_WORKSPACE_SUBQ, .{workspace_id}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.tenant_model_selection WHERE tenant_id = " ++ TENANT_OF_WORKSPACE_SUBQ, .{workspace_id}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM billing.usage_ledger WHERE workspace_id = $1::uuid", .{workspace_id}) catch |err| std.log.warn(IGNORED_ERROR_FMT, .{@errorName(err)});
}
