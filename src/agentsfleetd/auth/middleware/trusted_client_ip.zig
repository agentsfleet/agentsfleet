//! Trusted client-IP derivation — pure-function form.
//!
//! Behind a reverse proxy the raw TCP peer is the load balancer, not the
//! end client. Two header signals carry the real peer:
//!
//!   - `X-Forwarded-For` (XFF, industry-standard, chained left-to-right)
//!   - `Fly-Client-IP` (Fly.io's authoritative single-value header)
//!
//! Captain decision (May 18, 2026): in any non-dev deploy agentsfleetd lives
//! behind Fly's proxy; Fly stamps `Fly-Client-IP` on every proxied request
//! and strips client-supplied copies of its own header — so the Fly header
//! is the trust anchor. We still **default to XFF** so the audit chain
//! reads sensibly to any operator who's seen any HTTP infrastructure, and
//! we **always carry both** raw header values into audit events. If the
//! two disagree (client tried to spoof XFF; Fly's view contradicts), we
//! **flip to Fly-Client-IP** and surface `client_ip_divergent=true` so ops
//! can grep for forgery attempts.
//!
//! No `TRUSTED_PROXY_IPS` env / no IP allowlist — the Fly header IS the
//! trust anchor. Local-dev / direct-internet deploys have neither header
//! and fall back to the raw TCP peer.
//!
//! Pure function in this milestone — httpz `req.address` + header reads +
//! audit-event wiring land in the handler slice. Decoupling the logic from
//! the HTTP layer keeps testing deterministic and `src/auth/` portable.

const std = @import("std");

/// XFF chain separator + whitespace trim surface.
const S_T_R_N = " \t\r\n";
const S_COMMA = ",";

/// Which header (if any) the derived IP came from. Recorded into every
/// `.auth_audit` event that carries an `ip` field so forensic queries can
/// distinguish "real Fly proxy attribution" from "raw TCP peer (dev)".
pub const ClientIpSource = enum {
    /// XFF was present, agreed with Fly-Client-IP (or Fly header absent
    /// but XFF parsed successfully). Default trust source.
    xff,
    /// XFF and Fly-Client-IP both present and disagreed; we flipped to
    /// Fly's view. Records `divergent=true` alongside this source.
    fly_client_ip,
    /// Neither header was present (or both were empty). Fell back to the
    /// raw TCP peer. Expected only in local-dev or direct-internet deploys.
    tcp_peer,
};

/// Derived-client-IP result. Caller threads this onto the request context
/// so downstream consumers (rate-limit middleware, consume-idempotency
/// fingerprint, audit-event emitter) read the same value.
///
/// Returned slices point into the input slices — caller keeps `tcp_peer`,
/// `xff`, and `fly_client_ip` alive for the lifetime of `ip`. The raw
/// header values are also exposed (`xff_raw`, `fly_client_ip_raw`) so the
/// audit emitter can stamp both into events regardless of which one won.
pub const DerivedClientIp = struct {
    ip: []const u8,
    source: ClientIpSource,
    divergent: bool,
    xff_raw: ?[]const u8,
    fly_client_ip_raw: ?[]const u8,
};

/// Derive the effective client IP from the raw TCP peer + the two header
/// signals. See file header for the trust model.
///
///   - XFF leftmost entry is the candidate "real client" (industry standard).
///   - If `Fly-Client-IP` is also present, compare against the XFF candidate.
///     Agree → use XFF (default). Disagree → flip to Fly-Client-IP, mark
///     `divergent`.
///   - If only one header is present, use that one.
///   - If neither header carries a usable value, fall back to `tcp_peer`.
///
/// Empty / whitespace-only header values are treated as absent.
pub fn deriveClientIp(
    tcp_peer: []const u8,
    xff: ?[]const u8,
    fly_client_ip: ?[]const u8,
) DerivedClientIp {
    const xff_candidate = firstXffEntry(xff);
    const fly_candidate = trimOrNull(fly_client_ip);

    if (xff_candidate) |xff_ip| {
        if (fly_candidate) |fly_ip| {
            if (std.mem.eql(u8, xff_ip, fly_ip)) {
                return .{ .ip = xff_ip, .source = .xff, .divergent = false, .xff_raw = xff, .fly_client_ip_raw = fly_client_ip };
            }
            return .{ .ip = fly_ip, .source = .fly_client_ip, .divergent = true, .xff_raw = xff, .fly_client_ip_raw = fly_client_ip };
        }
        return .{ .ip = xff_ip, .source = .xff, .divergent = false, .xff_raw = xff, .fly_client_ip_raw = null };
    }

    if (fly_candidate) |fly_ip| {
        return .{ .ip = fly_ip, .source = .fly_client_ip, .divergent = false, .xff_raw = xff, .fly_client_ip_raw = fly_client_ip };
    }

    return .{ .ip = tcp_peer, .source = .tcp_peer, .divergent = false, .xff_raw = xff, .fly_client_ip_raw = fly_client_ip };
}

/// Returns the leftmost non-empty XFF entry (the originating client),
/// trimmed of surrounding whitespace. Null when the header is absent /
/// empty / only-commas-and-whitespace.
fn firstXffEntry(xff: ?[]const u8) ?[]const u8 {
    const header = xff orelse return null;
    var it = std.mem.tokenizeAny(u8, header, S_COMMA);
    while (it.next()) |entry_raw| {
        const entry = std.mem.trim(u8, entry_raw, S_T_R_N);
        if (entry.len > 0) return entry;
    }
    return null;
}

/// Trim a single-value header; null/empty/whitespace-only → null.
fn trimOrNull(value: ?[]const u8) ?[]const u8 {
    const raw = value orelse return null;
    const trimmed = std.mem.trim(u8, raw, S_T_R_N);
    return if (trimmed.len == 0) null else trimmed;
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

test "deriveClientIp falls back to TCP peer when neither header is present" {
    const got = deriveClientIp("203.0.113.7", null, null);
    try testing.expectEqualStrings("203.0.113.7", got.ip);
    try testing.expectEqual(ClientIpSource.tcp_peer, got.source);
    try testing.expect(!got.divergent);
}

test "deriveClientIp uses XFF when only XFF is present" {
    const got = deriveClientIp("10.0.0.1", "203.0.113.7", null);
    try testing.expectEqualStrings("203.0.113.7", got.ip);
    try testing.expectEqual(ClientIpSource.xff, got.source);
    try testing.expect(!got.divergent);
}

test "deriveClientIp uses Fly-Client-IP when only that header is present" {
    const got = deriveClientIp("10.0.0.1", null, "203.0.113.7");
    try testing.expectEqualStrings("203.0.113.7", got.ip);
    try testing.expectEqual(ClientIpSource.fly_client_ip, got.source);
    try testing.expect(!got.divergent);
}

test "deriveClientIp prefers XFF when both headers agree" {
    const got = deriveClientIp("10.0.0.1", "203.0.113.7", "203.0.113.7");
    try testing.expectEqualStrings("203.0.113.7", got.ip);
    try testing.expectEqual(ClientIpSource.xff, got.source);
    try testing.expect(!got.divergent);
}

test "deriveClientIp flips to Fly-Client-IP and marks divergent on disagreement" {
    // Classic spoof: client sets X-Forwarded-For: <victim>; Fly's proxy
    // saw the true peer and stamped Fly-Client-IP correctly.
    const got = deriveClientIp("10.0.0.1", "1.2.3.4", "203.0.113.7");
    try testing.expectEqualStrings("203.0.113.7", got.ip);
    try testing.expectEqual(ClientIpSource.fly_client_ip, got.source);
    try testing.expect(got.divergent);
}

test "deriveClientIp picks leftmost non-empty XFF entry from a chain" {
    const got = deriveClientIp("10.0.0.1", "203.0.113.7, 10.0.0.2, 10.0.0.1", null);
    try testing.expectEqualStrings("203.0.113.7", got.ip);
    try testing.expectEqual(ClientIpSource.xff, got.source);
}

test "deriveClientIp tolerates whitespace + empty XFF segments" {
    const got = deriveClientIp("10.0.0.1", "  , 203.0.113.7 , 10.0.0.1 ", null);
    try testing.expectEqualStrings("203.0.113.7", got.ip);
    try testing.expectEqual(ClientIpSource.xff, got.source);
}

test "deriveClientIp treats empty headers as absent and falls back to TCP peer" {
    const got = deriveClientIp("10.0.0.1", "", "");
    try testing.expectEqualStrings("10.0.0.1", got.ip);
    try testing.expectEqual(ClientIpSource.tcp_peer, got.source);
}

test "deriveClientIp exposes both raw header values for audit emission" {
    const got = deriveClientIp("10.0.0.1", "1.2.3.4", "203.0.113.7");
    try testing.expectEqualStrings("1.2.3.4", got.xff_raw.?);
    try testing.expectEqualStrings("203.0.113.7", got.fly_client_ip_raw.?);
}
