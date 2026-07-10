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
const integration = @import("../../../credentials/integration.zig");

// Field names single-sourced with the broker fingerprint's rotating-credential
// set — the callbacks write exactly the fields the broker excludes from the
// cache identity, so the two lists cannot drift apart.
const F_ACCESS_TOKEN = integration.FIELD_ACCESS_TOKEN;
const F_REFRESH_TOKEN = integration.FIELD_REFRESH_TOKEN;
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

/// Connect-time stamp for the handle's `connected_at_ms` field — the
/// guaranteed-fresh identity field the broker's cache fingerprint keys on.
/// Several providers' other non-rotating fields are constants (Linear's label)
/// or instance-scoped (Zoho's data center, Jira's site), so without this stamp
/// a reconnect to a DIFFERENT account could keep serving the previous
/// account's cached token until expiry. The rotation write-back preserves it,
/// so ordinary refreshes still hit the cache.
pub fn connectedAtMs() i64 {
    return common.clock.nowMillis();
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

/// Merge a rotated refresh token into the CURRENTLY-vaulted handle and
/// re-store it under the same provider key — the mint write-back path for
/// providers that rotate the refresh token on every exchange. This is a
/// guarded merge, not a blind overwrite: the caller's mint ran against a
/// handle snapshot loaded BEFORE the network exchange, and an admin may have
/// reconnected the integration in that window. The handle is re-loaded here
/// and persisted only when its stored refresh token still equals the one the
/// mint posted — otherwise the row now belongs to a newer grant and the
/// write-back is dropped (returns false; worst case is the one forced
/// reconnect the mint already accepted). Rewrites ONLY the refresh_token
/// field of the FRESH row; every other field rides along untouched.
pub fn storeRotatedRefreshToken(
    hx: hx_mod.Hx,
    conn: *pg.Conn,
    provider: []const u8,
    workspace_id: []const u8,
    posted_refresh_token: []const u8,
    rotated_refresh_token: []const u8,
) !bool {
    var current = try vault.loadJson(hx.alloc, conn, workspace_id, provider);
    defer current.deinit();
    if (!refreshTokenEquals(current.value, posted_refresh_token)) return false;
    const obj = switch (current.value) {
        .object => |*o| o,
        else => return vault.Error.NotAnObject, // loadJson guarantees .object; defensive
    };
    try obj.put(current.arena.allocator(), F_REFRESH_TOKEN, .{ .string = rotated_refresh_token });
    try storeHandle(hx, conn, provider, workspace_id, current.value);
    return true;
}

/// Does the stored handle's refresh token equal `posted`? A missing field or
/// non-object shape is a mismatch (the row was replaced or removed).
fn refreshTokenEquals(stored: std.json.Value, posted: []const u8) bool {
    const obj = switch (stored) {
        .object => |o| o,
        else => return false,
    };
    const rt = jsonStr(obj, F_REFRESH_TOKEN) orelse return false;
    return std.mem.eql(u8, rt, posted);
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

test "refreshTokenEquals: the write-back guard matches only the identical stored token" {
    // The guarded merge drops a rotation when the vault row changed under the
    // exchange (a concurrent reconnect): stored ≠ posted, missing field, or a
    // replaced non-object row must all read as a mismatch, never a match.
    var same = try std.json.parseFromSlice(std.json.Value, testing.allocator, "{\"refresh_token\":\"rt_0\"}", .{});
    defer same.deinit();
    try testing.expect(refreshTokenEquals(same.value, "rt_0"));
    try testing.expect(!refreshTokenEquals(same.value, "rt_other"));
    var missing = try std.json.parseFromSlice(std.json.Value, testing.allocator, "{\"integration\":\"zoho\"}", .{});
    defer missing.deinit();
    try testing.expect(!refreshTokenEquals(missing.value, "rt_0"));
    try testing.expect(!refreshTokenEquals(.null, "rt_0"));
}
