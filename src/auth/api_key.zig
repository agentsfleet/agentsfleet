//! Shared helpers for zmb_ external agent key authentication.
//! Used by execute.zig, integration_grants.zig, and external_agents.zig.

const std = @import("std");

/// SHA-256 of input, returned as lower-hex [64]u8. Stack-allocated, no alloc.
pub fn sha256Hex(input: []const u8) [64]u8 {
    var digest: [std.crypto.hash.sha2.Sha256.digest_length]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(input, &digest, .{});
    return std.fmt.bytesToHex(digest, .lower);
}

/// XOR accumulation — constant time regardless of first differing byte (RULE CTM).
pub fn constantTimeEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var diff: u8 = 0;
    for (a, b) |x, y| diff |= x ^ y;
    return diff == 0;
}

// ── Tests ──────────────────────────────────────────────────────────────────

test "sha256Hex: stable output" {
    const h = sha256Hex("hello");
    try std.testing.expectEqualStrings(
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        h[0..],
    );
}

test "constantTimeEql: equal slices" {
    try std.testing.expect(constantTimeEql("abc", "abc"));
}

test "constantTimeEql: different slices same length" {
    try std.testing.expect(!constantTimeEql("abc", "abd"));
}

test "constantTimeEql: different lengths" {
    try std.testing.expect(!constantTimeEql("abc", "abcd"));
}
