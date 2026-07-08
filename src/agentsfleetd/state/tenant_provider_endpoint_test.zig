// Database (DB)-backed tenant provider endpoint and malformed-secret tests.

const std = @import("std");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const tenant_provider = @import("tenant_provider.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");
const fixture = @import("tenant_provider_test.zig");

test "resolveActiveProvider carries a validated base_url for openai-compatible (end-to-end)" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(fixture.ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);
    defer fixture.cleanupTeardown(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);

    const custom_url = "https://api.openrouter.ai/v1";
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(fixture.ALLOC);
    try obj.put(fixture.ALLOC, "provider", .{ .string = fixture.COMPAT });
    try obj.put(fixture.ALLOC, "api_key", .{ .string = "sk_user_compat_xyz" });
    try obj.put(fixture.ALLOC, "model", .{ .string = "gpt-4o-mini" });
    try obj.put(fixture.ALLOC, "base_url", .{ .string = custom_url });
    try base.storeVaultJson(fixture.ALLOC, db_ctx.conn, fixture.WS_TP_SELF_MANAGED, "compat-endpoint", .{ .object = obj });

    try tenant_provider.upsertSelfManaged(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID, "compat-endpoint", "gpt-4o-mini", 128_000);

    var rp = try tenant_provider.resolveActiveProvider(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID);
    defer rp.deinit(fixture.ALLOC);

    try std.testing.expectEqual(tenant_provider.Mode.self_managed, rp.mode);
    try std.testing.expectEqualStrings(fixture.COMPAT, rp.provider);
    try std.testing.expectEqualStrings("sk_user_compat_xyz", rp.api_key);
    try std.testing.expectEqualStrings(custom_url, rp.base_url.?);
}

test "resolveActiveProvider resolves an openai-compatible credential with NO api_key (keyless endpoint)" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(fixture.ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);
    defer fixture.cleanupTeardown(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);

    const keyless_url = "https://vllm.public.example/v1";
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(fixture.ALLOC);
    try obj.put(fixture.ALLOC, "provider", .{ .string = fixture.COMPAT });
    try obj.put(fixture.ALLOC, "model", .{ .string = "gpt-4o-mini" });
    try obj.put(fixture.ALLOC, "base_url", .{ .string = keyless_url });
    try base.storeVaultJson(fixture.ALLOC, db_ctx.conn, fixture.WS_TP_SELF_MANAGED, "keyless-compat", .{ .object = obj });

    try tenant_provider.upsertSelfManaged(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID, "keyless-compat", "gpt-4o-mini", 128_000);

    var rp = try tenant_provider.resolveActiveProvider(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID);
    defer rp.deinit(fixture.ALLOC);

    try std.testing.expectEqual(tenant_provider.Mode.self_managed, rp.mode);
    try std.testing.expectEqualStrings(fixture.COMPAT, rp.provider);
    try std.testing.expectEqualStrings("", rp.api_key);
    try std.testing.expectEqualStrings(keyless_url, rp.base_url.?);
}

test "resolveActiveProvider rejects an openai-compatible credential with a Server-Side Request Forgery base_url" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(fixture.ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);
    defer fixture.cleanupTeardown(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);

    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(fixture.ALLOC);
    try obj.put(fixture.ALLOC, "provider", .{ .string = fixture.COMPAT });
    try obj.put(fixture.ALLOC, "api_key", .{ .string = "sk_user_compat_xyz" });
    try obj.put(fixture.ALLOC, "model", .{ .string = "gpt-4o-mini" });
    try obj.put(fixture.ALLOC, "base_url", .{ .string = "https://169.254.169.254/v1" });
    try base.storeVaultJson(fixture.ALLOC, db_ctx.conn, fixture.WS_TP_SELF_MANAGED, "ssrf-endpoint", .{ .object = obj });

    try std.testing.expectError(
        tenant_provider.ResolveError.SecretEndpointInvalid,
        tenant_provider.upsertSelfManaged(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID, "ssrf-endpoint", "gpt-4o-mini", 128_000),
    );
}

test "resolveActiveProvider accepts dashboard fleet-prefixed credential rows" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(fixture.ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);
    defer fixture.cleanupTeardown(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);

    const secret_ref = "dashboard-provider-key";
    try fixture.seedFleetCredential(db_ctx.conn, fixture.ALLOC, fixture.WS_TP_SELF_MANAGED, secret_ref, fixture.TP_TEST_PROVIDER, "fw_DASHBOARD_abc", "accounts/fireworks/models/kimi-k2.6");

    try tenant_provider.upsertSelfManaged(
        fixture.ALLOC,
        db_ctx.conn,
        uc1.TENANT_ID,
        secret_ref,
        "accounts/fireworks/models/kimi-k2.6",
        256_000,
    );

    var rp = try tenant_provider.resolveActiveProvider(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID);
    defer rp.deinit(fixture.ALLOC);

    try std.testing.expectEqual(tenant_provider.Mode.self_managed, rp.mode);
    try std.testing.expectEqualStrings(fixture.TP_TEST_PROVIDER, rp.provider);
    try std.testing.expectEqualStrings("fw_DASHBOARD_abc", rp.api_key);
    try std.testing.expectEqualStrings("accounts/fireworks/models/kimi-k2.6", rp.model);

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT secret_ref FROM core.tenant_model_selection WHERE tenant_id = $1::uuid
    , .{uc1.TENANT_ID}));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqualStrings(secret_ref, try row.get([]const u8, 0));
}

test "resolveActiveProvider returns SecretMissing when self_managed credential row absent" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(fixture.ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);
    defer fixture.cleanupTeardown(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);

    try fixture.seedSelfManagedCredential(db_ctx.conn, fixture.ALLOC, fixture.WS_TP_SELF_MANAGED, "account-fireworks-self-managed", fixture.TP_TEST_PROVIDER, "fw_USER_abc", "any-model");
    try tenant_provider.upsertSelfManaged(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID, "account-fireworks-self-managed", "any-model", 256_000);

    _ = try db_ctx.conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2", .{ fixture.WS_TP_SELF_MANAGED, "account-fireworks-self-managed" });

    try std.testing.expectError(
        tenant_provider.ResolveError.SecretMissing,
        tenant_provider.resolveActiveProvider(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID),
    );
}

test "resolveActiveProvider returns SecretDataMalformed when JSON lacks api_key" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(fixture.ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);
    defer fixture.cleanupTeardown(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);

    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(fixture.ALLOC);
    try obj.put(fixture.ALLOC, "provider", .{ .string = fixture.TP_TEST_PROVIDER });
    try obj.put(fixture.ALLOC, "model", .{ .string = "any-model" });
    try base.storeVaultJson(fixture.ALLOC, db_ctx.conn, fixture.WS_TP_SELF_MANAGED, "bad-cred", .{ .object = obj });

    try std.testing.expectError(
        tenant_provider.ResolveError.SecretDataMalformed,
        tenant_provider.upsertSelfManaged(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID, "bad-cred", "any-model", 256_000),
    );
}

test "resolveActiveProvider returns SecretDataMalformed when JSON lacks model" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(fixture.ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);
    defer fixture.cleanupTeardown(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);

    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(fixture.ALLOC);
    try obj.put(fixture.ALLOC, "provider", .{ .string = fixture.TP_TEST_PROVIDER });
    try obj.put(fixture.ALLOC, "api_key", .{ .string = "fw_USER_abc" });
    try base.storeVaultJson(fixture.ALLOC, db_ctx.conn, fixture.WS_TP_SELF_MANAGED, "no-model-cred", .{ .object = obj });

    try std.testing.expectError(
        tenant_provider.ResolveError.SecretDataMalformed,
        tenant_provider.upsertSelfManaged(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID, "no-model-cred", "override-model", 256_000),
    );
}

test "resolveActiveProvider returns SecretDataMalformed when model is an empty string" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(fixture.ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);
    defer fixture.cleanupTeardown(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);

    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(fixture.ALLOC);
    try obj.put(fixture.ALLOC, "provider", .{ .string = fixture.TP_TEST_PROVIDER });
    try obj.put(fixture.ALLOC, "api_key", .{ .string = "fw_USER_abc" });
    try obj.put(fixture.ALLOC, "model", .{ .string = "" });
    try base.storeVaultJson(fixture.ALLOC, db_ctx.conn, fixture.WS_TP_SELF_MANAGED, "empty-model-cred", .{ .object = obj });

    try std.testing.expectError(
        tenant_provider.ResolveError.SecretDataMalformed,
        tenant_provider.upsertSelfManaged(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID, "empty-model-cred", "override-model", 256_000),
    );
}

test "resolveActiveProvider returns SecretDataMalformed when api_key is an empty string" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(fixture.ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);
    defer fixture.cleanupTeardown(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);

    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(fixture.ALLOC);
    try obj.put(fixture.ALLOC, "provider", .{ .string = fixture.TP_TEST_PROVIDER });
    try obj.put(fixture.ALLOC, "api_key", .{ .string = "" });
    try obj.put(fixture.ALLOC, "model", .{ .string = "any-model" });
    try base.storeVaultJson(fixture.ALLOC, db_ctx.conn, fixture.WS_TP_SELF_MANAGED, "empty-key-cred", .{ .object = obj });

    try std.testing.expectError(
        tenant_provider.ResolveError.SecretDataMalformed,
        tenant_provider.upsertSelfManaged(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID, "empty-key-cred", "any-model", 256_000),
    );
}

test "resolveActiveProvider rejects an openai-compatible credential that lacks a base_url" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(fixture.ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);
    defer fixture.cleanupTeardown(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);

    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(fixture.ALLOC);
    try obj.put(fixture.ALLOC, "provider", .{ .string = fixture.COMPAT });
    try obj.put(fixture.ALLOC, "api_key", .{ .string = "sk_user_compat_xyz" });
    try obj.put(fixture.ALLOC, "model", .{ .string = "gpt-4o-mini" });
    try base.storeVaultJson(fixture.ALLOC, db_ctx.conn, fixture.WS_TP_SELF_MANAGED, "compat-no-baseurl-cred", .{ .object = obj });

    try std.testing.expectError(
        tenant_provider.ResolveError.SecretEndpointInvalid,
        tenant_provider.upsertSelfManaged(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID, "compat-no-baseurl-cred", "gpt-4o-mini", 128_000),
    );
}

test "resolveActiveProvider rejects a named provider that smuggles a base_url" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(fixture.ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);
    defer fixture.cleanupTeardown(db_ctx.conn, fixture.WS_TP_SELF_MANAGED);

    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(fixture.ALLOC);
    try obj.put(fixture.ALLOC, "provider", .{ .string = fixture.TP_TEST_PROVIDER });
    try obj.put(fixture.ALLOC, "api_key", .{ .string = "fw_USER_abc" });
    try obj.put(fixture.ALLOC, "model", .{ .string = "any-model" });
    try obj.put(fixture.ALLOC, "base_url", .{ .string = "https://evil.example.com/v1" });
    try base.storeVaultJson(fixture.ALLOC, db_ctx.conn, fixture.WS_TP_SELF_MANAGED, "named-smuggle-cred", .{ .object = obj });

    try std.testing.expectError(
        tenant_provider.ResolveError.SecretEndpointInvalid,
        tenant_provider.upsertSelfManaged(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID, "named-smuggle-cred", "any-model", 256_000),
    );
}
