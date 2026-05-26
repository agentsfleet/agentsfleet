// Pure HMAC-SHA256 + constant-time primitives.
// Zero external deps beyond std. Safe to import from any layer; M28_001
// designates this as the single canonical source for webhook auth
// primitives replacing prior per-middleware copies.

const std = @import("std");

pub const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;
pub const MAC_LEN: usize = HmacSha256.mac_length;

pub fn computeMac(secret: []const u8, parts: []const []const u8) [MAC_LEN]u8 {
    var mac: [MAC_LEN]u8 = undefined;
    var h = HmacSha256.init(secret);
    for (parts) |p| h.update(p);
    h.final(&mac);
    return mac;
}

/// Length-mismatch folded in after XOR (RULE CTC). Safe for attacker-controlled
/// `b`; no short-circuit on secret material.
pub fn constantTimeEql(a: []const u8, b: []const u8) bool {
    const min_len = @min(a.len, b.len);
    var diff: u8 = 0;
    for (a[0..min_len], b[0..min_len]) |x, y| diff |= x ^ y;
    diff |= @as(u8, @intFromBool(a.len != b.len));
    return diff == 0;
}

pub fn hexDecode32(hex: []const u8) ?[MAC_LEN]u8 {
    if (hex.len != MAC_LEN * 2) return null;
    var out: [MAC_LEN]u8 = undefined;
    _ = std.fmt.hexToBytes(&out, hex) catch return null;
    return out;
}

/// Writes `prefix ++ lowercase_hex(mac)` into `buf`. `buf.len` must be
/// ≥ `prefix.len + MAC_LEN*2`.
pub fn encodeMacHex(buf: []u8, prefix: []const u8, mac: [MAC_LEN]u8) []const u8 {
    @memcpy(buf[0..prefix.len], prefix);
    const hex = std.fmt.bytesToHex(mac, .lower);
    @memcpy(buf[prefix.len..][0..hex.len], &hex);
    return buf[0 .. prefix.len + hex.len];
}

/// Freshness check over unix-seconds timestamp strings. Accepts values in
/// `[now - max_drift, now + max_drift]`; rejects pre-signed requests.
pub fn isTimestampFresh(timestamp: []const u8, max_drift: i64) bool {
    const ts = std.fmt.parseInt(i64, timestamp, 10) catch return false;
    if (ts <= 0) return false;
    const now = std.time.timestamp();
    if (ts > now + max_drift) return false;
    if (now > ts and now - ts > max_drift) return false;
    return true;
}
