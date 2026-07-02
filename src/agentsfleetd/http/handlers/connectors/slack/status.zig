//! Slack status hook — the provider delta the generic status handler
//! (`connectors/status.zig`) dispatches to. Renders "connected" plus the
//! Slack team name when the `fleet:slack` vault handle carries a bot token,
//! else "not_connected". Never fabricates a connected state — a missing or
//! unreadable handle reads as not connected. `team` is the workspace's Slack
//! name (not a secret) so the dashboard can render "Slack connected: {team}".

const std = @import("std");
const hx_mod = @import("../../hx.zig");

const F_BOT_TOKEN = "bot_token";
const F_TEAM_NAME = "team_name";
const STATUS_CONNECTED = "connected";
const STATUS_NOT_CONNECTED = "not_connected";

/// Registry `respond_status` hook: `handle` is the parsed `fleet:slack` vault
/// object (null = missing/unreadable). Owns the full response body.
pub fn respondStatus(hx: hx_mod.Hx, handle: ?std.json.ObjectMap) void {
    const obj = handle orelse return respondNotConnected(hx);
    if (obj.get(F_BOT_TOKEN) == null) return respondNotConnected(hx);
    hx.ok(.ok, .{ .status = STATUS_CONNECTED, .team = strField(obj, F_TEAM_NAME) });
}

fn respondNotConnected(hx: hx_mod.Hx) void {
    hx.ok(.ok, .{ .status = STATUS_NOT_CONNECTED, .team = @as(?[]const u8, null) });
}

fn strField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (obj.get(key) orelse return null) {
        .string => |s| s,
        else => null,
    };
}
