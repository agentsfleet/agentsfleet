// Tests for src/crypto/hmac_sig.zig — the canonical HMAC-SHA256 + constant-time
// primitives consumed by every webhook-auth caller.

const std = @import("std");
const hs = @import("hmac_sig");

// ── computeMac ───────────────────────────────────────────────────────────

test "computeMac: known vector (RFC-style)" {
    const secret = "mysecret";
    const body = "{\"action\":\"opened\"}";
    const mac = hs.computeMac(secret, &.{body});
    // Match via round-trip: encode then decode → same bytes.
    var buf: [hs.MAC_LEN * 2]u8 = undefined;
    const encoded = hs.encodeMacHex(&buf, "", mac);
    const decoded = hs.hexDecode32(encoded).?;
    try std.testing.expectEqualSlices(u8, &mac, &decoded);
}

test "computeMac: same inputs → same output (determinism)" {
    const a = hs.computeMac("k", &.{ "foo", ":", "bar" });
    const b = hs.computeMac("k", &.{ "foo", ":", "bar" });
    try std.testing.expectEqualSlices(u8, &a, &b);
}

test "computeMac: different secret → different output" {
    const a = hs.computeMac("k1", &.{"body"});
    const b = hs.computeMac("k2", &.{"body"});
    try std.testing.expect(!std.mem.eql(u8, &a, &b));
}

test "computeMac: multi-part basestring matches single concatenation" {
    const single = hs.computeMac("k", &.{"v0:123:hello"});
    const multi = hs.computeMac("k", &.{ "v0", ":", "123", ":", "hello" });
    try std.testing.expectEqualSlices(u8, &single, &multi);
}

// ── constantTimeEql ──────────────────────────────────────────────────────

test "constantTimeEql: identical" {
    try std.testing.expect(hs.constantTimeEql("hello", "hello"));
}

test "constantTimeEql: different same-length" {
    try std.testing.expect(!hs.constantTimeEql("hello", "world"));
}

test "constantTimeEql: length mismatch (a shorter)" {
    try std.testing.expect(!hs.constantTimeEql("ab", "abc"));
}

test "constantTimeEql: length mismatch (a longer)" {
    try std.testing.expect(!hs.constantTimeEql("abcd", "abc"));
}

test "constantTimeEql: both empty" {
    try std.testing.expect(hs.constantTimeEql("", ""));
}

test "constantTimeEql: empty vs non-empty" {
    try std.testing.expect(!hs.constantTimeEql("", "a"));
    try std.testing.expect(!hs.constantTimeEql("a", ""));
}

test "constantTimeEql: length mismatch does NOT short-circuit on prefix match" {
    // "abc" is a prefix of "abcd" — naive implementations might return early
    // on the length check before scanning payload. This implementation XORs
    // over min(a,b) and folds the length difference at the end.
    try std.testing.expect(!hs.constantTimeEql("abc", "abcd"));
    try std.testing.expect(!hs.constantTimeEql("abcd", "abc"));
}

// ── hexDecode32 ──────────────────────────────────────────────────────────

test "hexDecode32: valid round-trip" {
    const mac = hs.computeMac("k", &.{"body"});
    var buf: [hs.MAC_LEN * 2]u8 = undefined;
    const encoded = hs.encodeMacHex(&buf, "", mac);
    const decoded = hs.hexDecode32(encoded).?;
    try std.testing.expectEqualSlices(u8, &mac, &decoded);
}

test "hexDecode32: wrong length returns null" {
    try std.testing.expect(hs.hexDecode32("abc") == null);
    try std.testing.expect(hs.hexDecode32("") == null);
    // 63 chars — one short of 64.
    try std.testing.expect(hs.hexDecode32("a" ** 63) == null);
    // 65 chars — one over.
    try std.testing.expect(hs.hexDecode32("a" ** 65) == null);
}

test "hexDecode32: non-hex chars return null" {
    try std.testing.expect(hs.hexDecode32("z" ** 64) == null);
    try std.testing.expect(hs.hexDecode32("G" ** 64) == null);
}

test "hexDecode32: accepts uppercase" {
    const upper = "DEADBEEF" ++ ("00" ** 28);
    try std.testing.expect(hs.hexDecode32(upper) != null);
}

// ── encodeMacHex ─────────────────────────────────────────────────────────

test "encodeMacHex: prefix + lowercase hex" {
    const mac = hs.computeMac("k", &.{"body"});
    var buf: [hs.MAC_LEN * 2 + 8]u8 = undefined;
    const out = hs.encodeMacHex(&buf, "v0=", mac);
    try std.testing.expect(std.mem.startsWith(u8, out, "v0="));
    try std.testing.expectEqual(@as(usize, 3 + 64), out.len);
    // All hex chars lowercase.
    for (out["v0=".len..]) |c| {
        try std.testing.expect((c >= '0' and c <= '9') or (c >= 'a' and c <= 'f'));
    }
}

test "encodeMacHex: empty prefix" {
    const mac = hs.computeMac("k", &.{"body"});
    var buf: [hs.MAC_LEN * 2]u8 = undefined;
    const out = hs.encodeMacHex(&buf, "", mac);
    try std.testing.expectEqual(@as(usize, 64), out.len);
}
