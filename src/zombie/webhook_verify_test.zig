// Tests for src/zombie/webhook_verify.zig.
//
// Extracted from webhook_verify.zig to keep the production file under the
// 350-line RULE FLL gate (test files are exempt). Do not add non-test code
// here — keep it next to the verify logic.

const std = @import("std");
const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;
const wv = @import("webhook_verify.zig");
const verifySignature = wv.verifySignature;
const isTimestampFresh = wv.isTimestampFresh;
const detectProvider = wv.detectProvider;
const NoHeaders = wv.NoHeaders;
const SLACK = wv.SLACK;
const GITHUB = wv.GITHUB;
const LINEAR = wv.LINEAR;

// ── Helpers ──────────────────────────────────────────────────────────────

/// Compute HMAC and format as "prefix<hex>". Test helper only.
fn testSig(
    buf: []u8,
    secret: []const u8,
    parts: []const []const u8,
    prefix: []const u8,
) []const u8 {
    var mac: [HmacSha256.mac_length]u8 = undefined;
    var hmac = HmacSha256.init(secret);
    for (parts) |p| hmac.update(p);
    hmac.final(&mac);
    const hex = std.fmt.bytesToHex(mac, .lower);
    @memcpy(buf[0..prefix.len], prefix);
    @memcpy(buf[prefix.len .. prefix.len + 64], &hex);
    return buf[0 .. prefix.len + 64];
}

/// Minimal header bag for detectProvider tests. Mirrors the `.header(name)`
/// shape of the HTTP request type without depending on `src/http/`.
const FakeHeaders = struct {
    entries: []const struct { name: []const u8, value: []const u8 },
    pub fn header(self: FakeHeaders, name: []const u8) ?[]const u8 {
        for (self.entries) |e| {
            if (std.ascii.eqlIgnoreCase(e.name, name)) return e.value;
        }
        return null;
    }
};

// ── verifySignature ──────────────────────────────────────────────────────

test "Slack: valid signature accepted" {
    const secret = "test_signing_secret_not_real_00";
    const ts = "1531420618";
    const body = "payload=test_webhook_body_data";
    var buf: [67]u8 = undefined;
    const sig = testSig(&buf, secret, &.{ "v0:", ts, ":", body }, "v0=");
    try std.testing.expect(verifySignature(SLACK, secret, ts, body, sig));
}

test "Slack: tampered body rejected" {
    const secret = "test_signing_secret_not_real_00";
    const ts = "1531420618";
    const body = "payload=test_webhook_body_data";
    var buf: [67]u8 = undefined;
    const sig = testSig(&buf, secret, &.{ "v0:", ts, ":", body }, "v0=");
    try std.testing.expect(!verifySignature(SLACK, secret, ts, "tampered", sig));
}

test "Slack: missing timestamp rejected" {
    try std.testing.expect(!verifySignature(SLACK, "s", null, "b", "v0=abcd"));
}

test "GitHub: valid signature accepted" {
    const secret = "mysecret";
    const body = "{\"action\":\"opened\"}";
    var buf: [71]u8 = undefined;
    const sig = testSig(&buf, secret, &.{body}, "sha256=");
    try std.testing.expect(verifySignature(GITHUB, secret, null, body, sig));
}

test "GitHub: wrong body rejected" {
    const secret = "mysecret";
    const body = "{\"action\":\"opened\"}";
    var buf: [71]u8 = undefined;
    const sig = testSig(&buf, secret, &.{body}, "sha256=");
    try std.testing.expect(!verifySignature(GITHUB, secret, null, "wrong", sig));
}

test "Linear: valid signature accepted" {
    const secret = "linearsecret";
    const body = "{\"type\":\"Issue\"}";
    var buf: [64]u8 = undefined;
    const sig = testSig(&buf, secret, &.{body}, "");
    try std.testing.expect(verifySignature(LINEAR, secret, null, body, sig));
}

test "wrong prefix rejected" {
    try std.testing.expect(!verifySignature(GITHUB, "s", null, "b", "v0=abcd"));
}

test "short hex rejected" {
    try std.testing.expect(!verifySignature(GITHUB, "s", null, "b", "sha256=abcd"));
}

// ── isTimestampFresh ─────────────────────────────────────────────────────

test "isTimestampFresh: recent accepted" {
    var buf: [20]u8 = undefined;
    const now = std.time.timestamp();
    const s = std.fmt.bufPrint(&buf, "{d}", .{now}) catch unreachable;
    try std.testing.expect(isTimestampFresh(s, 300));
}

test "isTimestampFresh: old rejected" {
    var buf: [20]u8 = undefined;
    const old = std.time.timestamp() - 600;
    const s = std.fmt.bufPrint(&buf, "{d}", .{old}) catch unreachable;
    try std.testing.expect(!isTimestampFresh(s, 300));
}

test "isTimestampFresh: garbage rejected" {
    try std.testing.expect(!isTimestampFresh("not_a_number", 300));
    try std.testing.expect(!isTimestampFresh("", 300));
}

test "isTimestampFresh: negative timestamp rejected (overflow protection)" {
    try std.testing.expect(!isTimestampFresh("-9223372036854775808", 300));
    try std.testing.expect(!isTimestampFresh("-1", 300));
    try std.testing.expect(!isTimestampFresh("0", 300));
}

test "isTimestampFresh: far-future timestamp rejected (no pre-signed requests)" {
    var buf: [20]u8 = undefined;
    const future = std.time.timestamp() + 3600;
    const s = std.fmt.bufPrint(&buf, "{d}", .{future}) catch unreachable;
    try std.testing.expect(!isTimestampFresh(s, 300));
}

test "isTimestampFresh: small forward clock skew accepted (within max_drift)" {
    var buf: [20]u8 = undefined;
    const slightly_future = std.time.timestamp() + 10;
    const s = std.fmt.bufPrint(&buf, "{d}", .{slightly_future}) catch unreachable;
    try std.testing.expect(isTimestampFresh(s, 300));
}

test "isTimestampFresh: at exactly max_drift seconds old is accepted" {
    var buf: [20]u8 = undefined;
    const at_boundary = std.time.timestamp() - 300;
    const s = std.fmt.bufPrint(&buf, "{d}", .{at_boundary}) catch unreachable;
    try std.testing.expect(isTimestampFresh(s, 300));
}

test "isTimestampFresh: at max_drift + 1 seconds old is rejected" {
    var buf: [20]u8 = undefined;
    const just_outside = std.time.timestamp() - 301;
    const s = std.fmt.bufPrint(&buf, "{d}", .{just_outside}) catch unreachable;
    try std.testing.expect(!isTimestampFresh(s, 300));
}

test "isTimestampFresh: at exactly max_drift seconds ahead is accepted (clock skew)" {
    var buf: [20]u8 = undefined;
    const at_forward_boundary = std.time.timestamp() + 300;
    const s = std.fmt.bufPrint(&buf, "{d}", .{at_forward_boundary}) catch unreachable;
    try std.testing.expect(isTimestampFresh(s, 300));
}

test "isTimestampFresh: at max_drift + 1 seconds ahead is rejected (pre-sign attack)" {
    var buf: [20]u8 = undefined;
    const just_outside_future = std.time.timestamp() + 301;
    const s = std.fmt.bufPrint(&buf, "{d}", .{just_outside_future}) catch unreachable;
    try std.testing.expect(!isTimestampFresh(s, 300));
}

// constantTimeEql tests live in src/crypto/hmac_sig_test.zig (canonical source).

// ── detectProvider (§2) ──────────────────────────────────────────────────

test "detectProvider: github by source (dim 2.1)" {
    const got = detectProvider("github", NoHeaders{}) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings("github", got.name);
    try std.testing.expectEqualStrings("x-hub-signature-256", got.sig_header);
}

test "detectProvider: linear by source (dim 2.2, Q1 first-class)" {
    const got = detectProvider("linear", NoHeaders{}) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings("linear", got.name);
    try std.testing.expectEqualStrings("linear-signature", got.sig_header);
}

test "detectProvider: unknown source falls back to header (dim 2.3)" {
    const headers = FakeHeaders{ .entries = &.{
        .{ .name = "x-hub-signature-256", .value = "sha256=abc" },
    } };
    const got = detectProvider("some-unknown-provider", headers) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings("github", got.name);
}

test "detectProvider: no match returns null (dim 2.4)" {
    const headers = FakeHeaders{ .entries = &.{
        .{ .name = "x-random-signature", .value = "whatever" },
    } };
    try std.testing.expect(detectProvider("nope", headers) == null);
    // Empty source with empty headers also yields null.
    try std.testing.expect(detectProvider("", NoHeaders{}) == null);
}

test "detectProvider: source match is case-insensitive" {
    const got = detectProvider("GitHub", NoHeaders{}) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings("github", got.name);
}
