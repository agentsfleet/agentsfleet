//! Pure Slack v0 request-signature verdict — the ingress-side counterpart to
//! the `webhook_sig` middleware's `verifyHmac`, but principal-free and
//! Ctx-free so `connectors/slack/events.zig` can call it inline and own the
//! `UZ-SLK-010`/`UZ-SLK-011` + HTTP-status mapping itself.
//!
//! Slack signs the basestring `v0:{timestamp}:{raw_body}` and sends
//! `X-Slack-Signature: v0=<hex>` + `X-Slack-Request-Timestamp`. This reuses the
//! canonical HMAC primitives (`hmac_sig`) and the Slack `VerifyConfig`
//! (`webhook_verify.SLACK`) so the header names, `v0` version, and 300 s drift
//! window live in exactly one place (RULE UFS). It lives here rather than in
//! `auth/middleware/` because that layer deliberately does not import
//! `fleet_runtime/webhook_verify` (its portability boundary) and because Slack
//! events are verified in-handler, not through the middleware chain (the
//! signing secret is resolved per-request from the admin vault, not a boot
//! secret) — mirroring `grant_approval_webhook`.
//!
//! Constant-time compare over the whole MAC (RULE CTM/CTC): no early return on
//! secret material.

const std = @import("std");
const hs = @import("hmac_sig");
const clock = @import("common").clock;
const webhook_verify = @import("../../../../fleet_runtime/webhook_verify.zig");

/// The Slack signature scheme (header names, `v0=` prefix, `v0` version, 300 s
/// drift), reused verbatim from the multi-provider registry.
pub const CONFIG = webhook_verify.SLACK;

/// Request header carrying `v0=<hex>` — read by the handler off the request.
pub const SIG_HEADER = CONFIG.sig_header;
/// Request header carrying the unix-seconds signing timestamp.
pub const TS_HEADER = CONFIG.ts_header.?;

/// Non-authorizing verdict (RULE TGU). The handler maps each variant to its
/// `UZ-SLK-0xx` code + HTTP status; nothing here touches the response.
pub const Verdict = enum { ok, bad_signature, stale_timestamp };

/// Verify a Slack v0 signature against an explicit `now` (unix seconds;
/// caller-injectable so a boundary test never races two live clock reads —
/// mirrors `hmac_sig.isTimestampFreshAt`). An empty `signing_secret` →
/// `bad_signature`: an empty HMAC key is deterministic and attacker-computable
/// (defense-in-depth, same guard as `webhook_sig.verifyHmac`).
pub fn verifyAt(
    signing_secret: []const u8,
    timestamp: []const u8,
    provided_sig: []const u8,
    body: []const u8,
    now_s: i64,
) Verdict {
    if (signing_secret.len == 0) return .bad_signature;
    // Freshness first: reject stale/replayed timestamps before spending a MAC.
    if (!hs.isTimestampFreshAt(timestamp, now_s, CONFIG.max_ts_drift_seconds)) return .stale_timestamp;
    if (!std.mem.startsWith(u8, provided_sig, CONFIG.prefix)) return .bad_signature;
    const expected = hs.hexDecode32(provided_sig[CONFIG.prefix.len..]) orelse return .bad_signature;
    const mac = hs.computeMac(signing_secret, &.{ CONFIG.hmac_version, ":", timestamp, ":", body });
    if (!hs.constantTimeEql(&mac, &expected)) return .bad_signature;
    return .ok;
}

/// Production entry point — verifies against the live wall clock.
pub fn verify(
    signing_secret: []const u8,
    timestamp: []const u8,
    provided_sig: []const u8,
    body: []const u8,
) Verdict {
    return verifyAt(signing_secret, timestamp, provided_sig, body, clock.nowSeconds());
}

// ── Tests (Dim 2.2: invalid signature / stale timestamp) ─────────────────────

const testing = std.testing;
// A readable, low-entropy fixture (not a real secret) — any string round-trips
// through the HMAC; a high-entropy hex value trips gitleaks' generic-api-key rule.
const SECRET = "slack-test-signing-secret-fixture";
const NOW: i64 = 1_700_000_000;

/// Sign a body exactly the way Slack does, so the happy-path test asserts a
/// real round-trip (not a tautology): `v0=` ++ hex(HMAC(secret, v0:ts:body)).
fn signTest(buf: []u8, timestamp: []const u8, body: []const u8) []const u8 {
    const mac = hs.computeMac(SECRET, &.{ CONFIG.hmac_version, ":", timestamp, ":", body });
    return hs.encodeMacHex(buf, CONFIG.prefix, mac);
}

test "verifyAt: a correctly signed fresh request is accepted" {
    const ts = "1700000000";
    const body = "{\"type\":\"event_callback\"}";
    var buf: [128]u8 = undefined;
    const sig = signTest(&buf, ts, body);
    try testing.expectEqual(Verdict.ok, verifyAt(SECRET, ts, sig, body, NOW));
}

test "verifyAt: a tampered body flips the verdict to bad_signature" {
    const ts = "1700000000";
    var buf: [128]u8 = undefined;
    const sig = signTest(&buf, ts, "{\"type\":\"event_callback\"}");
    // Same signature, different body → MAC mismatch.
    try testing.expectEqual(Verdict.bad_signature, verifyAt(SECRET, ts, sig, "{\"type\":\"tampered\"}", NOW));
}

test "verifyAt: a wrong signing secret is rejected constant-time" {
    const ts = "1700000000";
    const body = "{\"type\":\"event_callback\"}";
    var buf: [128]u8 = undefined;
    const sig = signTest(&buf, ts, body);
    try testing.expectEqual(Verdict.bad_signature, verifyAt("wrong-secret-entirely", ts, sig, body, NOW));
}

test "verifyAt: an empty signing secret never validates (attacker-computable MAC)" {
    try testing.expectEqual(Verdict.bad_signature, verifyAt("", "1700000000", "v0=deadbeef", "{}", NOW));
}

test "verifyAt: a timestamp older than the 300s window is stale, not bad-sig" {
    const stale_ts = "1699999600"; // NOW - 400s
    const body = "{\"type\":\"event_callback\"}";
    var buf: [128]u8 = undefined;
    // Correctly signed for the stale timestamp, so the ONLY reason to reject is drift.
    const sig = signTest(&buf, stale_ts, body);
    try testing.expectEqual(Verdict.stale_timestamp, verifyAt(SECRET, stale_ts, sig, body, NOW));
}

test "verifyAt: a future timestamp beyond the window is stale" {
    const future_ts = "1700000400"; // NOW + 400s
    const body = "{}";
    var buf: [128]u8 = undefined;
    const sig = signTest(&buf, future_ts, body);
    try testing.expectEqual(Verdict.stale_timestamp, verifyAt(SECRET, future_ts, sig, body, NOW));
}

test "verifyAt: a garbage/non-numeric timestamp is rejected (stale)" {
    try testing.expectEqual(Verdict.stale_timestamp, verifyAt(SECRET, "not-a-number", "v0=deadbeef", "{}", NOW));
}

test "verifyAt: a signature missing the v0= prefix is bad_signature" {
    const ts = "1700000000";
    const body = "{}";
    var buf: [128]u8 = undefined;
    const full = signTest(&buf, ts, body);
    // Strip the "v0=" prefix → hexDecode32 sees a hex too long / no prefix.
    try testing.expectEqual(Verdict.bad_signature, verifyAt(SECRET, ts, full[CONFIG.prefix.len..], body, NOW));
}
