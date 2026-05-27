//! Auth-audit primitives — `.auth_audit` log scope + session_id pseudonymization.
//!
//! Pure functions only; no HTTP, no Redis, no handler imports. Keeps
//! `src/auth/` portable into a standalone `zombie-auth` repo per the
//! existing chain/bearer convention.
//!
//! Session_id classification: a sensitive ephemeral capability equivalent
//! to a password-reset token. The `.auth` log scope MUST use
//! `redactSessionIdInto()` at info/warn/error level. The `.auth_audit`
//! scope carries the keyed HMAC + first-8 prefix (per `sessionIdHashHex`
//! + `sessionIdPrefix` below) — never the raw id.
//!
//! Two peppers consumed by this module are distinct from each other:
//!   - `AUTH_SESSION_CODE_PEPPER` — keys the verification-code HMAC
//!     (consumed by the session store, not by this module).
//!   - `AUDIT_LOG_PEPPER`        — keys the session_id pseudonymization
//!     HMAC emitted in `.auth_audit` events (this module).
//! Both are loaded once at boot in `src/config/runtime_loader.zig`.

const std = @import("std");
const hmac_sig = @import("hmac_sig");

/// Length of the human-correlation prefix surfaced into audit events
/// (8 hex chars = 32 bits = ~4B entries). Coarse enough to avoid
/// capability re-derivation; precise enough to grep correlated events
/// for one session without the pepper.
pub const SESSION_ID_PREFIX_LEN: usize = 8;

/// Lower-case hex encoding of a 32-byte HMAC — 64 chars.
pub const SESSION_ID_HASH_HEX_LEN: usize = hmac_sig.MAC_LEN * 2;

/// Writes the redacted form `<prefix8>…(len=<n>)` into `buf` and returns
/// the populated slice. Caller-buffer shape avoids allocation in the hot
/// log path. `buf` must hold ≥ `SESSION_ID_PREFIX_LEN + 12` bytes for the
/// longest possible id length tail; 32 bytes is a safe upper bound for
/// UUIDv7 (36 chars input).
///
/// When `id.len < SESSION_ID_PREFIX_LEN` the entire id is surfaced
/// (already too short to redact further) — still followed by the length
/// suffix so reviewers can spot truncation.
pub fn redactSessionIdInto(buf: []u8, id: []const u8) []const u8 {
    const prefix_len = @min(id.len, SESSION_ID_PREFIX_LEN);
    const prefix = id[0..prefix_len];
    return std.fmt.bufPrint(buf, "{s}…(len={d})", .{ prefix, id.len }) catch buf[0..0];
}

/// First-N hex characters of the raw id, used as the human-correlation
/// `session_id_prefix` field on every `.auth_audit` event. Returns a slice
/// into the input — no allocation, no copy. Caller must keep `id` alive
/// for the lifetime of the returned slice.
///
/// Coarse-by-design: 8 hex chars is too few bits to brute-force the verify
/// endpoint (≤5 attempts per session × 4B-entry prefix space is hopeless).
/// Full correlation lives on `sessionIdHashHex` below.
pub fn sessionIdPrefix(id: []const u8) []const u8 {
    return id[0..@min(id.len, SESSION_ID_PREFIX_LEN)];
}

/// HMAC-SHA256(audit_log_pepper, session_id) — 32 raw bytes. Same input
/// pair always yields the same output (correlation preserved across all
/// audit events for one session), but the keyed construction means an
/// attacker who reads the audit log without the pepper cannot recover
/// the raw session_id via offline pre-image attack.
///
/// The pepper is the `AUDIT_LOG_PEPPER` value loaded at boot in
/// `src/config/runtime_loader.zig`; held in zombied process memory only.
pub fn sessionIdHash(pepper: []const u8, session_id: []const u8) [hmac_sig.MAC_LEN]u8 {
    return hmac_sig.computeMac(pepper, &.{session_id});
}

/// Same as `sessionIdHash` but encoded as lower-case hex into a caller
/// buffer. `buf.len` MUST be ≥ `SESSION_ID_HASH_HEX_LEN`. Returns the
/// populated slice.
pub fn sessionIdHashHex(buf: []u8, pepper: []const u8, session_id: []const u8) []const u8 {
    const mac = sessionIdHash(pepper, session_id);
    const hex = std.fmt.bytesToHex(mac, .lower);
    @memcpy(buf[0..SESSION_ID_HASH_HEX_LEN], &hex);
    return buf[0..SESSION_ID_HASH_HEX_LEN];
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

test "redactSessionIdInto truncates UUIDv7 to first-8 + length suffix" {
    var buf: [32]u8 = undefined;
    const out = redactSessionIdInto(&buf, "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40");
    try testing.expectEqualStrings("0195b4ba…(len=36)", out);
}

test "redactSessionIdInto surfaces full id when shorter than prefix" {
    var buf: [32]u8 = undefined;
    const out = redactSessionIdInto(&buf, "abc");
    try testing.expectEqualStrings("abc…(len=3)", out);
}

test "sessionIdPrefix returns first 8 hex chars of a UUIDv7" {
    const id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40";
    try testing.expectEqualStrings("0195b4ba", sessionIdPrefix(id));
}

test "sessionIdPrefix returns the whole id when shorter than prefix" {
    try testing.expectEqualStrings("abc", sessionIdPrefix("abc"));
}

test "sessionIdHash is deterministic under identical pepper+id" {
    const pepper = "test-pepper-bytes-32-len--padded";
    const id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40";
    const a = sessionIdHash(pepper, id);
    const b = sessionIdHash(pepper, id);
    try testing.expectEqualSlices(u8, &a, &b);
}

test "sessionIdHash differs when the pepper changes" {
    const id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40";
    const a = sessionIdHash("pepper-one-with-enough-bytes-32x", id);
    const b = sessionIdHash("pepper-two-distinct-bytes-32xxxx", id);
    try testing.expect(!std.mem.eql(u8, &a, &b));
}

test "sessionIdHash differs across different session_ids under one pepper" {
    const pepper = "test-pepper-bytes-32-len--padded";
    const a = sessionIdHash(pepper, "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40");
    const b = sessionIdHash(pepper, "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa41");
    try testing.expect(!std.mem.eql(u8, &a, &b));
}

test "sessionIdHashHex is lower-case hex of the raw HMAC" {
    var buf: [SESSION_ID_HASH_HEX_LEN]u8 = undefined;
    const pepper = "test-pepper-bytes-32-len--padded";
    const id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40";
    const hex = sessionIdHashHex(&buf, pepper, id);
    try testing.expectEqual(@as(usize, SESSION_ID_HASH_HEX_LEN), hex.len);
    for (hex) |ch| try testing.expect(std.ascii.isHex(ch) and !std.ascii.isUpper(ch));
}
