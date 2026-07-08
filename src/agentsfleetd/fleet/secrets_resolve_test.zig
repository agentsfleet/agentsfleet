//! Credential resolver tests — DB-backed, skip when TEST_DATABASE_URL absent.

const std = @import("std");
const pg = @import("pg");
const secrets = @import("secrets_resolve.zig");
const base = @import("../db/test_fixtures.zig");
const crypto_primitives = @import("../secrets/crypto_primitives.zig");

const TEST_WS_ID = "0195b4ba-8d3a-7f13-8abc-cd0000000010";

fn cleanupTestRows(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1", .{TEST_WS_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    base.teardownWorkspace(conn, TEST_WS_ID);
    base.teardownTenant(conn);
}

fn putCredential(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    name: []const u8,
    fields: []const struct { k: []const u8, v: []const u8 },
) !void {
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    for (fields) |f| try obj.put(alloc, f.k, .{ .string = f.v });
    try base.storeVaultJson(alloc, conn, TEST_WS_ID, name, .{ .object = obj });
}

// resolveSecretsMap acquires its own connection from the pool. Tests must
// release the seed connection before calling it; the test pool is sized 1
// in dev (`base.openTestConn` defaults), so holding two checkouts at once
// deadlocks on `pool.acquire`.

test "resolveSecretsMap returns parsed objects in order" {
    crypto_primitives.setTestKek();
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer handle.pool.deinit();

    try base.seedTenant(handle.conn);
    try base.seedWorkspace(handle.conn, TEST_WS_ID);
    try putCredential(alloc, handle.conn, "fly", &.{
        .{ .k = "host", .v = "api.machines.dev" },
        .{ .k = "api_token", .v = "FLY_TOKEN" },
    });
    try putCredential(alloc, handle.conn, "slack", &.{
        .{ .k = "host", .v = "slack.com" },
        .{ .k = "bot_token", .v = "xoxb-test" },
    });
    handle.pool.release(handle.conn);

    const names = &[_][]const u8{ "fly", "slack" };
    const resolved = try secrets.resolveSecretsMap(alloc, handle.pool, TEST_WS_ID, names);
    defer secrets.freeResolved(alloc, resolved);

    try std.testing.expectEqual(@as(usize, 2), resolved.len);
    try std.testing.expectEqualStrings("fly", resolved[0].name);
    try std.testing.expectEqualStrings(
        "api.machines.dev",
        resolved[0].parsed.value.object.get("host").?.string,
    );
    try std.testing.expectEqualStrings("slack", resolved[1].name);
    try std.testing.expectEqualStrings(
        "xoxb-test",
        resolved[1].parsed.value.object.get("bot_token").?.string,
    );

    const cleanup_conn = try handle.pool.acquire();
    defer handle.pool.release(cleanup_conn);
    cleanupTestRows(cleanup_conn);
}

test "resolveSecretsMap surfaces CredentialNotFound on missing name" {
    crypto_primitives.setTestKek();
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer handle.pool.deinit();

    try base.seedTenant(handle.conn);
    try base.seedWorkspace(handle.conn, TEST_WS_ID);
    try putCredential(alloc, handle.conn, "fly", &.{
        .{ .k = "host", .v = "api.machines.dev" },
        .{ .k = "api_token", .v = "FLY_TOKEN" },
    });
    handle.pool.release(handle.conn);

    const names = &[_][]const u8{ "fly", "missing-credential" };
    try std.testing.expectError(
        error.CredentialNotFound,
        secrets.resolveSecretsMap(alloc, handle.pool, TEST_WS_ID, names),
    );

    const cleanup_conn = try handle.pool.acquire();
    defer handle.pool.release(cleanup_conn);
    cleanupTestRows(cleanup_conn);
}
