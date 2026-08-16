//! GitHub user-authorization proof for an App installation callback. A claimed
//! installation id is verified directly. When the App already exists but the
//! agentsfleet binding does not, the same user token discovers the unique
//! accessible installation and repairs that drift without trusting GitHub UI
//! state.

const std = @import("std");
const hx_mod = @import("../../hx.zig");
const bounded_fetch = @import("../bounded_fetch.zig");
const oauth2 = @import("../oauth2.zig");
const spec = @import("spec.zig");

const API_BASE = "https://api.github.com";
const INSTALLATIONS_PATH_FMT = "/user/installations/{s}/repositories?per_page=1";
const INSTALLATIONS_LIST_PATH = "/user/installations?per_page=2";
const FIELD_ACCESS_TOKEN = "access_token";
const FIELD_INSTALLATIONS = "installations";
const FIELD_ID = "id";
const HEADER_ACCEPT = "accept";
const HEADER_AUTHORIZATION = "authorization";
const HEADER_API_VERSION = "x-github-api-version";
const CONTENT_TYPE_JSON = "application/json";
const AUTHORIZATION_BEARER_FMT = "Bearer {s}";
const API_VERSION = "2026-03-10";
const HTTP_OK: u16 = 200;

pub const Resolution = union(enum) {
    none,
    one: []const u8,
    multiple,

    pub fn deinit(self: Resolution, alloc: std.mem.Allocator) void {
        switch (self) {
            .one => |installation_id| alloc.free(installation_id),
            .none, .multiple => {},
        }
    }
};

pub const Proof = struct {
    resolution: Resolution,
    token: []const u8,

    pub fn deinit(self: Proof, alloc: std.mem.Allocator) void {
        self.resolution.deinit(alloc);
        alloc.free(self.token);
    }
};

/// Exchange the callback code and resolve the installation accessible to this
/// GitHub App user token. The caller may retain the token only for the bounded
/// App-install continuation when no installation exists yet.
pub fn resolve(hx: hx_mod.Hx, code: []const u8, claimed_installation_id: ?[]const u8, redirect_uri: []const u8) anyerror!Proof {
    const creds = try loadCreds(hx);
    defer creds.deinit(hx.alloc);

    var token_spec = spec.USER_AUTH;
    if (hx.ctx.connector_oauth_token_endpoint_override) |endpoint| token_spec.token_endpoint = endpoint;
    const result = try oauth2.exchange(hx.alloc, hx.ctx.io, hx.ctx.deadline_scheduler, token_spec, creds, code, redirect_uri);
    defer hx.alloc.free(result.body);
    if (result.status != HTTP_OK) return error.ExchangeFailed;

    const token = try parseAccessToken(hx.alloc, result.body);
    errdefer hx.alloc.free(token);
    if (claimed_installation_id) |installation_id| {
        try verifyInstallation(hx, token, installation_id);
        return .{ .resolution = .{ .one = try hx.alloc.dupe(u8, installation_id) }, .token = token };
    }
    return .{ .resolution = try listInstallations(hx, token), .token = token };
}

pub fn verifyClaim(hx: hx_mod.Hx, token: []const u8, installation_id: []const u8) !void {
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
    const authorization = try std.fmt.allocPrint(hx.alloc, AUTHORIZATION_BEARER_FMT, .{token});
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

fn listInstallations(hx: hx_mod.Hx, token: []const u8) !Resolution {
    const endpoint = try std.fmt.allocPrint(hx.alloc, "{s}{s}", .{
        hx.ctx.connector_github_api_base_override orelse API_BASE,
        INSTALLATIONS_LIST_PATH,
    });
    defer hx.alloc.free(endpoint);
    const authorization = try std.fmt.allocPrint(hx.alloc, AUTHORIZATION_BEARER_FMT, .{token});
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
    return parseInstallations(hx.alloc, response.body);
}

fn parseInstallations(alloc: std.mem.Allocator, body: []const u8) !Resolution {
    var parsed = std.json.parseFromSlice(std.json.Value, alloc, body, .{}) catch return error.OwnershipDenied;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |value| value,
        else => return error.OwnershipDenied,
    };
    const items = switch (obj.get(FIELD_INSTALLATIONS) orelse return error.OwnershipDenied) {
        .array => |value| value.items,
        else => return error.OwnershipDenied,
    };
    if (items.len == 0) return .none;
    if (items.len != 1) return .multiple;
    const item = switch (items[0]) {
        .object => |value| value,
        else => return error.OwnershipDenied,
    };
    const id = switch (item.get(FIELD_ID) orelse return error.OwnershipDenied) {
        .integer => |value| try std.fmt.allocPrint(alloc, "{d}", .{value}),
        .string => |value| try alloc.dupe(u8, value),
        else => return error.OwnershipDenied,
    };
    return .{ .one = id };
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

test "should resolve zero, one, and multiple accessible installations" {
    try testing.expectEqual(Resolution.none, try parseInstallations(testing.allocator, "{\"installations\":[]}"));

    const one = try parseInstallations(testing.allocator, "{\"installations\":[{\"id\":424242}]}");
    defer one.deinit(testing.allocator);
    try testing.expectEqualStrings("424242", one.one);

    const multiple = try parseInstallations(testing.allocator, "{\"installations\":[{\"id\":1},{\"id\":2}]}");
    try testing.expectEqual(Resolution.multiple, multiple);
}

test "should reject malformed installation discovery responses" {
    try testing.expectError(error.OwnershipDenied, parseInstallations(testing.allocator, "{}"));
    try testing.expectError(error.OwnershipDenied, parseInstallations(testing.allocator, "{\"installations\":[{}]}"));
    try testing.expectError(error.OwnershipDenied, parseInstallations(testing.allocator, "not-json"));
}
