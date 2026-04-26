//! Vault structured-credential layer tests.
//!
//! Pure tests (shape gate) run on every `make test`. DB-backed round-trip
//! tests skip with `error.SkipZigTest` when neither TEST_DATABASE_URL nor
//! DATABASE_URL is set, so the unit tier never depends on a live Postgres.

const std = @import("std");
const pg = @import("pg");
const vault = @import("vault.zig");
const crypto_store = @import("../secrets/crypto_store.zig");
const base = @import("../db/test_fixtures.zig");

// ── shape gate (pure) ──────────────────────────────────────────────────────

test "validateObject rejects strings" {
    const v = std.json.Value{ .string = "fly" };
    try std.testing.expectError(vault.Error.NotAnObject, vault.validateObject(v));
}

test "validateObject rejects arrays" {
    var arr = std.json.Array.init(std.testing.allocator);
    defer arr.deinit();
    const v = std.json.Value{ .array = arr };
    try std.testing.expectError(vault.Error.NotAnObject, vault.validateObject(v));
}

test "validateObject rejects integers and bools and nulls" {
    try std.testing.expectError(vault.Error.NotAnObject, vault.validateObject(.{ .integer = 7 }));
    try std.testing.expectError(vault.Error.NotAnObject, vault.validateObject(.{ .bool = true }));
    try std.testing.expectError(vault.Error.NotAnObject, vault.validateObject(.null));
}

test "validateObject rejects empty object" {
    var obj = std.json.ObjectMap.init(std.testing.allocator);
    defer obj.deinit();
    const v = std.json.Value{ .object = obj };
    try std.testing.expectError(vault.Error.EmptyObject, vault.validateObject(v));
}

test "validateObject accepts non-empty object" {
    var obj = std.json.ObjectMap.init(std.testing.allocator);
    defer obj.deinit();
    try obj.put("host", .{ .string = "api.machines.dev" });
    const v = std.json.Value{ .object = obj };
    try vault.validateObject(v);
}

// ── DB-backed round-trip ──────────────────────────────────────────────────
//
// A single unique workspace_id keeps these tests self-isolating; we clean
// up the `vault.secrets` rows we wrote at the end of each test rather
// than via `defer`, because deferred cleanup at the end of an integration
// test can race with pool teardown (see signup_bootstrap_test.zig comments).

const TEST_WS_ID = "0195b4ba-8d3a-7f13-8abc-cd0000000001";
const ENCRYPTION_KEY_HEX = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn setEncryptionKey() void {
    const c = @cImport(@cInclude("stdlib.h"));
    var z: [65]u8 = undefined;
    @memcpy(z[0..64], ENCRYPTION_KEY_HEX);
    z[64] = 0;
    _ = c.setenv("ENCRYPTION_MASTER_KEY", &z, 1);
}

fn cleanupTestRows(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1", .{TEST_WS_ID}) catch {};
    base.teardownWorkspace(conn, TEST_WS_ID);
    base.teardownTenant(conn);
}

fn seedWorkspaceForVault(conn: *pg.Conn) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, TEST_WS_ID);
}

fn buildFlyCredential(alloc: std.mem.Allocator) !std.json.Value {
    var obj = std.json.ObjectMap.init(alloc);
    try obj.put("host", .{ .string = "api.machines.dev" });
    try obj.put("api_token", .{ .string = "FLY_API_TOKEN_xyz" });
    return .{ .object = obj };
}

test "storeJson + loadJson round-trip preserves nested object" {
    setEncryptionKey();
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspaceForVault(handle.conn);
    defer cleanupTestRows(handle.conn);

    var stored = try buildFlyCredential(alloc);
    defer stored.object.deinit();

    try vault.storeJson(alloc, handle.conn, TEST_WS_ID, "zombie:fly", stored, 1);

    var loaded = try vault.loadJson(alloc, handle.conn, TEST_WS_ID, "zombie:fly");
    defer loaded.deinit();

    try std.testing.expect(loaded.value == .object);
    try std.testing.expectEqualStrings(
        "api.machines.dev",
        loaded.value.object.get("host").?.string,
    );
    try std.testing.expectEqualStrings(
        "FLY_API_TOKEN_xyz",
        loaded.value.object.get("api_token").?.string,
    );
}

test "storeJson + loadJson round-trip preserves nested + arrays + numbers + bools + nulls" {
    setEncryptionKey();
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspaceForVault(handle.conn);
    defer cleanupTestRows(handle.conn);

    // Nested object with every JSON shape the parser supports — guards against
    // a future "stringify only handles strings" regression in storeJson and
    // proves loadJson reconstructs the same shape after KMS roundtrip.
    var nested = std.json.ObjectMap.init(alloc);
    defer nested.deinit();
    try nested.put("inner_str", .{ .string = "deep" });
    try nested.put("inner_num", .{ .integer = 42 });

    var arr = std.json.Array.init(alloc);
    defer arr.deinit();
    try arr.append(.{ .integer = 1 });
    try arr.append(.{ .string = "two" });
    try arr.append(.{ .bool = true });

    var top = std.json.ObjectMap.init(alloc);
    defer top.deinit();
    try top.put("str", .{ .string = "FLY_TOKEN_xyz" });
    try top.put("num_int", .{ .integer = 31415 });
    try top.put("num_neg", .{ .integer = -7 });
    try top.put("bool_t", .{ .bool = true });
    try top.put("bool_f", .{ .bool = false });
    try top.put("null_field", .null);
    try top.put("nested", .{ .object = nested });
    try top.put("arr", .{ .array = arr });

    try vault.storeJson(alloc, handle.conn, TEST_WS_ID, "zombie:mixed", .{ .object = top }, 1);

    var loaded = try vault.loadJson(alloc, handle.conn, TEST_WS_ID, "zombie:mixed");
    defer loaded.deinit();

    try std.testing.expect(loaded.value == .object);
    const root = loaded.value.object;
    try std.testing.expectEqualStrings("FLY_TOKEN_xyz", root.get("str").?.string);
    try std.testing.expectEqual(@as(i64, 31415), root.get("num_int").?.integer);
    try std.testing.expectEqual(@as(i64, -7), root.get("num_neg").?.integer);
    try std.testing.expect(root.get("bool_t").?.bool == true);
    try std.testing.expect(root.get("bool_f").?.bool == false);
    try std.testing.expect(root.get("null_field").? == .null);

    const nested_loaded = root.get("nested").?.object;
    try std.testing.expectEqualStrings("deep", nested_loaded.get("inner_str").?.string);
    try std.testing.expectEqual(@as(i64, 42), nested_loaded.get("inner_num").?.integer);

    const arr_loaded = root.get("arr").?.array;
    try std.testing.expectEqual(@as(usize, 3), arr_loaded.items.len);
    try std.testing.expectEqual(@as(i64, 1), arr_loaded.items[0].integer);
    try std.testing.expectEqualStrings("two", arr_loaded.items[1].string);
    try std.testing.expect(arr_loaded.items[2].bool == true);
}

test "loadJson surfaces MalformedPlaintext when row was written as bare string" {
    setEncryptionKey();
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspaceForVault(handle.conn);
    defer cleanupTestRows(handle.conn);

    // Simulate a row written by the legacy `--value <string>` path: the
    // plaintext is a bare string, not JSON. loadJson must fail loud rather
    // than silently wrap.
    try crypto_store.store(alloc, handle.conn, TEST_WS_ID, "zombie:legacy", "raw-token", 1);
    try std.testing.expectError(
        vault.Error.MalformedPlaintext,
        vault.loadJson(alloc, handle.conn, TEST_WS_ID, "zombie:legacy"),
    );
}

test "deleteCredential reports true on existing row, false on missing" {
    setEncryptionKey();
    const alloc = std.testing.allocator;
    const handle = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedWorkspaceForVault(handle.conn);
    defer cleanupTestRows(handle.conn);

    var v = try buildFlyCredential(alloc);
    defer v.object.deinit();
    try vault.storeJson(alloc, handle.conn, TEST_WS_ID, "zombie:fly", v, 1);

    try std.testing.expect(try vault.deleteCredential(handle.conn, TEST_WS_ID, "zombie:fly"));
    try std.testing.expect(!(try vault.deleteCredential(handle.conn, TEST_WS_ID, "zombie:fly")));
}
