//! Shared plumbing for the oauth2 refresh-token callbacks (Zoho, Jira, Linear).
//! Every provider's post-auth hook parses the same `{access_token,
//! refresh_token, expires_in}` triple and vaults a `fleet:<provider>` handle the
//! broker later mints from; only the extra fields differ (Zoho's data-center
//! base, Jira's cloud id). This module owns the identical parts — the JSON
//! accessors, the triple parse, the expiry math, and the generic vault store —
//! so a new refresh provider is a ~20-line delta, not a copy-pasted file.

const std = @import("std");
const pg = @import("pg");
const common = @import("common");
const hx_mod = @import("../hx.zig");
const vault = @import("../../../state/vault.zig");

const F_ACCESS_TOKEN = "access_token";
const F_REFRESH_TOKEN = "refresh_token";
const F_EXPIRES_IN = "expires_in";
const MS_PER_SECOND: i64 = 1000;

/// The refresh-token triple every provider's token response carries.
pub const RefreshTriple = struct {
    access_token: []const u8,
    refresh_token: []const u8,
    expires_in_s: i64,
};

/// Parse the common triple from an already-decoded token-response object. A
/// missing access token, refresh token, or expiry is a failed exchange.
pub fn parseRefreshTriple(obj: std.json.ObjectMap) error{ExchangeFailed}!RefreshTriple {
    return .{
        .access_token = jsonStr(obj, F_ACCESS_TOKEN) orelse return error.ExchangeFailed,
        .refresh_token = jsonStr(obj, F_REFRESH_TOKEN) orelse return error.ExchangeFailed,
        .expires_in_s = jsonInt(obj, F_EXPIRES_IN) orelse return error.ExchangeFailed,
    };
}

/// Absolute expiry (ms) from a relative `expires_in` (seconds).
pub fn expiresAtMs(expires_in_s: i64) i64 {
    return common.clock.nowMillis() + expires_in_s * MS_PER_SECOND;
}

/// Optional string field — null when absent or non-string.
pub fn jsonStr(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (obj.get(key) orelse return null) {
        .string => |s| s,
        else => null,
    };
}

/// Optional integer field — accepts a JSON integer or float; null otherwise.
pub fn jsonInt(obj: std.json.ObjectMap, key: []const u8) ?i64 {
    return switch (obj.get(key) orelse return null) {
        .integer => |n| n,
        .float => |n| @intFromFloat(n),
        else => null,
    };
}

/// Serialize `handle` (any struct whose fields are the wire shape) and vault it
/// under `<provider>`. Each provider owns its own handle struct.
pub fn storeHandle(hx: hx_mod.Hx, conn: *pg.Conn, provider: []const u8, workspace_id: []const u8, handle: anytype) !void {
    const json = try std.json.Stringify.valueAlloc(hx.alloc, handle, .{});
    defer hx.alloc.free(json);
    try vault.storeJsonPlaintext(hx.alloc, conn, workspace_id, provider, json);
}

const testing = std.testing;
const TEST_EXPIRES_SECONDS: i64 = 3600;
const TEST_EXPIRES_SECONDS_TEXT = std.fmt.comptimePrint("{d}", .{TEST_EXPIRES_SECONDS});

test "parseRefreshTriple: extracts access/refresh/expiry" {
    var parsed = try std.json.parseFromSlice(std.json.Value, testing.allocator, "{\"access_token\":\"a\",\"refresh_token\":\"r\",\"expires_in\":" ++ TEST_EXPIRES_SECONDS_TEXT ++ "}", .{});
    defer parsed.deinit();
    const tok = try parseRefreshTriple(parsed.value.object);
    try testing.expectEqualStrings("a", tok.access_token);
    try testing.expectEqualStrings("r", tok.refresh_token);
    try testing.expectEqual(TEST_EXPIRES_SECONDS, tok.expires_in_s);
}

test "parseRefreshTriple: missing refresh token is rejected" {
    var parsed = try std.json.parseFromSlice(std.json.Value, testing.allocator, "{\"access_token\":\"a\",\"expires_in\":" ++ TEST_EXPIRES_SECONDS_TEXT ++ "}", .{});
    defer parsed.deinit();
    try testing.expectError(error.ExchangeFailed, parseRefreshTriple(parsed.value.object));
}

test "jsonInt: accepts a float expiry" {
    var parsed = try std.json.parseFromSlice(std.json.Value, testing.allocator, "{\"expires_in\":" ++ TEST_EXPIRES_SECONDS_TEXT ++ ".0}", .{});
    defer parsed.deinit();
    try testing.expectEqual(TEST_EXPIRES_SECONDS, jsonInt(parsed.value.object, "expires_in").?);
}
