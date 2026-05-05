//! Svix v1 webhook signature verifier — pure-function crypto core.
//!
//! Callers: `/v1/webhooks/svix/{zombie_id}` middleware (M28_001) and the
//! `/v1/webhooks/clerk` handler (M11_003). Both paths share the HMAC +
//! multi-sig + timestamp freshness logic; only the secret-lookup surface
//! differs (vault row vs env var).
//!
//! Secret format: `whsec_<base64>`. Decoder strips the prefix and accepts
//! both padded and unpadded standard base64.
//!
//! Basestring: `{svix-id}.{svix-timestamp}.{body}` (dots). Signature header
//! is space-delimited `v1,<base64_sig>` entries — any one matching accepts.
//! Comparison is constant-time (RULE CTM).

const std = @import("std");
const hs = @import("hmac_sig");

pub const SVIX_ID_HEADER: []const u8 = "svix-id";
pub const SVIX_TS_HEADER: []const u8 = "svix-timestamp";
pub const SVIX_SIG_HEADER: []const u8 = "svix-signature";
pub const SVIX_SECRET_PREFIX: []const u8 = "whsec_";
pub const SVIX_SIG_VERSION: []const u8 = "v1";
pub const SVIX_MAX_DRIFT_SECONDS: i64 = 300;

/// Max decoded secret length. Svix keys are 24 bytes; buffer is generous.
const MAX_SECRET_LEN: usize = 64;

pub const VerifyResult = enum {
    ok,
    invalid_signature,
    stale_timestamp,
};

/// Verify a Svix v1 signed request. Pure function; caller owns secret/body
/// lifetime and maps the result to its own error surface.
pub fn verifySvix(
    secret_raw: []const u8,
    svix_id: []const u8,
    svix_ts: []const u8,
    svix_sig: []const u8,
    body: []const u8,
    now_unix: i64,
    max_drift: i64,
) VerifyResult {
    if (!std.mem.startsWith(u8, secret_raw, SVIX_SECRET_PREFIX)) return .invalid_signature;

    var secret_buf: [MAX_SECRET_LEN]u8 = undefined;
    const secret = decodeBase64(&secret_buf, secret_raw[SVIX_SECRET_PREFIX.len..]) orelse
        return .invalid_signature;
    // Defense-in-depth: empty key makes HMAC attacker-computable.
    if (secret.len == 0) return .invalid_signature;

    if (svix_id.len == 0 or svix_ts.len == 0 or svix_sig.len == 0) return .invalid_signature;

    if (!isTimestampFresh(svix_ts, now_unix, max_drift)) return .stale_timestamp;

    const expected_mac = hs.computeMac(secret, &.{ svix_id, ".", svix_ts, ".", body });

    if (anySignatureMatches(svix_sig, &expected_mac)) return .ok;
    return .invalid_signature;
}

/// Iterate space-delimited `v1,<base64>` entries; return true on first
/// constant-time match against `expected_mac`. Unknown versions skipped.
fn anySignatureMatches(svix_sigs: []const u8, expected_mac: *const [hs.MAC_LEN]u8) bool {
    var it = std.mem.splitScalar(u8, svix_sigs, ' ');
    while (it.next()) |entry| {
        if (entry.len == 0) continue;
        const comma = std.mem.indexOfScalar(u8, entry, ',') orelse continue;
        if (!std.mem.eql(u8, entry[0..comma], SVIX_SIG_VERSION)) continue;
        var sig_buf: [hs.MAC_LEN]u8 = undefined;
        const decoded = decodeBase64(&sig_buf, entry[comma + 1 ..]) orelse continue;
        if (decoded.len != hs.MAC_LEN) continue;
        if (hs.constantTimeEql(expected_mac, &sig_buf)) return true;
    }
    return false;
}

/// Decode standard base64. Tries padded form first, then no-pad.
fn decodeBase64(dest: []u8, src: []const u8) ?[]u8 {
    if (tryDecode(&std.base64.standard.Decoder, dest, src)) |n| return dest[0..n];
    if (tryDecode(&std.base64.standard_no_pad.Decoder, dest, src)) |n| return dest[0..n];
    return null;
}

fn tryDecode(decoder: *const std.base64.Base64Decoder, dest: []u8, src: []const u8) ?usize {
    const size = decoder.calcSizeForSlice(src) catch return null;
    if (size > dest.len) return null;
    decoder.decode(dest[0..size], src) catch return null;
    return size;
}

/// Freshness check over unix-seconds timestamp string. Rejects pre-signed
/// requests; caller-injectable `now` for deterministic tests.
pub fn isTimestampFresh(timestamp: []const u8, now: i64, max_drift: i64) bool {
    const ts = std.fmt.parseInt(i64, timestamp, 10) catch return false;
    if (ts <= 0) return false;
    if (ts > now + max_drift) return false;
    if (now > ts and now - ts > max_drift) return false;
    return true;
}

test {
    _ = @import("svix_verify_test.zig");
}
