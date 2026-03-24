//! Shared JSON value helpers for the executor module.
//!
//! Used by handler.zig, runner.zig, and client.zig to extract typed
//! fields from std.json.Value objects. Consolidated here to avoid
//! duplication across the executor module boundary.

const std = @import("std");

pub fn getStr(obj: std.json.Value, key: []const u8) ?[]const u8 {
    if (obj != .object) return null;
    const val = obj.object.get(key) orelse return null;
    return switch (val) {
        .string => |s| s,
        else => null,
    };
}

pub fn getInt(obj: std.json.Value, key: []const u8) ?i64 {
    if (obj != .object) return null;
    const val = obj.object.get(key) orelse return null;
    return switch (val) {
        .integer => |i| i,
        else => null,
    };
}

pub fn getIntOrZero(obj: std.json.Value, key: []const u8) u64 {
    if (obj != .object) return 0;
    const val = obj.object.get(key) orelse return 0;
    return switch (val) {
        .integer => |i| @intCast(@max(0, i)),
        else => 0,
    };
}

pub fn getFloat(obj: std.json.Value, key: []const u8) ?f64 {
    if (obj != .object) return null;
    const val = obj.object.get(key) orelse return null;
    return switch (val) {
        .float => |f| f,
        .integer => |i| @as(f64, @floatFromInt(i)),
        else => null,
    };
}

pub fn getBool(obj: std.json.Value, key: []const u8) bool {
    if (obj != .object) return false;
    const val = obj.object.get(key) orelse return false;
    return switch (val) {
        .bool => |b| b,
        else => false,
    };
}

pub fn getBoolDefaultTrue(obj: std.json.Value, key: []const u8) bool {
    if (obj != .object) return true;
    const val = obj.object.get(key) orelse return true;
    return switch (val) {
        .bool => |b| b,
        else => true,
    };
}

// ── Tests ────────────────────────────────────────────────────────────────

test "getStr returns null for missing key" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try std.testing.expect(getStr(obj, "missing") == null);
}

test "getStr returns null for non-string value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("key", .{ .integer = 42 });
    try std.testing.expect(getStr(obj, "key") == null);
}

test "getStr returns string value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("key", .{ .string = "value" });
    try std.testing.expectEqualStrings("value", getStr(obj, "key").?);
}

test "getStr returns null for non-object" {
    try std.testing.expect(getStr(.{ .integer = 1 }, "k") == null);
}

test "getIntOrZero returns 0 for missing key" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try std.testing.expectEqual(@as(u64, 0), getIntOrZero(obj, "missing"));
}

test "getIntOrZero returns 0 for negative integer" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("val", .{ .integer = -5 });
    try std.testing.expectEqual(@as(u64, 0), getIntOrZero(obj, "val"));
}

test "getIntOrZero returns value for valid integer" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("count", .{ .integer = 42 });
    try std.testing.expectEqual(@as(u64, 42), getIntOrZero(obj, "count"));
}

test "getFloat returns null for missing key" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try std.testing.expect(getFloat(obj, "nope") == null);
}

test "getFloat returns float for float value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("temp", .{ .float = 0.42 });
    try std.testing.expectEqual(@as(f64, 0.42), getFloat(obj, "temp").?);
}

test "getFloat returns float for integer value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("temp", .{ .integer = 1 });
    try std.testing.expectEqual(@as(f64, 1.0), getFloat(obj, "temp").?);
}

test "getBool returns false for missing key" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try std.testing.expect(!getBool(obj, "missing"));
}

test "getBool returns value for bool field" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("ok", .{ .bool = true });
    try std.testing.expect(getBool(obj, "ok"));
}

test "getBoolDefaultTrue returns true for missing key" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try std.testing.expect(getBoolDefaultTrue(obj, "missing"));
}

test "getBoolDefaultTrue returns explicit false" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("enabled", .{ .bool = false });
    try std.testing.expect(!getBoolDefaultTrue(obj, "enabled"));
}

test "getBoolField returns false for non-bool value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("flag", .{ .string = "true" });
    try std.testing.expect(!getBool(obj, "flag"));
}

test "getStringField returns null for non-object input" {
    try std.testing.expect(getStr(.{ .integer = 42 }, "key") == null);
    try std.testing.expect(getStr(.null, "key") == null);
    try std.testing.expect(getStr(.{ .bool = true }, "key") == null);
}

// ── getInt (nullable i64 variant) ────────────────────────────────────────

test "getInt returns null for missing key" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try std.testing.expect(getInt(obj, "nope") == null);
}

test "getInt returns value for integer" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("n", .{ .integer = -42 });
    try std.testing.expectEqual(@as(i64, -42), getInt(obj, "n").?);
}

test "getInt returns null for non-integer value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("n", .{ .string = "42" });
    try std.testing.expect(getInt(obj, "n") == null);
}

test "getInt returns null for non-object input" {
    try std.testing.expect(getInt(.{ .bool = true }, "k") == null);
    try std.testing.expect(getInt(.null, "k") == null);
}

// ── getIntOrZero invalid types ───────────────────────────────────────────

test "getIntOrZero returns 0 for non-integer value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("n", .{ .string = "not a number" });
    try std.testing.expectEqual(@as(u64, 0), getIntOrZero(obj, "n"));
}

test "getIntOrZero returns 0 for non-object input" {
    try std.testing.expectEqual(@as(u64, 0), getIntOrZero(.null, "k"));
    try std.testing.expectEqual(@as(u64, 0), getIntOrZero(.{ .bool = true }, "k"));
}

// ── getFloat invalid types ───────────────────────────────────────────────

test "getFloat returns null for non-numeric value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("f", .{ .string = "not a float" });
    try std.testing.expect(getFloat(obj, "f") == null);
}

test "getFloat returns null for non-object input" {
    try std.testing.expect(getFloat(.null, "k") == null);
    try std.testing.expect(getFloat(.{ .integer = 1 }, "k") == null);
}

// ── getBool / getBoolDefaultTrue invalid types ───────────────────────────

test "getBool returns false for non-object input" {
    try std.testing.expect(!getBool(.null, "k"));
    try std.testing.expect(!getBool(.{ .integer = 1 }, "k"));
}

test "getBoolDefaultTrue returns true for non-object input" {
    try std.testing.expect(getBoolDefaultTrue(.null, "k"));
    try std.testing.expect(getBoolDefaultTrue(.{ .integer = 1 }, "k"));
}

test "getBool returns false for non-bool string value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("flag", .{ .string = "true" });
    try std.testing.expect(!getBool(obj, "flag"));
}

test "getBoolDefaultTrue returns true for non-bool string value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("flag", .{ .string = "false" });
    // Non-bool values fall through to default=true.
    try std.testing.expect(getBoolDefaultTrue(obj, "flag"));
}

// ── T2: Edge — unicode and empty strings ─────────────────────────────────

test "getStr returns unicode multibyte string" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("name", .{ .string = "日本語テスト 🚀" });
    try std.testing.expectEqualStrings("日本語テスト 🚀", getStr(obj, "name").?);
}

test "getStr returns empty string (distinct from missing)" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("empty", .{ .string = "" });
    const result = getStr(obj, "empty");
    try std.testing.expect(result != null);
    try std.testing.expectEqualStrings("", result.?);
}

// ── T2: Edge — zero and boundary integers ────────────────────────────────

test "getInt returns zero for zero value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("n", .{ .integer = 0 });
    try std.testing.expectEqual(@as(i64, 0), getInt(obj, "n").?);
}

test "getIntOrZero returns 0 for zero value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("n", .{ .integer = 0 });
    try std.testing.expectEqual(@as(u64, 0), getIntOrZero(obj, "n"));
}

test "getInt returns max i64" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("big", .{ .integer = std.math.maxInt(i64) });
    try std.testing.expectEqual(std.math.maxInt(i64), getInt(obj, "big").?);
}

test "getFloat returns null for bool input" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("f", .{ .bool = true });
    try std.testing.expect(getFloat(obj, "f") == null);
}

// ── T11: Performance — repeated calls no leak ────────────────────────────

test "json helpers 100 iterations no leak" {
    const alloc = std.testing.allocator;
    for (0..100) |_| {
        var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
        try obj.object.put("s", .{ .string = "val" });
        try obj.object.put("i", .{ .integer = 42 });
        try obj.object.put("f", .{ .float = 3.14 });
        try obj.object.put("b", .{ .bool = true });

        _ = getStr(obj, "s");
        _ = getInt(obj, "i");
        _ = getIntOrZero(obj, "i");
        _ = getFloat(obj, "f");
        _ = getBool(obj, "b");
        _ = getBoolDefaultTrue(obj, "b");
        _ = getStr(obj, "missing");
        _ = getInt(obj, "missing");

        obj.object.deinit();
    }
}
