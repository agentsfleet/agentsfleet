// Integration tests for tenant_provider.zig.
//
// Cover: Mode + ResolvedProvider invariants (no DB), and the resolver +
// upsert + delete entry points (real DB + vault). Skips when no DB.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const tenant_provider = @import("tenant_provider.zig");
const vault = @import("vault.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");

const ALLOC = std.testing.allocator;

const WS_TP_RESOLVE = "0195b4ba-8d3a-7f13-8abc-aa2000000001";
const WS_TP_UPSERT = "0195b4ba-8d3a-7f13-8abc-aa2000000002";
const WS_TP_BYOK = "0195b4ba-8d3a-7f13-8abc-aa2000000003";
const WS_TP_DELETE = "0195b4ba-8d3a-7f13-8abc-aa2000000004";

// Provider name scoped to this test file. The platform_llm_keys table has
// UNIQUE on provider, so tests that share a provider name fight over the
// same row via ON CONFLICT DO UPDATE. Using a test-scoped name keeps our
// rows isolated from other integration tests.
const TP_TEST_PROVIDER = "tenant_provider_test_fireworks";

const ENCRYPTION_KEY_HEX = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn setEncryptionKey() void {
    const c = @cImport(@cInclude("stdlib.h"));
    var z: [65]u8 = undefined;
    @memcpy(z[0..64], ENCRYPTION_KEY_HEX);
    z[64] = 0;
    _ = c.setenv("ENCRYPTION_MASTER_KEY", &z, 1);
}

fn cleanupTeardown(conn: *pg.Conn, ws_id: []const u8) void {
    _ = conn.exec("DELETE FROM core.tenant_providers WHERE tenant_id = $1::uuid", .{uc1.TENANT_ID}) catch {};
    _ = conn.exec("DELETE FROM core.platform_llm_keys WHERE source_workspace_id = $1::uuid", .{ws_id}) catch {};
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1", .{ws_id}) catch {};
    uc1.teardown(conn, ws_id);
}

fn seedPlatformLlmKey(conn: *pg.Conn, alloc: std.mem.Allocator, ws_id: []const u8, provider: []const u8, api_key: []const u8) !void {
    // Vault row at (ws_id, provider) — same M45 storage path BYOK uses.
    var obj = std.json.ObjectMap.init(alloc);
    defer obj.deinit();
    try obj.put("provider", .{ .string = provider });
    try obj.put("api_key", .{ .string = api_key });
    const value = std.json.Value{ .object = obj };
    try vault.storeJson(alloc, conn, ws_id, provider, value, 1);

    // Generate a UUIDv7 (required by ck_platform_llm_keys_id_uuidv7).
    const id_format = @import("../types/id_format.zig");
    const key_id = try id_format.generateZombieId(alloc);
    defer alloc.free(key_id);
    const now_ms: i64 = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO core.platform_llm_keys (id, provider, source_workspace_id, active, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, true, $4, $4)
        \\ON CONFLICT (provider) DO UPDATE
        \\SET source_workspace_id = EXCLUDED.source_workspace_id, active = true, updated_at = EXCLUDED.updated_at
    , .{ key_id, provider, ws_id, now_ms });
}

fn seedByokCredential(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    ws_id: []const u8,
    name: []const u8,
    provider: []const u8,
    api_key: []const u8,
    model: []const u8,
) !void {
    var obj = std.json.ObjectMap.init(alloc);
    defer obj.deinit();
    try obj.put("provider", .{ .string = provider });
    try obj.put("api_key", .{ .string = api_key });
    try obj.put("model", .{ .string = model });
    const value = std.json.Value{ .object = obj };
    try vault.storeJson(alloc, conn, ws_id, name, value, 1);
}

// ── Mode enum + ResolvedProvider invariants ────────────────────────────────

test "Mode label round-trips for both variants" {
    try std.testing.expectEqualStrings("platform", tenant_provider.Mode.platform.label());
    try std.testing.expectEqualStrings("byok", tenant_provider.Mode.byok.label());
}

test "ResolvedProvider.deinit completes without leaking" {
    const alloc = std.testing.allocator;
    var rp = tenant_provider.ResolvedProvider{
        .mode = .byok,
        .provider = try alloc.dupe(u8, TP_TEST_PROVIDER),
        .api_key = try alloc.dupe(u8, "fw_LIVE_secret_xyz"),
        .model = try alloc.dupe(u8, "accounts/fireworks/models/kimi-k2.6"),
        .context_cap_tokens = 256_000,
    };
    rp.deinit(alloc);
    // testing.allocator detects any un-freed bytes. The api_key zero-on-free
    // is enforced by std.crypto.secureZero at the call site in deinit; reading
    // the freed slice would be UAF, so the secureZero contract is verified by
    // code review rather than a test that inspects post-free memory.
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
    try std.testing.expectEqualStrings(tenant_provider.PLATFORM_DEFAULT_MODEL, rp.model);
    try std.testing.expectEqual(tenant_provider.PLATFORM_DEFAULT_CAP_TOKENS, rp.context_cap_tokens);
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
    try std.testing.expectEqualStrings(tenant_provider.PLATFORM_DEFAULT_MODEL, rp.model);
    try std.testing.expectEqual(tenant_provider.PLATFORM_DEFAULT_CAP_TOKENS, rp.context_cap_tokens);
}

// PlatformKeyMissing path is exercised in §13's integration suite where the
// schema is fresh-migrated and no other test has seeded a `platform_llm_keys`
// row. We skip the test here because the integration test pool is shared and
// a global `DELETE FROM core.platform_llm_keys` from this test would race
// with other tests' seedings.

// ── resolveActiveProvider — BYOK ────────────────────────────────────────────

test "resolveActiveProvider with byok row returns user provider api_key model" {
    setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_TP_BYOK);
    defer cleanupTeardown(db_ctx.conn, WS_TP_BYOK);

    try seedByokCredential(db_ctx.conn, ALLOC, WS_TP_BYOK, "account-fireworks-byok", TP_TEST_PROVIDER, "fw_USER_abc", "accounts/fireworks/models/kimi-k2.6");

    try tenant_provider.upsertByok(
        ALLOC,
        db_ctx.conn,
        uc1.TENANT_ID,
        "account-fireworks-byok",
        "accounts/fireworks/models/kimi-k2.6",
        256_000,
    );

    var rp = try tenant_provider.resolveActiveProvider(ALLOC, db_ctx.conn, uc1.TENANT_ID);
    defer rp.deinit(ALLOC);

    try std.testing.expectEqual(tenant_provider.Mode.byok, rp.mode);
    try std.testing.expectEqualStrings(TP_TEST_PROVIDER, rp.provider);
    try std.testing.expectEqualStrings("fw_USER_abc", rp.api_key);
    try std.testing.expectEqualStrings("accounts/fireworks/models/kimi-k2.6", rp.model);
    try std.testing.expectEqual(@as(u32, 256_000), rp.context_cap_tokens);
}

test "resolveActiveProvider returns CredentialMissing when byok credential row absent" {
    setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_TP_BYOK);
    defer cleanupTeardown(db_ctx.conn, WS_TP_BYOK);

    try seedByokCredential(db_ctx.conn, ALLOC, WS_TP_BYOK, "account-fireworks-byok", TP_TEST_PROVIDER, "fw_USER_abc", "any-model");
    try tenant_provider.upsertByok(ALLOC, db_ctx.conn, uc1.TENANT_ID, "account-fireworks-byok", "any-model", 256_000);

    // User deletes the credential while still in mode=byok.
    _ = try db_ctx.conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2", .{ WS_TP_BYOK, "account-fireworks-byok" });

    try std.testing.expectError(
        tenant_provider.ResolveError.CredentialMissing,
        tenant_provider.resolveActiveProvider(ALLOC, db_ctx.conn, uc1.TENANT_ID),
    );
}

test "resolveActiveProvider returns CredentialDataMalformed when JSON lacks api_key" {
    setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_TP_BYOK);
    defer cleanupTeardown(db_ctx.conn, WS_TP_BYOK);

    // Seed a malformed credential first (missing api_key); upsertByok must reject it.
    var obj = std.json.ObjectMap.init(ALLOC);
    defer obj.deinit();
    try obj.put("provider", .{ .string = TP_TEST_PROVIDER });
    try obj.put("model", .{ .string = "any-model" });
    try vault.storeJson(ALLOC, db_ctx.conn, WS_TP_BYOK, "bad-cred", .{ .object = obj }, 1);

    try std.testing.expectError(
        tenant_provider.ResolveError.CredentialDataMalformed,
        tenant_provider.upsertByok(ALLOC, db_ctx.conn, uc1.TENANT_ID, "bad-cred", "any-model", 256_000),
    );
}

// ── upsertByok / upsertPlatform / deleteRow ────────────────────────────────

test "upsertByok with non-existent credential returns CredentialMissing" {
    setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_TP_UPSERT);
    defer cleanupTeardown(db_ctx.conn, WS_TP_UPSERT);

    try std.testing.expectError(
        tenant_provider.ResolveError.CredentialMissing,
        tenant_provider.upsertByok(ALLOC, db_ctx.conn, uc1.TENANT_ID, "does-not-exist", "any-model", 256_000),
    );
}

test "upsertPlatform writes mode=platform with NULL credential_ref" {
    setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_TP_UPSERT);
    defer cleanupTeardown(db_ctx.conn, WS_TP_UPSERT);

    try seedPlatformLlmKey(db_ctx.conn, ALLOC, WS_TP_UPSERT, TP_TEST_PROVIDER, "fw_PLATFORM_xyz");
    try tenant_provider.upsertPlatform(ALLOC, db_ctx.conn, uc1.TENANT_ID);

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT mode, provider, model, context_cap_tokens, credential_ref
        \\FROM core.tenant_providers WHERE tenant_id = $1::uuid
    , .{uc1.TENANT_ID}));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqualStrings("platform", try row.get([]const u8, 0));
    try std.testing.expectEqualStrings(TP_TEST_PROVIDER, try row.get([]const u8, 1));
    try std.testing.expectEqualStrings(tenant_provider.PLATFORM_DEFAULT_MODEL, try row.get([]const u8, 2));
    try std.testing.expectEqual(@as(i32, @intCast(tenant_provider.PLATFORM_DEFAULT_CAP_TOKENS)), try row.get(i32, 3));
    try std.testing.expectEqual(@as(?[]const u8, null), try row.get(?[]const u8, 4));
}

test "deleteRow removes the tenant_providers row" {
    setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_TP_DELETE);
    defer cleanupTeardown(db_ctx.conn, WS_TP_DELETE);

    try seedPlatformLlmKey(db_ctx.conn, ALLOC, WS_TP_DELETE, TP_TEST_PROVIDER, "fw_PLATFORM_xyz");
    try tenant_provider.upsertPlatform(ALLOC, db_ctx.conn, uc1.TENANT_ID);

    try tenant_provider.deleteRow(db_ctx.conn, uc1.TENANT_ID);

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT COUNT(*)::BIGINT FROM core.tenant_providers WHERE tenant_id = $1::uuid
    , .{uc1.TENANT_ID}));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
}
