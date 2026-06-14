//! Unit tests for webhook_hmac middleware. Split from webhook_hmac.zig to
//! stay under the §6.2 250-line cap on middleware source files.

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const webhook_hmac = @import("webhook_hmac.zig");

const AuthCtx = auth_ctx.AuthCtx;
const WebhookHmac = webhook_hmac.WebhookHmac;
const SIGNATURE_VERSION = webhook_hmac.SIGNATURE_VERSION;
const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;

const test_fixtures = struct {
    var last_code: []const u8 = "";
    var last_detail: []const u8 = "";
    var write_count: usize = 0;

    fn reset() void {
        last_code = "";
        last_detail = "";
        write_count = 0;
    }

    fn writeError(_: *httpz.Response, code: []const u8, detail: []const u8, _: []const u8) void {
        last_code = code;
        last_detail = detail;
        write_count += 1;
    }
};

fn fixedNow() i64 {
    return 1_700_000_000_000; // 2023-11-14T22:13:20Z — arbitrary, stable across runs
}

fn signBodyAt(secret: []const u8, timestamp: []const u8, body: []const u8, out_buf: []u8) []const u8 {
    var mac: [HmacSha256.mac_length]u8 = undefined;
    var h = HmacSha256.init(secret);
    h.update(SIGNATURE_VERSION);
    h.update(":");
    h.update(timestamp);
    h.update(":");
    h.update(body);
    h.final(&mac);
    @memcpy(out_buf[0..SIGNATURE_VERSION.len], SIGNATURE_VERSION);
    out_buf[SIGNATURE_VERSION.len] = '=';
    const hex_chars = "0123456789abcdef";
    const hex_start = SIGNATURE_VERSION.len + 1;
    for (mac, 0..) |byte, i| {
        out_buf[hex_start + i * 2] = hex_chars[byte >> 4];
        out_buf[hex_start + i * 2 + 1] = hex_chars[byte & 0x0f];
    }
    return out_buf[0 .. hex_start + HmacSha256.mac_length * 2];
}

fn runOne(mw: *WebhookHmac, ht: anytype) !chain.Outcome {
    var ctx = AuthCtx{
        .alloc = testing.allocator,
        .res = ht.res,
        .req_id = "req_test",
        .write_error = test_fixtures.writeError,
    };
    return mw.execute(&ctx, ht.req);
}

test "webhook_hmac .next on matching signature within freshness window" {
    test_fixtures.reset();
    const secret = "correct horse battery staple";
    const body = "{\"event\":\"approval.created\"}";
    const ts_str = "1699999990"; // 10s before fixedNow
    var sig_buf: [3 + HmacSha256.mac_length * 2]u8 = undefined;
    const sig = signBodyAt(secret, ts_str, body, &sig_buf);

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-signature-timestamp", ts_str);
    ht.header("x-signature", sig);
    ht.body(body);

    var mw = WebhookHmac{ .secret = secret, .now_ms = fixedNow };
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), test_fixtures.write_count);
}

test "webhook_hmac short-circuits when x-signature-timestamp header is missing" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-signature", "v0=deadbeef");
    ht.body("{}");

    var mw = WebhookHmac{ .secret = "secret", .now_ms = fixedNow };
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_APPROVAL_INVALID_SIGNATURE, test_fixtures.last_code);
    try testing.expectEqualStrings("Missing signature timestamp", test_fixtures.last_detail);
}

test "webhook_hmac short-circuits when signature does not match body (wrong secret)" {
    test_fixtures.reset();
    const secret = "secret-a";
    const body = "{\"event\":\"x\"}";
    const ts_str = "1699999990";
    var sig_buf: [3 + HmacSha256.mac_length * 2]u8 = undefined;
    // Sign with a different secret — verifier will reject the mismatched MAC.
    const bad_sig = signBodyAt("secret-b", ts_str, body, &sig_buf);

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-signature-timestamp", ts_str);
    ht.header("x-signature", bad_sig);
    ht.body(body);

    var mw = WebhookHmac{ .secret = secret, .now_ms = fixedNow };
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings("Invalid signature", test_fixtures.last_detail);
}

test "webhook_hmac short-circuits on stale timestamp (> 300s old)" {
    test_fixtures.reset();
    const secret = "s";
    const body = "{}";
    const ts_str = "1699999600"; // 400s before fixedNow — outside freshness window
    var sig_buf: [3 + HmacSha256.mac_length * 2]u8 = undefined;
    const sig = signBodyAt(secret, ts_str, body, &sig_buf);

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-signature-timestamp", ts_str);
    ht.header("x-signature", sig);
    ht.body(body);

    var mw = WebhookHmac{ .secret = secret, .now_ms = fixedNow };
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings("Timestamp too old", test_fixtures.last_detail);
}

test "webhook_hmac short-circuits when timestamp is not parseable as integer" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-signature-timestamp", "not-a-number");
    ht.header("x-signature", "v0=deadbeef");
    ht.body("{}");

    var mw = WebhookHmac{ .secret = "s", .now_ms = fixedNow };
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings("Invalid timestamp", test_fixtures.last_detail);
}
