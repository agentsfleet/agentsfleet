// Database (DB)-backed tenant provider upsert tests.

const std = @import("std");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const tenant_provider = @import("tenant_provider.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");
const fixture = @import("tenant_provider_test.zig");

test "upsertSelfManaged with non-existent credential returns SecretMissing" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(fixture.ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, fixture.WS_TP_UPSERT);
    defer fixture.cleanupTeardown(db_ctx.conn, fixture.WS_TP_UPSERT);

    try std.testing.expectError(
        tenant_provider.ResolveError.SecretMissing,
        tenant_provider.upsertSelfManaged(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID, "does-not-exist", "any-model", 256_000),
    );
}

test "upsertPlatform writes mode=platform with NULL secret_ref" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(fixture.ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, fixture.WS_TP_UPSERT);
    defer fixture.cleanupTeardown(db_ctx.conn, fixture.WS_TP_UPSERT);

    try fixture.seedPlatformLlmKey(db_ctx.conn, fixture.ALLOC, fixture.WS_TP_UPSERT, fixture.TP_TEST_PROVIDER, "fw_PLATFORM_xyz");
    try tenant_provider.upsertPlatform(fixture.ALLOC, db_ctx.conn, uc1.TENANT_ID);

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT mode, provider, model, context_cap_tokens, secret_ref
        \\FROM core.tenant_model_selection WHERE tenant_id = $1::uuid
    , .{uc1.TENANT_ID}));
    defer q.deinit();
    const row = (try q.next()).?;
    try std.testing.expectEqualStrings("platform", try row.get([]const u8, 0));
    try std.testing.expectEqualStrings(fixture.TP_TEST_PROVIDER, try row.get([]const u8, 1));
    try std.testing.expectEqualStrings(fixture.TP_DEFAULT_MODEL, try row.get([]const u8, 2));
    try std.testing.expectEqual(@as(i32, @intCast(fixture.TP_DEFAULT_CAP)), try row.get(i32, 3));
    try std.testing.expectEqual(@as(?[]const u8, null), try row.get(?[]const u8, 4));
}
