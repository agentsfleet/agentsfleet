//! Validation guard for an operator-supplied OpenAI-compatible base URL.
//!
//! The URL is hostile input (RULE PRI): a tenant could point it at a loopback
//! admin port, the cloud metadata service, or an internal RFC1918 host to make
//! the runner egress against infrastructure it must never reach (Server-Side
//! Request Forgery, SSRF). This guard rejects those at the resolver parse
//! boundary, BEFORE any lease is issued, so a blocked endpoint never reaches the
//! engine or the egress allowlist (Invariant 5).
//!
//! Scope is HOST-LITERAL validation only — exactly what the runner's vendored
//! `nullclaw/net_security.zig` enforces for IP-literal hosts (the range checks
//! live in `ip_literal.zig`). DNS-rebinding (a hostname that resolves to a
//! private address) is out of scope here and is caught downstream at connect
//! time by the runner's resolve-then-check path; mirroring those predicates
//! keeps the control-plane verdict and the data-plane enforcement in agreement.
//! Only the host may appear in a rejection log — the api_key sitting beside the
//! URL in the credential JSON is never logged (VLT).

const std = @import("std");
const ip_literal = @import("ip_literal.zig");

/// Required scheme for any custom endpoint — plaintext http is rejected so the
/// api_key never crosses the wire unencrypted (RULE UFS — one site, asserted by
/// tests).
pub const REQUIRED_SCHEME: []const u8 = "https";

/// The outcome of validating a base URL. Tagged union (RULE TGU): callers branch
/// on the reason, not a bare bool — the resolver maps each variant to the typed
/// `UZ-PROVIDER-*` error + a host-only rejection log.
pub const Verdict = union(enum) {
    /// Scheme is https and the host is not an SSRF-unsafe IP literal. Carries the
    /// bare host (port/path stripped) so the caller derives the egress-allowlist
    /// entry without re-parsing — it borrows from the input `url`.
    ok: []const u8,
    /// Scheme is not https (http / ws / file / missing scheme).
    invalid_scheme,
    /// Host is an SSRF-unsafe IP literal: loopback, RFC1918 private, link-local
    /// (incl. cloud metadata), unspecified, multicast/broadcast, or an
    /// IPv4-mapped form of any of those.
    blocked_host,
    /// URL has no parseable authority / empty host, or is otherwise malformed.
    malformed,
};

/// Validate a user-supplied base URL for an OpenAI-compatible endpoint.
/// Order matters: scheme first (cheapest, and an http URL is rejected regardless
/// of host), then host extraction, then the SSRF IP-literal check.
pub fn validate(url: []const u8) Verdict {
    if (!hasHttpsScheme(url)) return .invalid_scheme;

    const host = extractHost(url) orelse return .malformed;
    if (host.len == 0) return .malformed;

    if (ip_literal.isBlockedHostLiteral(host)) return .blocked_host;
    return .{ .ok = host };
}

/// True only when the URL's scheme is exactly `https` (case-insensitive). A
/// missing `://` (schemeless host) is NOT https and is rejected — we require an
/// explicit secure scheme rather than inferring one.
fn hasHttpsScheme(url: []const u8) bool {
    const sep = std.mem.indexOf(u8, url, "://") orelse return false;
    return std.ascii.eqlIgnoreCase(url[0..sep], REQUIRED_SCHEME);
}

/// Extract the bare host from an `https://` URL: strip scheme, userinfo, port,
/// path, query, and fragment. Returns the bracketed form for an IPv6 literal
/// (e.g. `[::1]`) so the caller and `isBlockedHostLiteral` see the same bytes
/// `hostFromUrl` (execution_policy.zig) produces — the egress host stays in sync.
/// Returns null when there is no authority (e.g. `https:///path`).
fn extractHost(url: []const u8) ?[]const u8 {
    const sep = std.mem.indexOf(u8, url, "://") orelse return null;
    const after_scheme = url[sep + 3 ..];
    const authority_end = std.mem.indexOfAny(u8, after_scheme, "/?#") orelse after_scheme.len;
    const authority = after_scheme[0..authority_end];
    if (authority.len == 0) return null;

    // Strip optional `userinfo@` — a hostname carries none, and a smuggled
    // `user@evil` must not let `evil` masquerade as userinfo.
    const after_userinfo = if (std.mem.lastIndexOfScalar(u8, authority, '@')) |i| authority[i + 1 ..] else authority;
    if (after_userinfo.len == 0) return null;

    // IPv6 literal: `[::1]:443` — the inner colons are address bytes, not a port.
    // Return the bracketed host as-is (matches hostFromUrl); a missing close
    // bracket is malformed.
    if (after_userinfo[0] == '[') {
        const close = std.mem.indexOfScalar(u8, after_userinfo, ']') orelse return null;
        return after_userinfo[0 .. close + 1];
    }

    // Otherwise strip `:port` (a hostname / IPv4 literal has no other colon).
    const host_end = std.mem.indexOfScalar(u8, after_userinfo, ':') orelse after_userinfo.len;
    if (host_end == 0) return null;
    return after_userinfo[0..host_end];
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing = std.testing;

test "validate accepts a public https endpoint and returns the bare host" {
    const v = validate("https://api.openrouter.ai/v1");
    try testing.expect(v == .ok);
    try testing.expectEqualStrings("api.openrouter.ai", v.ok);
    // Port + path + userinfo are stripped down to the bare host.
    try testing.expectEqualStrings("gw.example.com", validate("https://user:pw@gw.example.com:8443/v1").ok);
    // A smuggled `user@evil` must not let an internal target masquerade as host.
    try testing.expect(validate("https://evil.com@169.254.169.254/v1") == .blocked_host);
}

test "validate rejects non-https schemes as invalid_scheme" {
    try testing.expect(validate("http://api.example.com/v1") == .invalid_scheme);
    try testing.expect(validate("ws://api.example.com") == .invalid_scheme);
    try testing.expect(validate("file:///etc/passwd") == .invalid_scheme);
    try testing.expect(validate("api.example.com/v1") == .invalid_scheme); // schemeless
    try testing.expect(validate("HTTP://api.example.com") == .invalid_scheme); // case-folded, still not https
    try testing.expect(validate("HTTPS://api.example.com").ok.len > 0); // https is case-insensitive
}

test "validate flags an empty or unparseable authority as malformed" {
    try testing.expect(validate("https:///just/a/path") == .malformed);
    try testing.expect(validate("https://") == .malformed);
    try testing.expect(validate("https://[::1") == .malformed); // unterminated bracket
    try testing.expect(validate("https://user@") == .malformed); // userinfo, no host
}

test "validate blocks the full SSRF v4 blocklist before any run" {
    try testing.expect(validate("https://127.0.0.1/v1") == .blocked_host);
    try testing.expect(validate("https://10.1.2.3/v1") == .blocked_host);
    try testing.expect(validate("https://172.16.5.9/v1") == .blocked_host);
    try testing.expect(validate("https://172.31.255.255/v1") == .blocked_host);
    try testing.expect(validate("https://192.168.1.1/v1") == .blocked_host);
    try testing.expect(validate("https://169.254.169.254/latest/meta-data") == .blocked_host); // cloud metadata
    try testing.expect(validate("https://0.0.0.0/v1") == .blocked_host);
}

test "validate blocks the SSRF v6 blocklist (bracketed literals)" {
    try testing.expect(validate("https://[::1]/v1") == .blocked_host); // loopback
    try testing.expect(validate("https://[::1]:8443/v1") == .blocked_host); // loopback + port
    try testing.expect(validate("https://[::]/v1") == .blocked_host); // unspecified
    try testing.expect(validate("https://[fc00::1]/v1") == .blocked_host); // unique-local
    try testing.expect(validate("https://[fe80::1]/v1") == .blocked_host); // link-local
    try testing.expect(validate("https://[ff02::1]/v1") == .blocked_host); // multicast
    try testing.expect(validate("https://[::ffff:127.0.0.1]/v1") == .blocked_host); // IPv4-mapped loopback
    try testing.expect(validate("https://[::ffff:169.254.169.254]/v1") == .blocked_host); // IPv4-mapped metadata
}

test "validate allows the 172.16/12 boundary hosts that are NOT private" {
    // 172.15.x and 172.32.x are public — the /12 must not over-block.
    try testing.expectEqualStrings("172.15.0.1", validate("https://172.15.0.1/v1").ok);
    try testing.expectEqualStrings("172.32.0.1", validate("https://172.32.0.1/v1").ok);
    // 169.253.x and 169.255.x are public — only 169.254/16 is link-local.
    try testing.expectEqualStrings("169.253.0.1", validate("https://169.253.0.1/v1").ok);
}

test "validate passes a public IP literal and a hostname unchanged" {
    try testing.expectEqualStrings("8.8.8.8", validate("https://8.8.8.8/v1").ok);
    try testing.expectEqualStrings("[2606:4700:4700::1111]", validate("https://[2606:4700:4700::1111]/v1").ok);
    try testing.expectEqualStrings("self-hosted.vllm.internal-corp.net", validate("https://self-hosted.vllm.internal-corp.net/v1").ok);
}
