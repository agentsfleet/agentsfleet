//! Tests for json_helpers.zig — split per RULE FLL.

const std = @import("std");
const json = @import("json_helpers.zig");

const getStr = json.getStr;
const getInt = json.getInt;
const getIntOrZero = json.getIntOrZero;
const getFloat = json.getFloat;
const getBool = json.getBool;
const strArray = json.strArray;


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

test "strArray returns empty for non-array input" {
    const out = try strArray(std.testing.allocator, .{ .integer = 1 });
    defer std.testing.allocator.free(out);
    try std.testing.expectEqual(@as(usize, 0), out.len);
}

test "strArray drops non-string elements silently" {
    const alloc = std.testing.allocator;
    var arr: std.json.Array = .init(alloc);
    defer arr.deinit();
    try arr.append(.{ .string = "keep1" });
    try arr.append(.{ .integer = 42 });
    try arr.append(.{ .string = "keep2" });
    try arr.append(.{ .bool = true });
    const out = try strArray(alloc, .{ .array = arr });
    defer alloc.free(out);
    try std.testing.expectEqual(@as(usize, 2), out.len);
    try std.testing.expectEqualStrings("keep1", out[0]);
    try std.testing.expectEqualStrings("keep2", out[1]);
}

test "strArray returns empty slice for empty array" {
    const alloc = std.testing.allocator;
    var arr: std.json.Array = .init(alloc);
    defer arr.deinit();
    const out = try strArray(alloc, .{ .array = arr });
    defer alloc.free(out);
    try std.testing.expectEqual(@as(usize, 0), out.len);
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

// ── getBool invalid types ────────────────────────────────────────────────

test "getBool returns false for non-object input" {
    try std.testing.expect(!getBool(.null, "k"));
    try std.testing.expect(!getBool(.{ .integer = 1 }, "k"));
}

test "getBool returns false for non-bool string value" {
    const alloc = std.testing.allocator;
    var obj = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer obj.object.deinit();
    try obj.object.put("flag", .{ .string = "true" });
    try std.testing.expect(!getBool(obj, "flag"));
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
        _ = getStr(obj, "missing");
        _ = getInt(obj, "missing");

        obj.object.deinit();
    }
}
