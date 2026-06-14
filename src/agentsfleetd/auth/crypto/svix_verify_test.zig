//! Pure-function tests for the Svix v1 verifier primitives.

const std = @import("std");
const testing = std.testing;
const hs = @import("hmac_sig");
const svix = @import("svix_verify.zig");

const RAW_KEY = "0123456789abcdef01234567"; // 24 bytes
const WHSEC_KEY = "whsec_MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3"; // base64("0123456789abcdef01234567")
const SVIX_ID = "msg_2abcXYZ";
const FIXED_NOW: i64 = 1_700_000_000;
const FRESH_TS = "1699999950"; // 50s before FIXED_NOW

fn signEntry(alloc: std.mem.Allocator, id: []const u8, ts: []const u8, body: []const u8) ![]u8 {
    const mac = hs.computeMac(RAW_KEY, &.{ id, ".", ts, ".", body });
    const Encoder = std.base64.standard.Encoder;
    const enc_len = Encoder.calcSize(mac.len);
    const out = try alloc.alloc(u8, 3 + enc_len);
    @memcpy(out[0..3], "v1,");
    _ = Encoder.encode(out[3..], &mac);
    return out;
}

test "verifySvix: single valid signature → ok" {
    const body = "{\"event\":\"user.created\"}";
    const entry = try signEntry(testing.allocator, SVIX_ID, FRESH_TS, body);
    defer testing.allocator.free(entry);

    const result = svix.verifySvix(WHSEC_KEY, SVIX_ID, FRESH_TS, entry, body, FIXED_NOW, svix.SVIX_MAX_DRIFT_SECONDS);
    try testing.expectEqual(svix.VerifyResult.ok, result);
}

test "verifySvix: tampered body → invalid_signature" {
    const entry = try signEntry(testing.allocator, SVIX_ID, FRESH_TS, "original");
    defer testing.allocator.free(entry);

    const result = svix.verifySvix(WHSEC_KEY, SVIX_ID, FRESH_TS, entry, "tampered", FIXED_NOW, svix.SVIX_MAX_DRIFT_SECONDS);
    try testing.expectEqual(svix.VerifyResult.invalid_signature, result);
}

test "verifySvix: rotation — first invalid, second valid → ok" {
    const body = "{\"event\":\"session.created\"}";
    const good = try signEntry(testing.allocator, SVIX_ID, FRESH_TS, body);
    defer testing.allocator.free(good);
    const combined = try std.fmt.allocPrint(
        testing.allocator,
        "v1,AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA= {s}",
        .{good},
    );
    defer testing.allocator.free(combined);

    const result = svix.verifySvix(WHSEC_KEY, SVIX_ID, FRESH_TS, combined, body, FIXED_NOW, svix.SVIX_MAX_DRIFT_SECONDS);
    try testing.expectEqual(svix.VerifyResult.ok, result);
}

test "verifySvix: stale timestamp → stale_timestamp" {
    const stale_ts = "1699999000"; // 1000s before FIXED_NOW
    const body = "{}";
    const entry = try signEntry(testing.allocator, SVIX_ID, stale_ts, body);
    defer testing.allocator.free(entry);

    const result = svix.verifySvix(WHSEC_KEY, SVIX_ID, stale_ts, entry, body, FIXED_NOW, svix.SVIX_MAX_DRIFT_SECONDS);
    try testing.expectEqual(svix.VerifyResult.stale_timestamp, result);
}

test "verifySvix: future timestamp (pre-sign attack) → stale_timestamp" {
    const future_ts = "1700000301"; // 301s after FIXED_NOW — outside +drift
    const body = "{}";
    const entry = try signEntry(testing.allocator, SVIX_ID, future_ts, body);
    defer testing.allocator.free(entry);

    const result = svix.verifySvix(WHSEC_KEY, SVIX_ID, future_ts, entry, body, FIXED_NOW, svix.SVIX_MAX_DRIFT_SECONDS);
    try testing.expectEqual(svix.VerifyResult.stale_timestamp, result);
}

test "verifySvix: missing whsec_ prefix → invalid_signature" {
    const body = "{}";
    const entry = try signEntry(testing.allocator, SVIX_ID, FRESH_TS, body);
    defer testing.allocator.free(entry);

    const bare = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3";
    const result = svix.verifySvix(bare, SVIX_ID, FRESH_TS, entry, body, FIXED_NOW, svix.SVIX_MAX_DRIFT_SECONDS);
    try testing.expectEqual(svix.VerifyResult.invalid_signature, result);
}

test "verifySvix: empty decoded secret → invalid_signature" {
    const body = "{}";
    const entry = try signEntry(testing.allocator, SVIX_ID, FRESH_TS, body);
    defer testing.allocator.free(entry);

    const result = svix.verifySvix("whsec_", SVIX_ID, FRESH_TS, entry, body, FIXED_NOW, svix.SVIX_MAX_DRIFT_SECONDS);
    try testing.expectEqual(svix.VerifyResult.invalid_signature, result);
}

test "verifySvix: empty svix-id → invalid_signature" {
    const body = "{}";
    const entry = try signEntry(testing.allocator, SVIX_ID, FRESH_TS, body);
    defer testing.allocator.free(entry);

    const result = svix.verifySvix(WHSEC_KEY, "", FRESH_TS, entry, body, FIXED_NOW, svix.SVIX_MAX_DRIFT_SECONDS);
    try testing.expectEqual(svix.VerifyResult.invalid_signature, result);
}

test "verifySvix: empty signature header → invalid_signature" {
    const body = "{}";
    const result = svix.verifySvix(WHSEC_KEY, SVIX_ID, FRESH_TS, "", body, FIXED_NOW, svix.SVIX_MAX_DRIFT_SECONDS);
    try testing.expectEqual(svix.VerifyResult.invalid_signature, result);
}

test "verifySvix: garbage signature (no comma) → invalid_signature" {
    const body = "{}";
    const result = svix.verifySvix(WHSEC_KEY, SVIX_ID, FRESH_TS, "garbage", body, FIXED_NOW, svix.SVIX_MAX_DRIFT_SECONDS);
    try testing.expectEqual(svix.VerifyResult.invalid_signature, result);
}

test "isTimestampFresh: boundary inclusive at ±max_drift" {
    try testing.expect(svix.isTimestampFresh("1699999700", 1700000000, 300));
    try testing.expect(!svix.isTimestampFresh("1699999699", 1700000000, 300));
    try testing.expect(svix.isTimestampFresh("1700000300", 1700000000, 300));
    try testing.expect(!svix.isTimestampFresh("1700000301", 1700000000, 300));
    try testing.expect(!svix.isTimestampFresh("not-a-number", 1700000000, 300));
    try testing.expect(!svix.isTimestampFresh("0", 1700000000, 300));
    try testing.expect(!svix.isTimestampFresh("-1", 1700000000, 300));
}
