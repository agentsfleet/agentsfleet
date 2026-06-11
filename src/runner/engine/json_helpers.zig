//! Shared JSON value helpers for the runner module.
//!
//! Used by runner.zig, runner_helpers.zig, and context_budget.zig to
//! extract typed fields from std.json.Value objects. Consolidated here to
//! avoid duplication across the runner module boundary.
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

pub fn getFloat(obj: std.json.Value, key: []const u8) ?f64 {
    if (obj != .object) return null;
    const val = obj.object.get(key) orelse return null;
    return switch (val) {
        .float => |f| f,
        .integer => |i| @as(f64, @floatFromInt(i)),
        else => null,
    };
}

test {
    _ = @import("json_helpers_test.zig");
}
