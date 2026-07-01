//! GET /v1/workspaces/{ws}/connectors/slack — workspace-authed.
//!
//! Reports the Slack connector state for the workspace: "connected" plus the
//! Slack team name when the `fleet:slack` vault handle exists (written by
//! `callback.zig`), else "not_connected". Never fabricates a connected state —
//! a missing/unreadable handle reads as not connected. `team` is the workspace
//! name (not a secret) so the dashboard can render "Slack connected: {team}".

const common = @import("../../common.zig");
const hx_mod = @import("../../hx.zig");
const ec = @import("../../../../errors/error_registry.zig");
const vault = @import("../../../../state/vault.zig");
const credential_key = @import("../../../../fleet_runtime/credential_key.zig");
const spec = @import("spec.zig");

const std = @import("std");

const F_BOT_TOKEN = "bot_token";
const F_TEAM_NAME = "team_name";
const STATUS_CONNECTED = "connected";
const STATUS_NOT_CONNECTED = "not_connected";
const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";

pub fn innerSlackStatus(hx: hx_mod.Hx, workspace_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    // Load the `fleet:slack` handle; connected iff it exists with a bot_token.
    // team_name (the workspace's Slack name, not a secret) is surfaced so the
    // dashboard renders which team is connected.
    const key = credential_key.allocKeyName(hx.alloc, spec.PROVIDER) catch return respondNotConnected(hx);
    defer hx.alloc.free(key);
    var parsed = vault.loadJson(hx.alloc, conn, workspace_id, key) catch return respondNotConnected(hx);
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return respondNotConnected(hx),
    };
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
