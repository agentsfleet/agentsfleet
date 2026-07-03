//! Jira Cloud OAuth callback hook. The generic callback exchanges the code;
//! this hook resolves the Atlassian cloud id through the documented accessible
//! resources endpoint and stores a refresh-token handle in the vault.

const std = @import("std");
const pg = @import("pg");
const common = @import("common");
const hx_mod = @import("../../hx.zig");
const bounded_fetch = @import("../bounded_fetch.zig");
const vault = @import("../../../../state/vault.zig");
const credential_key = @import("../../../../fleet_runtime/credential_key.zig");
const spec = @import("spec.zig");

const F_ACCESS_TOKEN = "access_token";
const F_REFRESH_TOKEN = "refresh_token";
const F_EXPIRES_IN = "expires_in";
const F_ID = "id";
const F_NAME = "name";
const F_URL = "url";
const HEADER_ACCEPT = "accept";
const HEADER_AUTHORIZATION = "authorization";
const CONTENT_TYPE_JSON = "application/json";
const ACCESSIBLE_RESOURCES_PATH = "/oauth/token/accessible-resources";
const MS_PER_SECOND: i64 = 1000;
const TEST_EXPIRES_SECONDS: i64 = 3600;
const TEST_EXPIRES_SECONDS_TEXT = std.fmt.comptimePrint("{d}", .{TEST_EXPIRES_SECONDS});

const Handle = struct {
    integration: []const u8,
    refresh_token: []const u8,
    access_token: []const u8,
    expires_at_ms: i64,
    cloud_id: []const u8,
    site_url: []const u8,
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
    const resource = try resolveResource(hx, tok.access_token);

    const conn: *pg.Conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
    defer hx.ctx.pool.release(conn);

    try storeHandle(hx, conn, workspace_id, .{
        .integration = spec.PROVIDER,
        .refresh_token = tok.refresh_token,
        .access_token = tok.access_token,
        .expires_at_ms = common.clock.nowMillis() + tok.expires_in_s * MS_PER_SECOND,
        .cloud_id = resource.cloud_id,
        .site_url = resource.site_url,
        .label = resource.name,
    });
}

fn storeHandle(hx: hx_mod.Hx, conn: *pg.Conn, workspace_id: []const u8, handle: Handle) !void {
    const key = try credential_key.allocKeyName(hx.alloc, spec.PROVIDER);
    defer hx.alloc.free(key);
    const json = try std.json.Stringify.valueAlloc(hx.alloc, handle, .{});
    defer hx.alloc.free(json);
    try vault.storeJsonPlaintext(hx.alloc, conn, workspace_id, key, json);
}

const Token = struct { access_token: []const u8, refresh_token: []const u8, expires_in_s: i64 };

fn parseToken(obj: std.json.ObjectMap) error{ExchangeFailed}!Token {
    return .{
        .access_token = strField(obj, F_ACCESS_TOKEN) orelse return error.ExchangeFailed,
        .refresh_token = strField(obj, F_REFRESH_TOKEN) orelse return error.ExchangeFailed,
        .expires_in_s = intField(obj, F_EXPIRES_IN) orelse return error.ExchangeFailed,
    };
}

const Resource = struct { cloud_id: []const u8, site_url: []const u8, name: []const u8 };

fn resolveResource(hx: hx_mod.Hx, access_token: []const u8) anyerror!Resource {
    const endpoint = try accessibleResourcesEndpoint(hx);
    defer hx.alloc.free(endpoint);
    const auth = try std.fmt.allocPrint(hx.alloc, "Bearer {s}", .{access_token});
    defer hx.alloc.free(auth);
    const headers = [_]std.http.Header{
        .{ .name = HEADER_AUTHORIZATION, .value = auth },
        .{ .name = HEADER_ACCEPT, .value = CONTENT_TYPE_JSON },
    };
    var wd: bounded_fetch.Watchdog = .{};
    defer wd.deinit();
    const resp = try bounded_fetch.fetch(hx.alloc, hx.ctx.io, &wd, .{
        .url = endpoint,
        .method = .GET,
        .extra_headers = &headers,
        .deadline_ms = bounded_fetch.TOKEN_EXCHANGE_DEADLINE_MS,
        .provider = spec.PROVIDER,
        .class = .token_exchange,
    });
    defer hx.alloc.free(resp.body);
    if (resp.status < 200 or resp.status >= 300) return error.ExchangeFailed;
    return parseFirstResource(hx.alloc, resp.body);
}

fn accessibleResourcesEndpoint(hx: hx_mod.Hx) ![]const u8 {
    const override = hx.ctx.connector_oauth_token_endpoint_override orelse
        return hx.alloc.dupe(u8, spec.ACCESSIBLE_RESOURCES_ENDPOINT);
    const origin = try originFromAbsoluteUrl(hx.alloc, override);
    defer hx.alloc.free(origin);
    return std.fmt.allocPrint(hx.alloc, "{s}" ++ ACCESSIBLE_RESOURCES_PATH, .{origin});
}

fn originFromAbsoluteUrl(alloc: std.mem.Allocator, url: []const u8) ![]const u8 {
    const scheme_end = std.mem.indexOf(u8, url, "://") orelse return error.ExchangeFailed;
    const host_start = scheme_end + 3;
    const path_start = std.mem.indexOfPos(u8, url, host_start, "/") orelse url.len;
    return alloc.dupe(u8, url[0..path_start]);
}

fn parseFirstResource(alloc: std.mem.Allocator, body: []const u8) !Resource {
    var parsed = std.json.parseFromSlice(std.json.Value, alloc, body, .{}) catch return error.ExchangeFailed;
    defer parsed.deinit();
    const arr = switch (parsed.value) {
        .array => |a| a,
        else => return error.ExchangeFailed,
    };
    if (arr.items.len == 0) return error.ExchangeFailed;
    const obj = switch (arr.items[0]) {
        .object => |o| o,
        else => return error.ExchangeFailed,
    };
    return .{
        .cloud_id = try alloc.dupe(u8, strField(obj, F_ID) orelse return error.ExchangeFailed),
        .site_url = try alloc.dupe(u8, strField(obj, F_URL) orelse ""),
        .name = try alloc.dupe(u8, strField(obj, F_NAME) orelse ""),
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

test "jira parseToken: extracts refresh-token pair" {
    var parsed = try std.json.parseFromSlice(std.json.Value, testing.allocator, "{\"access_token\":\"ja\",\"refresh_token\":\"jr\",\"expires_in\":" ++ TEST_EXPIRES_SECONDS_TEXT ++ "}", .{});
    defer parsed.deinit();
    const tok = try parseToken(parsed.value.object);
    try testing.expectEqualStrings("ja", tok.access_token);
    try testing.expectEqualStrings("jr", tok.refresh_token);
    try testing.expectEqual(TEST_EXPIRES_SECONDS, tok.expires_in_s);
}

test "jira parseFirstResource: extracts cloud id" {
    const got = try parseFirstResource(testing.allocator, "[{\"id\":\"cloud-1\",\"name\":\"Acme Jira\",\"url\":\"https://acme.atlassian.net\"}]");
    defer testing.allocator.free(got.cloud_id);
    defer testing.allocator.free(got.site_url);
    defer testing.allocator.free(got.name);
    try testing.expectEqualStrings("cloud-1", got.cloud_id);
    try testing.expectEqualStrings("Acme Jira", got.name);
    try testing.expectEqualStrings("https://acme.atlassian.net", got.site_url);
}

test "jira accessibleResourcesEndpoint: derives loopback origin from token override" {
    const origin = try originFromAbsoluteUrl(testing.allocator, "http://127.0.0.1:4123/oauth/token");
    defer testing.allocator.free(origin);
    try testing.expectEqualStrings("http://127.0.0.1:4123", origin);
}
