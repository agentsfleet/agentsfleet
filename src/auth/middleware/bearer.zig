//! Shared Bearer-token parsing helper used by the auth middlewares.

const std = @import("std");
const httpz = @import("httpz");

/// Extract the token slice from `Authorization: Bearer <token>`.
/// Returns `null` when the header is missing, malformed, or the token is
/// empty/whitespace — letting callers map to a single 401 branch.
pub fn parseBearerToken(req: *httpz.Request) ?[]const u8 {
    const auth = req.header("authorization") orelse return null;
    const prefix = "Bearer ";
    if (!std.mem.startsWith(u8, auth, prefix)) return null;
    const provided = auth[prefix.len..];
    if (std.mem.trim(u8, provided, " \t\r\n").len == 0) return null;
    return provided;
}

/// Linear scan of a comma-separated rotation list. Trims whitespace around
/// each candidate so operators can format the env var for readability.
pub fn matchRotatedKey(provided: []const u8, configured: []const u8) bool {
    var it = std.mem.tokenizeScalar(u8, configured, ',');
    while (it.next()) |candidate_raw| {
        const candidate = std.mem.trim(u8, candidate_raw, " \t");
        if (candidate.len == 0) continue;
        if (std.mem.eql(u8, provided, candidate)) return true;
    }
    return false;
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

test "matchRotatedKey accepts configured key anywhere in rotation" {
    try testing.expect(matchRotatedKey("key-b", "key-a, key-b, key-c"));
    try testing.expect(matchRotatedKey("key-a", "key-a,key-b"));
    try testing.expect(matchRotatedKey("key-c", "key-a, key-b, key-c"));
}

test "matchRotatedKey rejects non-matches and empty rotation" {
    try testing.expect(!matchRotatedKey("key-z", "key-a, , key-b"));
    try testing.expect(!matchRotatedKey("key-a", ""));
    try testing.expect(!matchRotatedKey("", "key-a"));
}
