//! Vault structured-credential layer tests.
//!
//! Pure tests (shape gate) run on every `make test`. DB-backed round-trip
//! tests skip with `error.SkipZigTest` when neither TEST_DATABASE_URL nor
//! DATABASE_URL is set, so the unit tier never depends on a live Postgres.

const std = @import("std");
const pg = @import("pg");
const vault = @import("vault.zig");
const base = @import("../db/test_fixtures.zig");
const crypto_primitives = @import("../secrets/crypto_primitives.zig");

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
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(std.testing.allocator);
    const v = std.json.Value{ .object = obj };
    try std.testing.expectError(vault.Error.EmptyObject, vault.validateObject(v));
}

test "validateObject accepts non-empty object" {
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(std.testing.allocator);
    try obj.put(std.testing.allocator, "host", .{ .string = "api.machines.dev" });
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

fn setEncryptionKey() void {
    crypto_primitives.setTestKek();
}

fn cleanupTestRows(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1", .{TEST_WS_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    base.teardownWorkspace(conn, TEST_WS_ID);
    base.teardownTenant(conn);
}

fn seedWorkspaceForVault(conn: *pg.Conn) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, TEST_WS_ID);
}

fn buildFlyCredential(alloc: std.mem.Allocator) !std.json.Value {
    var obj: std.json.ObjectMap = .empty;
    try obj.put(alloc, "host", .{ .string = "api.machines.dev" });
    try obj.put(alloc, "api_token", .{ .string = "FLY_API_TOKEN_xyz" });
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
    defer stored.object.deinit(alloc);

    try base.storeVaultJson(alloc, handle.conn, TEST_WS_ID, "agent:fly", stored);

    var loaded = try vault.loadJson(alloc, handle.conn, TEST_WS_ID, "agent:fly");
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
    var nested: std.json.ObjectMap = .empty;
    defer nested.deinit(alloc);
    try nested.put(alloc, "inner_str", .{ .string = "deep" });
    try nested.put(alloc, "inner_num", .{ .integer = 42 });

    var arr = std.json.Array.init(alloc);
    defer arr.deinit();
    try arr.append(.{ .integer = 1 });
    try arr.append(.{ .string = "two" });
    try arr.append(.{ .bool = true });

    var top: std.json.ObjectMap = .empty;
    defer top.deinit(alloc);
    try top.put(alloc, "str", .{ .string = "FLY_TOKEN_xyz" });
    try top.put(alloc, "num_int", .{ .integer = 31415 });
    try top.put(alloc, "num_neg", .{ .integer = -7 });
    try top.put(alloc, "bool_t", .{ .bool = true });
    try top.put(alloc, "bool_f", .{ .bool = false });
    try top.put(alloc, "null_field", .null);
    try top.put(alloc, "nested", .{ .object = nested });
    try top.put(alloc, "arr", .{ .array = arr });

    try base.storeVaultJson(alloc, handle.conn, TEST_WS_ID, "agent:mixed", .{ .object = top });

    var loaded = try vault.loadJson(alloc, handle.conn, TEST_WS_ID, "agent:mixed");
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
    defer v.object.deinit(alloc);
    try base.storeVaultJson(alloc, handle.conn, TEST_WS_ID, "agent:fly", v);

    try std.testing.expect(try vault.deleteCredential(handle.conn, TEST_WS_ID, "agent:fly"));
    try std.testing.expect(!(try vault.deleteCredential(handle.conn, TEST_WS_ID, "agent:fly")));
}
