//! Jira Cloud OAuth callback hook. The generic callback exchanges the code; this
//! hook resolves the Atlassian cloud id through the documented accessible-
//! resources endpoint and stores a refresh-token handle in the vault. The shared
//! triple parse + vault store live in `../oauth_refresh.zig`; Jira's delta is the
//! cloud-id resolution.

const std = @import("std");
const pg = @import("pg");
const hx_mod = @import("../../hx.zig");
const bounded_fetch = @import("../bounded_fetch.zig");
const oauth_refresh = @import("../oauth_refresh.zig");
const spec = @import("spec.zig");

const F_ID = "id";
const F_NAME = "name";
const F_URL = "url";
const HEADER_ACCEPT = "accept";
const HEADER_AUTHORIZATION = "authorization";
const CONTENT_TYPE_JSON = "application/json";
const ACCESSIBLE_RESOURCES_PATH = "/oauth/token/accessible-resources";

const Handle = struct {
    integration: []const u8,
    refresh_token: []const u8,
    access_token: []const u8,
    expires_at_ms: i64,
    connected_at_ms: i64,
    cloud_id: []const u8,
    site_url: []const u8,
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
    const resource = try resolveResource(hx, tok.access_token);

    const conn: *pg.Conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
    defer hx.ctx.pool.release(conn);

    try oauth_refresh.storeHandle(hx, conn, spec.PROVIDER, workspace_id, Handle{
        .integration = spec.PROVIDER,
        .refresh_token = tok.refresh_token,
        .access_token = tok.access_token,
        .expires_at_ms = oauth_refresh.expiresAtMs(tok.expires_in_s),
        .connected_at_ms = oauth_refresh.connectedAtMs(),
        .cloud_id = resource.cloud_id,
        .site_url = resource.site_url,
        .label = resource.name,
    });
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
    const resp = try bounded_fetch.fetch(hx.alloc, hx.ctx.io, hx.ctx.deadline_scheduler, .{
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
        .cloud_id = try alloc.dupe(u8, oauth_refresh.jsonStr(obj, F_ID) orelse return error.ExchangeFailed),
        .site_url = try alloc.dupe(u8, oauth_refresh.jsonStr(obj, F_URL) orelse ""),
        .name = try alloc.dupe(u8, oauth_refresh.jsonStr(obj, F_NAME) orelse ""),
    };
}

const testing = std.testing;

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
