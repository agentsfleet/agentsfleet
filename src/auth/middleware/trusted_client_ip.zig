//! Trusted client-IP derivation — pure-function form.
//!
//! Behind a reverse proxy the raw TCP peer is the load balancer, not the
//! end client. `X-Forwarded-For` carries the chain, but it MUST be trusted
//! only when the request arrived through a configured trusted proxy —
//! a client-supplied XFF header is forgeable and would let a malicious
//! caller forge a fingerprint or rate-limit-bucket key.
//!
//! Two failure modes if mis-derived:
//!   1. Use the LB IP directly       → every CLI shares one fingerprint;
//!                                     per-IP rate limit collapses into
//!                                     per-LB rate limit.
//!   2. Trust XFF from any source    → malicious client forges victim's IP;
//!                                     per-IP attribution + replay window
//!                                     fingerprint both compromised.
//!
//! Pure function in this milestone — httpz `req.address` + the actual
//! `TRUSTED_PROXY_IPS` env load + middleware-chain wiring land in the
//! handler slice. Decoupling the logic from the HTTP layer keeps testing
//! deterministic and `src/auth/` portable.

const std = @import("std");

/// Whitespace + comma separator surface for XFF parsing.
const S_T_R_N = " \t\r\n";
const S_COMMA = ",";

/// Derive the effective client IP from the raw TCP peer + XFF header
/// content + the operator-configured trusted-proxy allowlist.
///
///   - If `tcp_peer` is in `trusted_proxies`, parse `xff` left-to-right
///     and return the first entry that is NOT itself in the allowlist
///     (the original client). Fall back to `tcp_peer` if every XFF entry
///     is a trusted proxy OR if `xff` is null/empty.
///   - If `tcp_peer` is NOT in `trusted_proxies`, `xff` is ignored entirely
///     (client-supplied forwarding headers from a non-proxy source cannot
///     be trusted). Returns `tcp_peer`.
///
/// Returned slice points into one of the input slices — caller must keep
/// both `tcp_peer` and `xff` alive for the lifetime of the result.
///
/// Empty `trusted_proxies` (no proxy in front of zombied) makes XFF
/// untrusted by definition — covers direct-internet deploys.
pub fn deriveClientIp(
    tcp_peer: []const u8,
    xff: ?[]const u8,
    trusted_proxies: []const []const u8,
) []const u8 {
    if (!isTrustedProxy(tcp_peer, trusted_proxies)) return tcp_peer;
    const header = xff orelse return tcp_peer;
    var it = std.mem.tokenizeAny(u8, header, S_COMMA);
    while (it.next()) |entry_raw| {
        const entry = std.mem.trim(u8, entry_raw, S_T_R_N);
        if (entry.len == 0) continue;
        if (!isTrustedProxy(entry, trusted_proxies)) return entry;
    }
    return tcp_peer;
}

/// Linear scan of the allowlist. CIDR-range matching is a later
/// enhancement — for now operators list each proxy IP literally
/// (matches Cloudflare's documented IP-range pattern at deploy time).
pub fn isTrustedProxy(ip: []const u8, trusted_proxies: []const []const u8) bool {
    for (trusted_proxies) |proxy| {
        if (std.mem.eql(u8, ip, proxy)) return true;
    }
    return false;
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

test "deriveClientIp returns tcp peer when no proxies are configured" {
    const trusted: []const []const u8 = &.{};
    try testing.expectEqualStrings(
        "203.0.113.7",
        deriveClientIp("203.0.113.7", "198.51.100.1", trusted),
    );
}

test "deriveClientIp ignores XFF when tcp peer is not a trusted proxy" {
    const trusted: []const []const u8 = &.{"10.0.0.1"};
    try testing.expectEqualStrings(
        "203.0.113.7",
        deriveClientIp("203.0.113.7", "1.2.3.4", trusted),
    );
}

test "deriveClientIp returns first non-proxy XFF entry when peer is trusted" {
    const trusted: []const []const u8 = &.{ "10.0.0.1", "10.0.0.2" };
    try testing.expectEqualStrings(
        "203.0.113.7",
        deriveClientIp("10.0.0.1", "203.0.113.7, 10.0.0.2", trusted),
    );
}

test "deriveClientIp falls back to tcp peer when XFF is absent" {
    const trusted: []const []const u8 = &.{"10.0.0.1"};
    try testing.expectEqualStrings(
        "10.0.0.1",
        deriveClientIp("10.0.0.1", null, trusted),
    );
}

test "deriveClientIp falls back to tcp peer when every XFF entry is a trusted proxy" {
    const trusted: []const []const u8 = &.{ "10.0.0.1", "10.0.0.2" };
    try testing.expectEqualStrings(
        "10.0.0.1",
        deriveClientIp("10.0.0.1", "10.0.0.2, 10.0.0.1", trusted),
    );
}

test "deriveClientIp tolerates extra whitespace + empty XFF segments" {
    const trusted: []const []const u8 = &.{"10.0.0.1"};
    try testing.expectEqualStrings(
        "203.0.113.7",
        deriveClientIp("10.0.0.1", "  , 203.0.113.7 , 10.0.0.1 ", trusted),
    );
}

test "deriveClientIp handles empty XFF header by returning tcp peer" {
    const trusted: []const []const u8 = &.{"10.0.0.1"};
    try testing.expectEqualStrings(
        "10.0.0.1",
        deriveClientIp("10.0.0.1", "", trusted),
    );
}

test "isTrustedProxy returns true only on exact-string match" {
    const trusted: []const []const u8 = &.{ "10.0.0.1", "10.0.0.2" };
    try testing.expect(isTrustedProxy("10.0.0.1", trusted));
    try testing.expect(isTrustedProxy("10.0.0.2", trusted));
    try testing.expect(!isTrustedProxy("10.0.0.3", trusted));
    try testing.expect(!isTrustedProxy("10.0.0.10", trusted));
    try testing.expect(!isTrustedProxy("", trusted));
}
