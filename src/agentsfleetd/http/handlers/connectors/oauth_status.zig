//! Generic status hook for OAuth providers whose vault handle carries an
//! `integration` marker and optional display label. No secret leaves the vault.

const std = @import("std");
const hx_mod = @import("../hx.zig");

const F_INTEGRATION = "integration";
const F_LABEL = "label";
const STATUS_CONNECTED = "connected";
const STATUS_NOT_CONNECTED = "not_connected";

pub fn respondStatus(hx: hx_mod.Hx, handle: ?std.json.ObjectMap) void {
    const obj = handle orelse return respondNotConnected(hx);
    if (obj.get(F_INTEGRATION) == null) return respondNotConnected(hx);
    hx.ok(.ok, .{ .status = STATUS_CONNECTED, .label = strField(obj, F_LABEL) });
}

fn respondNotConnected(hx: hx_mod.Hx) void {
    hx.ok(.ok, .{ .status = STATUS_NOT_CONNECTED, .label = @as(?[]const u8, null) });
}

fn strField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (obj.get(key) orelse return null) {
        .string => |s| s,
        else => null,
    };
}
