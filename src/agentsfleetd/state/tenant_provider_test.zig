// Integration tests for tenant_provider.zig.
//
// Cover: Mode + ResolvedProvider invariants (no database (DB)), and the
// resolver + upsert + delete entry points (real DB + vault). Skips when no DB.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");

const tenant_provider = @import("tenant_provider.zig");
const crypto_primitives = @import("../secrets/crypto_primitives.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

/// Shared test allocator for tenant provider fixture files.
pub const ALLOC = std.testing.allocator;

/// Workspace id used by resolve-active-provider tests.
pub const WS_TP_RESOLVE = "0195b4ba-8d3a-7f13-8abc-aa2000000001";
/// Workspace id used by upsert tests.
pub const WS_TP_UPSERT = "0195b4ba-8d3a-7f13-8abc-aa2000000002";
/// Workspace id used by self-managed credential tests.
pub const WS_TP_SELF_MANAGED = "0195b4ba-8d3a-7f13-8abc-aa2000000003";

/// Provider name scoped to this test group.
pub const TP_TEST_PROVIDER = "tenant_provider_test_fireworks";
/// Platform default model fixture value.
pub const TP_DEFAULT_MODEL = "tp-test-default-model";
/// Platform default context cap fixture value.
pub const TP_DEFAULT_CAP: u32 = 192_000;

/// Configure deterministic test encryption.
pub fn setEncryptionKey() void {
    crypto_primitives.setTestKek();
}

/// Remove tenant provider fixture rows for one workspace.
pub fn cleanupTeardown(conn: *pg.Conn, ws_id: []const u8) void {
    _ = conn.exec("DELETE FROM core.tenant_model_selection WHERE tenant_id = $1::uuid", .{uc1.TENANT_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.platform_provider_defaults WHERE source_workspace_id = $1::uuid", .{ws_id}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    // After platform_provider_defaults (the FK referrer) is gone, the catalogue row is free to drop.
    _ = conn.exec("DELETE FROM core.model_library WHERE provider = $1", .{TP_TEST_PROVIDER}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1", .{ws_id}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    uc1.teardown(conn, ws_id);
}

/// Seed a platform default row plus matching vault secret.
pub fn seedPlatformLlmKey(conn: *pg.Conn, alloc: std.mem.Allocator, ws_id: []const u8, provider: []const u8, api_key: []const u8) !void {
    // Vault row at (ws_id, provider) — same storage path self-managed uses.
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "provider", .{ .string = provider });
    try obj.put(alloc, "api_key", .{ .string = api_key });
    const value = std.json.Value{ .object = obj };
    try base.storeVaultJson(alloc, conn, ws_id, provider, value);

    // Generate a UUIDv7 (required by ck_platform_provider_defaults_uid_uuidv7).
    const id_format = @import("../types/id_format.zig");
    const key_id = try id_format.generateFleetId(alloc);
    defer alloc.free(key_id);
    const now_ms: i64 = clock.nowMillis();
    // Catalogue row the default points at — fk_platform_provider_defaults_model requires it.
    const caps_uid = try id_format.generateFleetId(alloc);
    defer alloc.free(caps_uid);
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (uid, model_id, provider, context_cap_tokens,
        \\   input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok,
        \\   created_at_ms, updated_at_ms)
        \\VALUES ($1::uuid, $2, $3, $4, 0, 0, 0, $5, $5)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{ caps_uid, TP_DEFAULT_MODEL, provider, @as(i32, @intCast(TP_DEFAULT_CAP)), now_ms });
    _ = try conn.exec(
        \\INSERT INTO core.platform_provider_defaults (id, provider, source_workspace_id, model, context_cap_tokens, active, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, $5, $6, true, $4, $4)
        \\ON CONFLICT (provider) DO UPDATE
        \\SET source_workspace_id = EXCLUDED.source_workspace_id, model = EXCLUDED.model,
        \\    context_cap_tokens = EXCLUDED.context_cap_tokens, active = true, updated_at = EXCLUDED.updated_at
    , .{ key_id, provider, ws_id, now_ms, TP_DEFAULT_MODEL, @as(i32, @intCast(TP_DEFAULT_CAP)) });
}

/// Seed a self-managed vault credential row.
pub fn seedSelfManagedCredential(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    ws_id: []const u8,
    name: []const u8,
    provider: []const u8,
    api_key: []const u8,
    model: []const u8,
) !void {
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "provider", .{ .string = provider });
    try obj.put(alloc, "api_key", .{ .string = api_key });
    try obj.put(alloc, "model", .{ .string = model });
    const value = std.json.Value{ .object = obj };
    try base.storeVaultJson(alloc, conn, ws_id, name, value);
}

/// Seed a dashboard-style fleet credential row.
pub fn seedFleetCredential(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    ws_id: []const u8,
    name: []const u8,
    provider: []const u8,
    api_key: []const u8,
    model: []const u8,
) !void {
    try seedSelfManagedCredential(conn, alloc, ws_id, name, provider, api_key, model);
}

// ── base_url validation (pure — no DB) ──────────────────────────────────────
// validateSecretEndpoint is the resolver's parse-boundary Server-Side Request
// Forgery (SSRF) gate; these
// drive every provider⇔base_url branch the Dimensions name without a DB.

/// OpenAI-compatible provider fixture value.
pub const COMPAT = tenant_provider.OPENAI_COMPATIBLE_PROVIDER;

test "test_resolver_extracts_base_url" {
    // 6.1: openai-compatible + valid https base_url → carried through (the bare
    // validated URL is returned for the resolver to dupe onto the credential).
    const got = try tenant_provider.validateSecretEndpoint(COMPAT, "https://api.openrouter.ai/v1");
    try std.testing.expectEqualStrings("https://api.openrouter.ai/v1", got.?);
    // A self-hosted gateway hostname is equally fine.
    const gw = try tenant_provider.validateSecretEndpoint(COMPAT, "https://vllm.corp.internal:8443/v1");
    try std.testing.expectEqualStrings("https://vllm.corp.internal:8443/v1", gw.?);
}

test "test_resolver_rejects_non_https" {
    // 6.2: http / garbage scheme → typed invalid-endpoint error, no resolution.
    try std.testing.expectError(
        tenant_provider.ResolveError.SecretEndpointInvalid,
        tenant_provider.validateSecretEndpoint(COMPAT, "http://api.example.com/v1"),
    );
    try std.testing.expectError(
        tenant_provider.ResolveError.SecretEndpointInvalid,
        tenant_provider.validateSecretEndpoint(COMPAT, "ftp://api.example.com"),
    );
    try std.testing.expectError(
        tenant_provider.ResolveError.SecretEndpointInvalid,
        tenant_provider.validateSecretEndpoint(COMPAT, "not a url"),
    );
    // openai-compatible with NO base_url is the mirror mismatch — also rejected.
    try std.testing.expectError(
        tenant_provider.ResolveError.SecretEndpointInvalid,
        tenant_provider.validateSecretEndpoint(COMPAT, null),
    );
}

test "test_resolver_blocks_ssrf_hosts" {
    // Every SSRF-unsafe host the Dimension enumerates is blocked before any
    // run. Asserts all of: 127.0.0.1, 10.x, 172.16.x, 192.168.x, the cloud
    // metadata IP, ::1, and 0.0.0.0.
    const blocked = [_][]const u8{
        "https://127.0.0.1/v1",
        "https://10.1.2.3/v1",
        "https://172.16.0.9/v1",
        "https://192.168.1.1/v1",
        "https://169.254.169.254/latest/meta-data",
        "https://[::1]/v1",
        "https://0.0.0.0/v1",
    };
    for (blocked) |url| {
        try std.testing.expectError(
            tenant_provider.ResolveError.SecretEndpointInvalid,
            tenant_provider.validateSecretEndpoint(COMPAT, url),
        );
    }
}

test "test_resolver_named_provider_unchanged" {
    // 6.4: a named-provider credential with NO base_url resolves exactly as today
    // (null endpoint, no error) — the existing path is not regressed.
    try std.testing.expectEqual(
        @as(?[]const u8, null),
        try tenant_provider.validateSecretEndpoint("fireworks", null),
    );
    // …and a named provider must NOT smuggle a base_url (mismatch → rejected),
    // so the openai-compatible path is the only door to a custom host.
    try std.testing.expectError(
        tenant_provider.ResolveError.SecretEndpointInvalid,
        tenant_provider.validateSecretEndpoint("fireworks", "https://evil.example.com/v1"),
    );
}

// ── Mode enum + ResolvedProvider invariants ────────────────────────────────

test "Mode label round-trips for both variants" {
    try std.testing.expectEqualStrings("platform", tenant_provider.Mode.platform.label());
    try std.testing.expectEqualStrings("self_managed", tenant_provider.Mode.self_managed.label());
}

test "ResolvedProvider.deinit completes without leaking" {
    const alloc = std.testing.allocator;
    var rp = tenant_provider.ResolvedProvider{
        .mode = .self_managed,
        .provider = try alloc.dupe(u8, TP_TEST_PROVIDER),
        .api_key = try alloc.dupe(u8, "fw_LIVE_secret_xyz"),
        .model = try alloc.dupe(u8, "accounts/fireworks/models/kimi-k2.6"),
        .context_cap_tokens = 256_000,
    };
    rp.deinit(alloc);
    // testing.allocator detects any un-freed bytes. The api_key zero-on-free is
    // enforced by std.crypto.secureZero at the call site in deinit; reading the
    // freed slice would be a use-after-free, so code review verifies secureZero.
}

// ── resolveActiveProvider — synthesised platform default ───────────────────

test "resolveActiveProvider with no row returns synthesised platform default" {
    setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_TP_RESOLVE);
    defer cleanupTeardown(db_ctx.conn, WS_TP_RESOLVE);

    try seedPlatformLlmKey(db_ctx.conn, ALLOC, WS_TP_RESOLVE, TP_TEST_PROVIDER, "fw_PLATFORM_xyz");

    var rp = try tenant_provider.resolveActiveProvider(ALLOC, db_ctx.conn, uc1.TENANT_ID);
    defer rp.deinit(ALLOC);

    try std.testing.expectEqual(tenant_provider.Mode.platform, rp.mode);
    try std.testing.expectEqualStrings(TP_TEST_PROVIDER, rp.provider);
    try std.testing.expectEqualStrings("fw_PLATFORM_xyz", rp.api_key);
    try std.testing.expectEqualStrings(TP_DEFAULT_MODEL, rp.model);
    try std.testing.expectEqual(TP_DEFAULT_CAP, rp.context_cap_tokens);
}

test "resolveActiveProvider with explicit platform row returns same shape as synth" {
    setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_TP_RESOLVE);
    defer cleanupTeardown(db_ctx.conn, WS_TP_RESOLVE);

    try seedPlatformLlmKey(db_ctx.conn, ALLOC, WS_TP_RESOLVE, TP_TEST_PROVIDER, "fw_PLATFORM_xyz");
    try tenant_provider.upsertPlatform(ALLOC, db_ctx.conn, uc1.TENANT_ID);

    var rp = try tenant_provider.resolveActiveProvider(ALLOC, db_ctx.conn, uc1.TENANT_ID);
    defer rp.deinit(ALLOC);

    try std.testing.expectEqual(tenant_provider.Mode.platform, rp.mode);
    try std.testing.expectEqualStrings(TP_TEST_PROVIDER, rp.provider);
    try std.testing.expectEqualStrings(TP_DEFAULT_MODEL, rp.model);
    try std.testing.expectEqual(TP_DEFAULT_CAP, rp.context_cap_tokens);
}

// PlatformKeyMissing is exercised in the fresh-migration integration suite,
// where no other test has seeded a `platform_provider_defaults` row.

// ── resolveActiveProvider — self-managed ────────────────────────────────────────────

test "resolveActiveProvider with self_managed row returns user provider api_key model" {
    setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_TP_SELF_MANAGED);
    defer cleanupTeardown(db_ctx.conn, WS_TP_SELF_MANAGED);

    try seedSelfManagedCredential(db_ctx.conn, ALLOC, WS_TP_SELF_MANAGED, "account-fireworks-self-managed", TP_TEST_PROVIDER, "fw_USER_abc", "accounts/fireworks/models/kimi-k2.6");

    try tenant_provider.upsertSelfManaged(
        ALLOC,
        db_ctx.conn,
        uc1.TENANT_ID,
        "account-fireworks-self-managed",
        "accounts/fireworks/models/kimi-k2.6",
        256_000,
    );

    var rp = try tenant_provider.resolveActiveProvider(ALLOC, db_ctx.conn, uc1.TENANT_ID);
    defer rp.deinit(ALLOC);

    try std.testing.expectEqual(tenant_provider.Mode.self_managed, rp.mode);
    try std.testing.expectEqualStrings(TP_TEST_PROVIDER, rp.provider);
    try std.testing.expectEqualStrings("fw_USER_abc", rp.api_key);
    try std.testing.expectEqualStrings("accounts/fireworks/models/kimi-k2.6", rp.model);
    try std.testing.expectEqual(@as(u32, 256_000), rp.context_cap_tokens);
}

test "resolveActiveProvider reflects an in-place credential update (rotate key, same ref)" {
    // Case 5: tenant_model_selection.secret_ref is a pointer, not a copy — every
    // resolve re-reads the vault, so rotating the key (an upsert on the same
    // name) is picked up by the very next resolve with NO re-selection. The only
    // persistent trace is vault.secrets.updated_at — no audit row is written.
    setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_TP_SELF_MANAGED);
    defer cleanupTeardown(db_ctx.conn, WS_TP_SELF_MANAGED);

    const MODEL_ID = "accounts/fireworks/models/kimi-k2.6";
    try seedSelfManagedCredential(db_ctx.conn, ALLOC, WS_TP_SELF_MANAGED, "rotating-key", TP_TEST_PROVIDER, "fw_OLD_key", MODEL_ID);
    try tenant_provider.upsertSelfManaged(ALLOC, db_ctx.conn, uc1.TENANT_ID, "rotating-key", MODEL_ID, 256_000);

    var rp1 = try tenant_provider.resolveActiveProvider(ALLOC, db_ctx.conn, uc1.TENANT_ID);
    try std.testing.expectEqualStrings("fw_OLD_key", rp1.api_key);
    rp1.deinit(ALLOC);

    // Rotate the key in place — storeVaultJson upserts on (workspace_id, key_name).
    try seedSelfManagedCredential(db_ctx.conn, ALLOC, WS_TP_SELF_MANAGED, "rotating-key", TP_TEST_PROVIDER, "fw_NEW_key", MODEL_ID);

    // No re-activation: the same tenant_model_selection row now resolves the new key.
    var rp2 = try tenant_provider.resolveActiveProvider(ALLOC, db_ctx.conn, uc1.TENANT_ID);
    defer rp2.deinit(ALLOC);
    try std.testing.expectEqualStrings("fw_NEW_key", rp2.api_key);
}
