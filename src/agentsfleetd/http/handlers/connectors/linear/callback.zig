//! Linear OAuth callback hook. Linear now returns refresh-token pairs; keep the
//! refresh token vaulted so the broker can mint fresh access tokens later. The
//! shared triple parse + vault store live in `../oauth_refresh.zig`; this hook is
//! just Linear's handle shape.

const std = @import("std");
const pg = @import("pg");
const hx_mod = @import("../../hx.zig");
const oauth_refresh = @import("../oauth_refresh.zig");
const spec = @import("spec.zig");

const LABEL_LINEAR = "Linear";

const Handle = struct {
    integration: []const u8,
    refresh_token: []const u8,
    access_token: []const u8,
    expires_at_ms: i64,
    label: []const u8,
};

pub fn postAuth(hx: hx_mod.Hx, workspace_id: []const u8, body: []const u8, _: ?[]const u8) anyerror!void {
    var parsed = std.json.parseFromSlice(std.json.Value, hx.alloc, body, .{}) catch return error.ExchangeFailed;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return error.ExchangeFailed,
    };
    const tok = try oauth_refresh.parseRefreshTriple(obj);

    const conn: *pg.Conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
    defer hx.ctx.pool.release(conn);

    try oauth_refresh.storeHandle(hx, conn, spec.PROVIDER, workspace_id, Handle{
        .integration = spec.PROVIDER,
        .refresh_token = tok.refresh_token,
        .access_token = tok.access_token,
        .expires_at_ms = oauth_refresh.expiresAtMs(tok.expires_in_s),
        .label = LABEL_LINEAR,
    });
}
