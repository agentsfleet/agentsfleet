// M8_001 Slack Plugin — regression pins and API contract tests.
//
// This file covers spec-claims from M8_001_SLACK_PLUGIN_ACQUISITION.md that
// require cross-module assertions (error registry + handler constants) and
// cannot be expressed as inline tests within a single file.
//
// Tiers covered: T7 (regression safety nets), T8 (security), T3 (negative)
//
// Error code → HTTP status pins must not drift silently. If a status changes,
// the Slack dashboard redirect ("denied", "connected") may surface the wrong
// code to the user, and client retry logic in the CLI will break.

const std = @import("std");
const httpz = @import("httpz");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");

// ── T7: Error code HTTP status regression pins ────────────────────────────────

test "UZ-SLACK-001 (OAuth state invalid) → HTTP 403 Forbidden" {
    // CSRF / state mismatch. 403 signals the client should not retry with same
    // state — it must restart the OAuth flow from scratch.
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    common.errorResponse(ht.res, ec.ERR_SLACK_OAUTH_STATE, "csrf", "r1");
    try ht.expectStatus(403);
}

test "UZ-SLACK-002 (token exchange failed) → HTTP 502 Bad Gateway" {
    // Slack API is upstream dependency; 502 tells the caller that the fault
    // is upstream, not the client's request.
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    common.errorResponse(ht.res, ec.ERR_SLACK_TOKEN_EXCHANGE, "exchange", "r2");
    try ht.expectStatus(502);
}

test "UZ-SLACK-003 (bot token expired) → HTTP 401 Unauthorized" {
    // Revoked or expired bot token. 401 signals that reauthentication is required.
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    common.errorResponse(ht.res, ec.ERR_SLACK_TOKEN_EXPIRED, "expired", "r3");
    try ht.expectStatus(401);
}

test "UZ-WH-010 (invalid Slack signature) → HTTP 401 Unauthorized" {
    // Bad signature → request is not from Slack. 401 chosen over 403 because
    // the identity (it being Slack) cannot be confirmed.
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    common.errorResponse(ht.res, ec.ERR_WEBHOOK_SIG_INVALID, "bad sig", "r4");
    try ht.expectStatus(401);
}

test "UZ-WH-011 (stale timestamp) → HTTP 401 Unauthorized" {
    // Replay attack / too-old timestamp. Same 401 family as signature failure —
    // we cannot confirm the request is fresh from Slack.
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    common.errorResponse(ht.res, ec.ERR_WEBHOOK_TIMESTAMP_STALE, "old ts", "r5");
    try ht.expectStatus(401);
}

// ── T7: Constant regression pins ─────────────────────────────────────────────

test "SLACK_MAX_TS_DRIFT_SECONDS is exactly 300 (5-minute window)" {
    // Pin: Slack's documented freshness window is 5 minutes. If this changes,
    // legitimate requests near the boundary will be rejected.
    try std.testing.expectEqual(@as(i64, 300), ec.SLACK_MAX_TS_DRIFT_SECONDS);
}

test "SLACK_SIG_VERSION is \"v0\" (Slack signing protocol version)" {
    // Pin: Slack's HMAC basestring uses "v0:" prefix. Wrong version = all
    // signatures fail verification silently.
    try std.testing.expectEqualStrings("v0", ec.SLACK_SIG_VERSION);
}

test "GATE_PENDING_TTL_SECONDS is at least 3600 (1-hour approval window)" {
    // Gate TTL must allow enough time for a human to review and act. If this
    // drops below 3600, approvals sent after 1 hour will be silently dropped.
    try std.testing.expect(ec.GATE_PENDING_TTL_SECONDS >= 3600);
}

test "GATE_PENDING_KEY_PREFIX contains \"gate\" (routing invariant)" {
    // Approval gate GETDEL keys must use this prefix. Bot loop detection and
    // GETDEL atomicity depend on the exact prefix format. Pin to catch renames.
    try std.testing.expect(std.mem.indexOf(u8, ec.GATE_PENDING_KEY_PREFIX, "gate") != null);
}

// ── T8: Security invariants ───────────────────────────────────────────────────

test "SLACK_SIG_HEADER is lowercase (HTTP header comparison must be case-insensitive)" {
    // httpz normalises headers to lowercase. If we ever compare with a
    // non-lowercase constant, lookups will silently fail on some proxies.
    for (ec.SLACK_SIG_HEADER) |c| {
        try std.testing.expect(c != std.ascii.toUpper(c) or c == '-');
    }
}

test "SLACK_TS_HEADER is lowercase" {
    for (ec.SLACK_TS_HEADER) |c| {
        try std.testing.expect(c != std.ascii.toUpper(c) or c == '-');
    }
}

test "error response body contains error_code field (RFC 7807 contract)" {
    // Clients parse error_code to decide retry vs. show-user-message. If the
    // field is missing the CLI falls back to opaque 4xx/5xx handling.
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    common.errorResponse(ht.res, ec.ERR_SLACK_OAUTH_STATE, "detail", "req-999");
    const body = try ht.getBody();
    try std.testing.expect(std.mem.indexOf(u8, body, "error_code") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "UZ-SLACK-001") != null);
}

test "error response Content-Type is application/problem+json (RFC 7807)" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    common.errorResponse(ht.res, ec.ERR_SLACK_TOKEN_EXCHANGE, "detail", "req-998");
    try ht.expectHeader("Content-Type", "application/problem+json");
}
