//! Zoho Desk OAuth callback hook. The generic callback already verified state,
//! resolved the data-center-specific token endpoint (`multi_dc.zig`, keyed off
//! the callback's `location` param), and exchanged the code there; this hook
//! shapes the token body into the vaulted `fleet:zoho` refresh handle. The
//! shared triple parse + vault store live in `../oauth_refresh.zig`.

const std = @import("std");
const pg = @import("pg");
const hx_mod = @import("../../hx.zig");
const oauth_refresh = @import("../oauth_refresh.zig");
const spec = @import("spec.zig");
const multi_dc = @import("multi_dc.zig");

const F_API_DOMAIN = "api_domain";

const Handle = struct {
    integration: []const u8,
    refresh_token: []const u8,
    access_token: []const u8,
    expires_at_ms: i64,
    accounts_base: []const u8,
    label: []const u8,
};

pub fn postAuth(hx: hx_mod.Hx, workspace_id: []const u8, body: []const u8, location: ?[]const u8) anyerror!void {
    var parsed = std.json.parseFromSlice(std.json.Value, hx.alloc, body, .{}) catch return error.ExchangeFailed;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return error.ExchangeFailed,
    };
    const tok = try oauth_refresh.parseRefreshTriple(obj);
    // The data-center accounts server this refresh token is redeemable at —
    // derived from the SAME `location` signal the exchange itself was routed
    // by (multi_dc.zig), not guessed and not re-derived from `api_domain`
    // (which names the API host, a related but distinct value, kept below
    // only as an informational label).
    const accounts_base = multi_dc.accountsBase(location);
    const api_domain = oauth_refresh.jsonStr(obj, F_API_DOMAIN) orelse accounts_base;

    const conn: *pg.Conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
    defer hx.ctx.pool.release(conn);

    try oauth_refresh.storeHandle(hx, conn, spec.PROVIDER, workspace_id, Handle{
        .integration = spec.PROVIDER,
        .refresh_token = tok.refresh_token,
        .access_token = tok.access_token,
        .expires_at_ms = oauth_refresh.expiresAtMs(tok.expires_in_s),
        .accounts_base = accounts_base,
        .label = api_domain,
    });
}
