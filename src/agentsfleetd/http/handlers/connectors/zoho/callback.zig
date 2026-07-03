//! Zoho Desk OAuth callback hook. The generic callback already verified state
//! and exchanged the code; this hook shapes the token body into the vaulted
//! `fleet:zoho` refresh handle. The shared triple parse + vault store live in
//! `../oauth_refresh.zig`; Zoho's delta is its data-center `api_domain`.

const std = @import("std");
const pg = @import("pg");
const hx_mod = @import("../../hx.zig");
const oauth_refresh = @import("../oauth_refresh.zig");
const spec = @import("spec.zig");

const F_API_DOMAIN = "api_domain";
const DEFAULT_ACCOUNTS_BASE = "https://accounts.zoho.com";

const Handle = struct {
    integration: []const u8,
    refresh_token: []const u8,
    access_token: []const u8,
    expires_at_ms: i64,
    accounts_base: []const u8,
    label: []const u8,
};

pub fn postAuth(hx: hx_mod.Hx, workspace_id: []const u8, body: []const u8) anyerror!void {
    var parsed = std.json.parseFromSlice(std.json.Value, hx.alloc, body, .{}) catch return error.ExchangeFailed;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return error.ExchangeFailed,
    };
    const tok = try oauth_refresh.parseRefreshTriple(obj);
    // Zoho's data-center-specific base URL is captured from the token response's
    // issuing host rather than guessed.
    const api_domain = oauth_refresh.jsonStr(obj, F_API_DOMAIN) orelse DEFAULT_ACCOUNTS_BASE;

    const conn: *pg.Conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
    defer hx.ctx.pool.release(conn);

    try oauth_refresh.storeHandle(hx, conn, spec.PROVIDER, workspace_id, Handle{
        .integration = spec.PROVIDER,
        .refresh_token = tok.refresh_token,
        .access_token = tok.access_token,
        .expires_at_ms = oauth_refresh.expiresAtMs(tok.expires_in_s),
        .accounts_base = DEFAULT_ACCOUNTS_BASE,
        .label = api_domain,
    });
}
