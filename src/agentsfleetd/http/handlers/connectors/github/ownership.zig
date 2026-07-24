//! GitHub user-authorization proof for an App installation callback.
//! The setup URL's `installation_id` is untrusted until a short-lived GitHub
//! user token can access that installation through the user-installations API.

const std = @import("std");
const hx_mod = @import("../../hx.zig");
const bounded_fetch = @import("../bounded_fetch.zig");
const connect_h = @import("../connect.zig");
const oauth2 = @import("../oauth2.zig");
const spec = @import("spec.zig");

const TOKEN_ENDPOINT = "https://github.com/login/oauth/access_token";
const API_BASE = "https://api.github.com";
const INSTALLATIONS_PATH_FMT = "/user/installations/{s}/repositories?per_page=1";
const FIELD_ACCESS_TOKEN = "access_token";
const HEADER_ACCEPT = "accept";
const HEADER_AUTHORIZATION = "authorization";
const HEADER_API_VERSION = "x-github-api-version";
const CONTENT_TYPE_JSON = "application/json";
const API_VERSION = "2026-03-10";
const HTTP_OK: u16 = 200;

const TOKEN_SPEC = oauth2.Spec{
    .provider = spec.PROVIDER,
    .authorize_endpoint = "https://github.com/login/oauth/authorize",
    .token_endpoint = TOKEN_ENDPOINT,
    .scopes = "read:user",
    .state = spec.STATE,
};

/// Exchange the callback code and prove the authenticated GitHub user can
/// access `installation_id`. The user token is request-local and never stored.
pub fn verify(hx: hx_mod.Hx, code: []const u8, installation_id: []const u8) anyerror!void {
    const creds = try loadCreds(hx);
    defer creds.deinit(hx.alloc);
    const redirect_uri = try connect_h.callbackUrl(hx, spec.PROVIDER);
    defer hx.alloc.free(redirect_uri);

    var token_spec = TOKEN_SPEC;
    if (hx.ctx.connector_oauth_token_endpoint_override) |endpoint| token_spec.token_endpoint = endpoint;
    const result = try oauth2.exchange(hx.alloc, hx.ctx.io, hx.ctx.deadline_scheduler, token_spec, creds, code, redirect_uri);
    defer hx.alloc.free(result.body);
    if (result.status != HTTP_OK) return error.ExchangeFailed;

    const token = try parseAccessToken(hx.alloc, result.body);
    defer hx.alloc.free(token);
    try verifyInstallation(hx, token, installation_id);
}

fn loadCreds(hx: hx_mod.Hx) !oauth2.AppCreds {
    const conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
    defer hx.ctx.pool.release(conn);
    return oauth2.loadAppCreds(hx.alloc, conn, hx.ctx.platform_admin_workspace_id, spec.PROVIDER) orelse error.NotConfigured;
}

fn parseAccessToken(alloc: std.mem.Allocator, body: []const u8) ![]const u8 {
    var parsed = std.json.parseFromSlice(std.json.Value, alloc, body, .{}) catch return error.ExchangeFailed;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |value| value,
        else => return error.ExchangeFailed,
    };
    const field = obj.get(FIELD_ACCESS_TOKEN) orelse return error.ExchangeFailed;
    const token = switch (field) {
        .string => |value| value,
        else => return error.ExchangeFailed,
    };
    if (token.len == 0) return error.ExchangeFailed;
    return alloc.dupe(u8, token);
}

fn verifyInstallation(hx: hx_mod.Hx, token: []const u8, installation_id: []const u8) !void {
    const endpoint = try installationEndpoint(hx.alloc, hx.ctx.connector_github_api_base_override orelse API_BASE, installation_id);
    defer hx.alloc.free(endpoint);
    const authorization = try std.fmt.allocPrint(hx.alloc, "Bearer {s}", .{token});
    defer hx.alloc.free(authorization);
    const headers = [_]std.http.Header{
        .{ .name = HEADER_ACCEPT, .value = CONTENT_TYPE_JSON },
        .{ .name = HEADER_AUTHORIZATION, .value = authorization },
        .{ .name = HEADER_API_VERSION, .value = API_VERSION },
    };
    const response = try bounded_fetch.fetch(hx.alloc, hx.ctx.io, hx.ctx.deadline_scheduler, .{
        .url = endpoint,
        .method = .GET,
        .extra_headers = &headers,
        .deadline_ms = bounded_fetch.TOKEN_EXCHANGE_DEADLINE_MS,
        .provider = spec.PROVIDER,
        .class = .installation_verify,
    });
    defer hx.alloc.free(response.body);
    if (response.status != HTTP_OK) return error.OwnershipDenied;
}

fn installationEndpoint(alloc: std.mem.Allocator, base: []const u8, installation_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(alloc, "{s}" ++ INSTALLATIONS_PATH_FMT, .{ base, installation_id });
}

const testing = std.testing;

test "should reject malformed GitHub user-token responses" {
    try testing.expectError(error.ExchangeFailed, parseAccessToken(testing.allocator, "{}"));
    try testing.expectError(error.ExchangeFailed, parseAccessToken(testing.allocator, "{\"access_token\":\"\"}"));
    try testing.expectError(error.ExchangeFailed, parseAccessToken(testing.allocator, "not-json"));
}

test "should build the installation ownership endpoint" {
    const endpoint = try installationEndpoint(testing.allocator, "https://api.github.test", "424242");
    defer testing.allocator.free(endpoint);
    try testing.expectEqualStrings(
        "https://api.github.test/user/installations/424242/repositories?per_page=1",
        endpoint,
    );
}
