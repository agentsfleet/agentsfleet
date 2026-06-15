// Tests for src/agent/webhook_verify.zig.
//
// Extracted from webhook_verify.zig to keep the production file under the
// 350-line RULE FLL gate (test files are exempt). Do not add non-test code
// here — keep it next to the verify logic.
//
// Signature verification itself lives in the auth middleware
// (auth/middleware/webhook_sig.zig) and is tested there; this file covers the
// provider-metadata surface (detectProvider) plus the shared timestamp
// freshness helpers the registry consumers rely on.

const std = @import("std");
const clock = @import("common").clock;
const hs = @import("hmac_sig");
const wv = @import("webhook_verify.zig");
const isTimestampFresh = hs.isTimestampFresh;
const isTimestampFreshAt = hs.isTimestampFreshAt;
const detectProvider = wv.detectProvider;
const NoHeaders = wv.NoHeaders;

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

// ── isTimestampFresh ─────────────────────────────────────────────────────

test "isTimestampFresh: recent accepted" {
    var buf: [20]u8 = undefined;
    const now = clock.nowSeconds();
    const s = std.fmt.bufPrint(&buf, "{d}", .{now}) catch unreachable;
    try std.testing.expect(isTimestampFresh(s, 300));
}

test "isTimestampFresh: old rejected" {
    var buf: [20]u8 = undefined;
    const old = clock.nowSeconds() - 600;
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
    const future = clock.nowSeconds() + 3600;
    const s = std.fmt.bufPrint(&buf, "{d}", .{future}) catch unreachable;
    try std.testing.expect(!isTimestampFresh(s, 300));
}

test "isTimestampFresh: small forward clock skew accepted (within max_drift)" {
    var buf: [20]u8 = undefined;
    const slightly_future = clock.nowSeconds() + 10;
    const s = std.fmt.bufPrint(&buf, "{d}", .{slightly_future}) catch unreachable;
    try std.testing.expect(isTimestampFresh(s, 300));
}

// Exact-edge cases assert against an injected `now` (isTimestampFreshAt) so they
// pin the ±max_drift boundary without racing two live-clock reads: the test reads
// the clock, then isTimestampFresh reads it again, and a second tick between them
// shifts the edge — fatal at an exact boundary and routine under valgrind.
test "isTimestampFresh: at exactly max_drift seconds old is accepted" {
    try std.testing.expect(isTimestampFreshAt("1699999700", 1700000000, 300));
}

test "isTimestampFresh: at max_drift + 1 seconds old is rejected" {
    try std.testing.expect(!isTimestampFreshAt("1699999699", 1700000000, 300));
}

test "isTimestampFresh: at exactly max_drift seconds ahead is accepted (clock skew)" {
    try std.testing.expect(isTimestampFreshAt("1700000300", 1700000000, 300));
}

test "isTimestampFresh: at max_drift + 1 seconds ahead is rejected (pre-sign attack)" {
    try std.testing.expect(!isTimestampFreshAt("1700000301", 1700000000, 300));
}

// constantTimeEql tests live in src/crypto/hmac_sig_test.zig (canonical source).

// ── detectProvider ───────────────────────────────────────────────────────

test "detectProvider: github by source" {
    const got = detectProvider("github", NoHeaders{}) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings("github", got.name);
    try std.testing.expectEqualStrings("x-hub-signature-256", got.sig_header);
}

test "detectProvider: linear by source" {
    const got = detectProvider("linear", NoHeaders{}) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings("linear", got.name);
    try std.testing.expectEqualStrings("linear-signature", got.sig_header);
}

test "detectProvider: unknown source falls back to header" {
    const headers = FakeHeaders{ .entries = &.{
        .{ .name = "x-hub-signature-256", .value = "sha256=abc" },
    } };
    const got = detectProvider("some-unknown-provider", headers) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings("github", got.name);
}

test "detectProvider: no match returns null" {
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
