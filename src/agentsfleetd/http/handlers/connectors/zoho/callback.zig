//! Zoho Desk OAuth callback hook. The generic callback already verified state
//! and exchanged the code; this hook shapes the token body into the vaulted
//! `fleet:zoho` refresh handle.

const std = @import("std");
const pg = @import("pg");
const common = @import("common");
const hx_mod = @import("../../hx.zig");
const vault = @import("../../../../state/vault.zig");
const credential_key = @import("../../../../fleet_runtime/credential_key.zig");
const spec = @import("spec.zig");

const F_ACCESS_TOKEN = "access_token";
const F_REFRESH_TOKEN = "refresh_token";
const F_EXPIRES_IN = "expires_in";
const F_API_DOMAIN = "api_domain";
const DEFAULT_ACCOUNTS_BASE = "https://accounts.zoho.com";
const MS_PER_SECOND: i64 = 1000;
const TEST_EXPIRES_SECONDS: i64 = 3600;
const TEST_EXPIRES_SECONDS_TEXT = std.fmt.comptimePrint("{d}", .{TEST_EXPIRES_SECONDS});

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
    const tok = try parseToken(obj);

    const conn: *pg.Conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
    defer hx.ctx.pool.release(conn);

    try storeHandle(hx, conn, workspace_id, .{
        .integration = spec.PROVIDER,
        .refresh_token = tok.refresh_token,
        .access_token = tok.access_token,
        .expires_at_ms = common.clock.nowMillis() + tok.expires_in_s * MS_PER_SECOND,
        .accounts_base = DEFAULT_ACCOUNTS_BASE,
        .label = tok.api_domain,
    });
}

fn storeHandle(hx: hx_mod.Hx, conn: *pg.Conn, workspace_id: []const u8, handle: Handle) !void {
    const key = try credential_key.allocKeyName(hx.alloc, spec.PROVIDER);
    defer hx.alloc.free(key);
    const json = try std.json.Stringify.valueAlloc(hx.alloc, handle, .{});
    defer hx.alloc.free(json);
    try vault.storeJsonPlaintext(hx.alloc, conn, workspace_id, key, json);
}

const Token = struct {
    access_token: []const u8,
    refresh_token: []const u8,
    expires_in_s: i64,
    api_domain: []const u8,
};

fn parseToken(obj: std.json.ObjectMap) error{ExchangeFailed}!Token {
    return .{
        .access_token = strField(obj, F_ACCESS_TOKEN) orelse return error.ExchangeFailed,
        .refresh_token = strField(obj, F_REFRESH_TOKEN) orelse return error.ExchangeFailed,
        .expires_in_s = intField(obj, F_EXPIRES_IN) orelse return error.ExchangeFailed,
        .api_domain = strField(obj, F_API_DOMAIN) orelse DEFAULT_ACCOUNTS_BASE,
    };
}

fn strField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (obj.get(key) orelse return null) {
        .string => |s| s,
        else => null,
    };
}

fn intField(obj: std.json.ObjectMap, key: []const u8) ?i64 {
    return switch (obj.get(key) orelse return null) {
        .integer => |n| n,
        .float => |n| @intFromFloat(n),
        else => null,
    };
}

const testing = std.testing;

test "zoho parseToken: extracts refresh handle fields" {
    var parsed = try std.json.parseFromSlice(std.json.Value, testing.allocator, "{\"access_token\":\"za\",\"refresh_token\":\"zr\",\"expires_in\":" ++ TEST_EXPIRES_SECONDS_TEXT ++ ",\"api_domain\":\"https://desk.zoho.com\"}", .{});
    defer parsed.deinit();
    const tok = try parseToken(parsed.value.object);
    try testing.expectEqualStrings("za", tok.access_token);
    try testing.expectEqualStrings("zr", tok.refresh_token);
    try testing.expectEqual(TEST_EXPIRES_SECONDS, tok.expires_in_s);
    try testing.expectEqualStrings("https://desk.zoho.com", tok.api_domain);
}

test "zoho parseToken: missing refresh token is rejected" {
    var parsed = try std.json.parseFromSlice(std.json.Value, testing.allocator, "{\"access_token\":\"za\",\"expires_in\":" ++ TEST_EXPIRES_SECONDS_TEXT ++ "}", .{});
    defer parsed.deinit();
    try testing.expectError(error.ExchangeFailed, parseToken(parsed.value.object));
}
