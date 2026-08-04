//! The credential-hashing helper every bearer surface shares: runner tokens
//! (`auth/middleware/runner_bearer.zig`), tenant keys
//! (`auth/middleware/tenant_api_key.zig`, `api_keys/tenant.zig`), and the test
//! fixtures that seed them.
//!
//! The `agt_a` fleet-key prefix that used to live here retired with
//! `core.fleet_keys` (M154 §8): its only two readers were the handler-local
//! authentication and the fleet-key management surface, both deleted. What
//! remains is hashing, which was never fleet-key-specific.

const std = @import("std");

/// SHA-256 of input, returned as lower-hex [64]u8. Stack-allocated, no alloc.
pub fn sha256Hex(input: []const u8) [64]u8 {
    var digest: [std.crypto.hash.sha2.Sha256.digest_length]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(input, &digest, .{});
    return std.fmt.bytesToHex(digest, .lower);
}

// Constant-time compare lives ONLY in crypto/hmac_sig.zig (canonical source);
// the former local copy is deleted, callers import the `hmac_sig` module.

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
    const h1 = sha256Hex("agt_aaaaa");
    const h2 = sha256Hex("agt_abbbb");
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
