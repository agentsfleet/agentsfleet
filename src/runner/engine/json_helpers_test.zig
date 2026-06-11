//! Tests for json_helpers.zig — split per RULE FLL.

const std = @import("std");
const json = @import("json_helpers.zig");

const getStr = json.getStr;
const getInt = json.getInt;
const getFloat = json.getFloat;

test "getStr returns null for missing key" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try std.testing.expect(getStr(obj, "missing") == null);
}

test "getStr returns null for non-string value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try obj.object.put(alloc, "key", .{ .integer = 42 });
    try std.testing.expect(getStr(obj, "key") == null);
}

test "getStr returns string value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try obj.object.put(alloc, "key", .{ .string = "value" });
    try std.testing.expectEqualStrings("value", getStr(obj, "key").?);
}

test "getStr returns null for non-object" {
    try std.testing.expect(getStr(.{ .integer = 1 }, "k") == null);
}

test "getFloat returns null for missing key" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try std.testing.expect(getFloat(obj, "nope") == null);
}

test "getFloat returns float for float value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try obj.object.put(alloc, "temp", .{ .float = 0.42 });
    try std.testing.expectEqual(@as(f64, 0.42), getFloat(obj, "temp").?);
}

test "getFloat returns float for integer value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try obj.object.put(alloc, "temp", .{ .integer = 1 });
    try std.testing.expectEqual(@as(f64, 1.0), getFloat(obj, "temp").?);
}

test "getStringField returns null for non-object input" {
    try std.testing.expect(getStr(.{ .integer = 42 }, "key") == null);
    try std.testing.expect(getStr(.null, "key") == null);
    try std.testing.expect(getStr(.{ .bool = true }, "key") == null);
}

// ── getInt (nullable i64 variant) ────────────────────────────────────────

test "getInt returns null for missing key" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try std.testing.expect(getInt(obj, "nope") == null);
}

test "getInt returns value for integer" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try obj.object.put(alloc, "n", .{ .integer = -42 });
    try std.testing.expectEqual(@as(i64, -42), getInt(obj, "n").?);
}

test "getInt returns null for non-integer value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try obj.object.put(alloc, "n", .{ .string = "42" });
    try std.testing.expect(getInt(obj, "n") == null);
}

test "getInt returns null for non-object input" {
    try std.testing.expect(getInt(.{ .bool = true }, "k") == null);
    try std.testing.expect(getInt(.null, "k") == null);
}

// ── getFloat invalid types ───────────────────────────────────────────────

test "getFloat returns null for non-numeric value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try obj.object.put(alloc, "f", .{ .string = "not a float" });
    try std.testing.expect(getFloat(obj, "f") == null);
}

test "getFloat returns null for non-object input" {
    try std.testing.expect(getFloat(.null, "k") == null);
    try std.testing.expect(getFloat(.{ .integer = 1 }, "k") == null);
}

// ── getBool invalid types ────────────────────────────────────────────────

// ── T2: Edge — unicode and empty strings ─────────────────────────────────

test "getStr returns unicode multibyte string" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try obj.object.put(alloc, "name", .{ .string = "日本語テスト 🚀" });
    try std.testing.expectEqualStrings("日本語テスト 🚀", getStr(obj, "name").?);
}

test "getStr returns empty string (distinct from missing)" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try obj.object.put(alloc, "empty", .{ .string = "" });
    const result = getStr(obj, "empty");
    try std.testing.expect(result != null);
    try std.testing.expectEqualStrings("", result.?);
}

// ── T2: Edge — zero and boundary integers ────────────────────────────────

test "getInt returns zero for zero value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try obj.object.put(alloc, "n", .{ .integer = 0 });
    try std.testing.expectEqual(@as(i64, 0), getInt(obj, "n").?);
}

test "getInt returns max i64" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try obj.object.put(alloc, "big", .{ .integer = std.math.maxInt(i64) });
    try std.testing.expectEqual(std.math.maxInt(i64), getInt(obj, "big").?);
}

test "getFloat returns null for bool input" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
    defer obj.object.deinit(alloc);
    try obj.object.put(alloc, "f", .{ .bool = true });
    try std.testing.expect(getFloat(obj, "f") == null);
}

// ── T11: Performance — repeated calls no leak ────────────────────────────

test "json helpers 100 iterations no leak" {
    const alloc = std.testing.allocator;
    for (0..100) |_| {
        var obj = std.json.Value{ .object = std.json.ObjectMap.empty };
        try obj.object.put(alloc, "s", .{ .string = "val" });
        try obj.object.put(alloc, "i", .{ .integer = 42 });
        try obj.object.put(alloc, "f", .{ .float = 3.14 });
        try obj.object.put(alloc, "b", .{ .bool = true });

        _ = getStr(obj, "s");
        _ = getInt(obj, "i");
        _ = getFloat(obj, "f");
        _ = getStr(obj, "missing");
        _ = getInt(obj, "missing");

        obj.object.deinit(alloc);
    }
}
