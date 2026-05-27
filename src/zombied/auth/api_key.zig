//! Shared helpers for zmb_ agent-key authentication.
//! Used by integration_grants/handler.zig and api_keys/agent.zig.

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

test "sha256Hex: output is always 64 hex chars" {
    const h = sha256Hex("any input at all");
    try std.testing.expectEqual(@as(usize, 64), h.len);
}

test "sha256Hex: empty string produces known hash" {
    const h = sha256Hex("");
    try std.testing.expectEqualStrings(
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        h[0..],
    );
}

test "sha256Hex: different inputs produce different hashes" {
    const h1 = sha256Hex("zmb_aaaa");
    const h2 = sha256Hex("zmb_bbbb");
    // Compare as slices — [64]u8 arrays are always equal length so check content.
    try std.testing.expect(!std.mem.eql(u8, h1[0..], h2[0..]));
}

test "sha256Hex: output contains only lowercase hex chars" {
    const h = sha256Hex("test-key-value");
    for (h) |c| {
        const is_hex = (c >= '0' and c <= '9') or (c >= 'a' and c <= 'f');
        try std.testing.expect(is_hex);
    }
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

test "constantTimeEql: empty strings are equal" {
    try std.testing.expect(constantTimeEql("", ""));
}

test "constantTimeEql: only last byte differs returns false (RULE CTM — no short-circuit)" {
    // XOR accumulation means last-byte difference is caught even if first N-1 bytes match.
    try std.testing.expect(!constantTimeEql("abc1", "abc2"));
}

test "constantTimeEql: only first byte differs returns false" {
    try std.testing.expect(!constantTimeEql("Xbc", "abc"));
}

test "constantTimeEql: single char equal" {
    try std.testing.expect(constantTimeEql("x", "x"));
}

test "constantTimeEql: single char different" {
    try std.testing.expect(!constantTimeEql("x", "y"));
}
