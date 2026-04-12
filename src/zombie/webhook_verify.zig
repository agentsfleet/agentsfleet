// Webhook signature verification — config-driven, multi-provider.
//
// Each provider is a VerifyConfig entry. Adding a new provider = one new const.
// No switch statements, no per-provider functions.
// Uses constant-time comparison to prevent timing side-channels (RULE CTM).

const std = @import("std");
const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;
const ec = @import("../errors/error_registry.zig");

/// Configuration for verifying a webhook provider's signature.
pub const VerifyConfig = struct {
    /// Header carrying the signature (e.g., "x-slack-signature").
    sig_header: []const u8,
    /// Header carrying the timestamp (null if provider doesn't use one).
    ts_header: ?[]const u8 = null,
    /// Prefix before the hex digest in the signature value (e.g., "v0=", "sha256=", "").
    prefix: []const u8,
    /// Version string prepended to the HMAC basestring (e.g., "v0" for Slack).
    /// Only used when includes_timestamp is true. Empty string if not needed.
    hmac_version: []const u8 = "",
    /// Whether the HMAC input includes "{hmac_version}:{timestamp}:{body}" or just body.
    includes_timestamp: bool = false,
    /// Max allowed clock drift in seconds (only used when ts_header is set).
    max_ts_drift_seconds: i64 = ec.SLACK_MAX_TS_DRIFT_SECONDS,
};

// ── Provider configs ──────────────────────────────────────────────────────

pub const SLACK = VerifyConfig{
    .sig_header = ec.SLACK_SIG_HEADER,
    .ts_header = ec.SLACK_TS_HEADER,
    .prefix = "v0=",
    .hmac_version = ec.SLACK_SIG_VERSION,
    .includes_timestamp = true,
};

pub const GITHUB = VerifyConfig{
    .sig_header = "x-hub-signature-256",
    .prefix = "sha256=",
};

pub const LINEAR = VerifyConfig{
    .sig_header = "linear-signature",
    .prefix = "",
};

// ── Public API ────────────────────────────────────────────────────────────

/// Verify a webhook signature using the given config.
/// Returns true if the signature is valid.
pub fn verifySignature(
    cfg: VerifyConfig,
    secret: []const u8,
    timestamp: ?[]const u8,
    body: []const u8,
    signature: []const u8,
) bool {
    // Strip prefix
    if (!std.mem.startsWith(u8, signature, cfg.prefix)) return false;
    const hex_part = signature[cfg.prefix.len..];

    // Decode expected hash
    const expected = hexDecode(hex_part) orelse return false;

    // Compute HMAC
    var mac: [HmacSha256.mac_length]u8 = undefined;
    var hmac = HmacSha256.init(secret);
    if (cfg.includes_timestamp) {
        const ts = timestamp orelse return false;
        hmac.update(cfg.hmac_version);
        hmac.update(":");
        hmac.update(ts);
        hmac.update(":");
    }
    hmac.update(body);
    hmac.final(&mac);

    return constantTimeEql(&mac, &expected);
}

/// Check if a timestamp string is within the allowed drift window.
/// Rejects future timestamps (ts > now) — an attacker cannot pre-sign requests.
/// Allows a small forward tolerance (max_drift seconds) for clock skew only.
pub fn isTimestampFresh(timestamp: []const u8, max_drift: i64) bool {
    const ts = std.fmt.parseInt(i64, timestamp, 10) catch return false;
    if (ts <= 0) return false;
    const now = std.time.timestamp();
    // Reject far-future timestamps; allow only max_drift seconds of forward clock skew.
    if (ts > now + max_drift) return false;
    // Reject stale timestamps.
    if (now > ts and now - ts > max_drift) return false;
    return true;
}

// ── Internal ──────────────────────────────────────────────────────────────

fn hexDecode(hex: []const u8) ?[32]u8 {
    if (hex.len != 64) return null;
    var out: [32]u8 = undefined;
    _ = std.fmt.hexToBytes(&out, hex) catch return null;
    return out;
}

fn constantTimeEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var diff: u8 = 0;
    for (a, b) |x, y| diff |= x ^ y;
    return diff == 0;
}

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

// ── Tests ─────────────────────────────────────────────────────────────────

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
    // 1 hour in the future — must be rejected to prevent pre-signed request attacks
    const future = std.time.timestamp() + 3600;
    const s = std.fmt.bufPrint(&buf, "{d}", .{future}) catch unreachable;
    try std.testing.expect(!isTimestampFresh(s, 300));
}

test "isTimestampFresh: small forward clock skew accepted (within max_drift)" {
    var buf: [20]u8 = undefined;
    // 10 seconds in the future — within the 300s drift window (clock skew tolerance)
    const slightly_future = std.time.timestamp() + 10;
    const s = std.fmt.bufPrint(&buf, "{d}", .{slightly_future}) catch unreachable;
    try std.testing.expect(isTimestampFresh(s, 300));
}

test "wrong prefix rejected" {
    try std.testing.expect(!verifySignature(GITHUB, "s", null, "b", "v0=abcd"));
}

test "short hex rejected" {
    try std.testing.expect(!verifySignature(GITHUB, "s", null, "b", "sha256=abcd"));
}

test "constantTimeEql: identical" {
    try std.testing.expect(constantTimeEql("hello", "hello"));
}

test "constantTimeEql: different" {
    try std.testing.expect(!constantTimeEql("hello", "world"));
}

test "constantTimeEql: length mismatch" {
    try std.testing.expect(!constantTimeEql("ab", "abc"));
}

test "constantTimeEql: both empty slices are equal" {
    // Zero-length path: the for-loop body never runs, diff stays 0 → true.
    try std.testing.expect(constantTimeEql("", ""));
}

test "constantTimeEql: empty vs non-empty is false (length guard)" {
    try std.testing.expect(!constantTimeEql("", "a"));
}

// ── T2: isTimestampFresh boundary cases ──────────────────────────────────────

test "isTimestampFresh: at exactly max_drift seconds old is accepted" {
    var buf: [20]u8 = undefined;
    // now - max_drift: on the boundary (just within window).
    const at_boundary = std.time.timestamp() - 300;
    const s = std.fmt.bufPrint(&buf, "{d}", .{at_boundary}) catch unreachable;
    // Implementation: accepted when now - ts <= max_drift (boundary inclusive on <=).
    try std.testing.expect(isTimestampFresh(s, 300));
}

test "isTimestampFresh: at max_drift + 1 seconds old is rejected" {
    var buf: [20]u8 = undefined;
    // now - (max_drift + 1): one second past the window.
    const just_outside = std.time.timestamp() - 301;
    const s = std.fmt.bufPrint(&buf, "{d}", .{just_outside}) catch unreachable;
    try std.testing.expect(!isTimestampFresh(s, 300));
}

test "isTimestampFresh: at exactly max_drift seconds ahead is accepted (clock skew)" {
    var buf: [20]u8 = undefined;
    const at_forward_boundary = std.time.timestamp() + 300;
    const s = std.fmt.bufPrint(&buf, "{d}", .{at_forward_boundary}) catch unreachable;
    // On the boundary: ts == now + max_drift → ts > now + max_drift is false → accepted.
    try std.testing.expect(isTimestampFresh(s, 300));
}

test "isTimestampFresh: at max_drift + 1 seconds ahead is rejected (pre-sign attack)" {
    var buf: [20]u8 = undefined;
    const just_outside_future = std.time.timestamp() + 301;
    const s = std.fmt.bufPrint(&buf, "{d}", .{just_outside_future}) catch unreachable;
    try std.testing.expect(!isTimestampFresh(s, 300));
}
