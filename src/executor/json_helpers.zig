//! Shared JSON value helpers for the executor module.
//!
//! Used by handler.zig, runner.zig, and client.zig to extract typed
//! fields from std.json.Value objects. Consolidated here to avoid
//! duplication across the executor module boundary.
//!
//! Tests live in `json_helpers_test.zig`.

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

pub fn getObjectParam(obj: std.json.Value, key: []const u8) ?std.json.Value {
    if (obj != .object) return null;
    const val = obj.object.get(key) orelse return null;
    return if (val == .object) val else null;
}

pub fn getArrayParam(obj: std.json.Value, key: []const u8) ?std.json.Value {
    if (obj != .object) return null;
    const val = obj.object.get(key) orelse return null;
    return if (val == .array) val else null;
}

/// Escape a string for embedding inside a JSON string literal.
/// Caller owns returned slice.
pub fn escapeAlloc(alloc: std.mem.Allocator, input: []const u8) ![]u8 {
    var buf: std.ArrayList(u8) = .{};
    errdefer buf.deinit(alloc);

    for (input) |c| {
        switch (c) {
            '"' => try buf.appendSlice(alloc, "\\\""),
            '\\' => try buf.appendSlice(alloc, "\\\\"),
            '\n' => try buf.appendSlice(alloc, "\\n"),
            '\r' => try buf.appendSlice(alloc, "\\r"),
            '\t' => try buf.appendSlice(alloc, "\\t"),
            else => {
                if (c < 0x20) {
                    var tmp: [6]u8 = undefined;
                    const written = std.fmt.bufPrint(&tmp, "\\u{x:0>4}", .{c}) catch continue;
                    try buf.appendSlice(alloc, written);
                } else {
                    try buf.append(alloc, c);
                }
            },
        }
    }
    return buf.toOwnedSlice(alloc);
}

/// Decode a JSON array of strings into `[]const []const u8` allocated on
/// `alloc`. Non-array input collapses to an empty slice; non-string
/// elements are silently skipped. Strict typing happens via OpenAPI / spec
/// enforcement upstream — the executor is permissive at the wire to keep
/// failure modes coarse.
pub fn strArray(alloc: std.mem.Allocator, v: std.json.Value) ![]const []const u8 {
    if (v != .array) return &.{};
    const items = v.array.items;
    const out = try alloc.alloc([]const u8, items.len);
    var n: usize = 0;
    for (items) |item| {
        if (item == .string) {
            out[n] = item.string;
            n += 1;
        }
    }
    return out[0..n];
}

test {
    _ = @import("json_helpers_test.zig");
}
