//! Pure-function tests for the helper functions in `session_store_redis.zig`.
//! Extracted so the production file stays under the 350-line FLL cap.

const std = @import("std");
const testing = std.testing;
const hmac_sig = @import("hmac_sig");
const store = @import("session_store_redis.zig");

test "formatSessionKey rejects non-UUIDv7" {
    var buf: [store.SESSION_KEY_BUF_LEN]u8 = undefined;
    try testing.expectError(error.InvalidSessionId, store.formatSessionKey(&buf, "abc"));
    try testing.expectError(error.InvalidSessionId, store.formatSessionKey(&buf, "'; DROP TABLE sessions; --"));
}

test "formatSessionKey accepts a UUIDv7" {
    var buf: [store.SESSION_KEY_BUF_LEN]u8 = undefined;
    const key = try store.formatSessionKey(&buf, "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40");
    try testing.expectEqualStrings("auth:session:0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40", key);
}

test "isDigits accepts 6-digit code; rejects mixed input" {
    try testing.expect(store.isDigits("123456"));
    try testing.expect(store.isDigits("000000"));
    try testing.expect(!store.isDigits("12345a"));
    try testing.expect(!store.isDigits("12345 "));
}

test "computeCodeHmacHex is deterministic per (pepper, sid, code)" {
    var a: [hmac_sig.MAC_LEN * 2]u8 = undefined;
    var b: [hmac_sig.MAC_LEN * 2]u8 = undefined;
    const pepper = "test-pepper-bytes-32-len--padded";
    const sid = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40";
    _ = store.computeCodeHmacHex(&a, pepper, sid, "123456");
    _ = store.computeCodeHmacHex(&b, pepper, sid, "123456");
    try testing.expectEqualSlices(u8, &a, &b);
}

test "computeCodeHmacHex differs when the code changes" {
    var a: [hmac_sig.MAC_LEN * 2]u8 = undefined;
    var b: [hmac_sig.MAC_LEN * 2]u8 = undefined;
    const pepper = "test-pepper-bytes-32-len--padded";
    const sid = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40";
    _ = store.computeCodeHmacHex(&a, pepper, sid, "123456");
    _ = store.computeCodeHmacHex(&b, pepper, sid, "123457");
    try testing.expect(!std.mem.eql(u8, &a, &b));
}
