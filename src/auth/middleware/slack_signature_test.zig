//! Unit tests for slack_signature. Split from slack_signature.zig to match
//! the sibling-test convention used by webhook_hmac / jwks.

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const slack = @import("slack_signature.zig");

const AuthCtx = auth_ctx.AuthCtx;
const SlackSignature = slack.SlackSignature;
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
    return 1_700_000_000; // arbitrary stable reference
}

fn signSlack(secret: []const u8, timestamp: []const u8, body: []const u8, out_buf: []u8) []const u8 {
    var mac: [HmacSha256.mac_length]u8 = undefined;
    var h = HmacSha256.init(secret);
    h.update("v0:");
    h.update(timestamp);
    h.update(":");
    h.update(body);
    h.final(&mac);
    @memcpy(out_buf[0..3], "v0=");
    const hex_chars = "0123456789abcdef";
    for (mac, 0..) |byte, i| {
        out_buf[3 + i * 2] = hex_chars[byte >> 4];
        out_buf[3 + i * 2 + 1] = hex_chars[byte & 0x0f];
    }
    return out_buf[0 .. 3 + HmacSha256.mac_length * 2];
}

fn runOne(mw: *SlackSignature, ht: anytype) !chain.Outcome {
    var ctx = AuthCtx{
        .alloc = testing.allocator,
        .res = ht.res,
        .req_id = "req_test",
        .write_error = test_fixtures.writeError,
    };
    return mw.execute(&ctx, ht.req);
}

test "slack_signature .next on matching signature within freshness window" {
    test_fixtures.reset();
    const secret = "8f742231b10e8888abcd99yyyzz0ek9i"; // same shape as Slack signing secrets
    const body = "payload=test_webhook_body_data";
    const ts_str = "1699999990"; // 10s before fixedNow (well within 300s)
    var sig_buf: [3 + HmacSha256.mac_length * 2]u8 = undefined;
    const sig = signSlack(secret, ts_str, body, &sig_buf);

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-slack-request-timestamp", ts_str);
    ht.header("x-slack-signature", sig);
    ht.body(body);

    var mw = SlackSignature{ .secret = secret, .now_seconds = fixedNow };
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), test_fixtures.write_count);
}

test "slack_signature short-circuits with UZ-WH-010 when x-slack-signature header missing" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-slack-request-timestamp", "1699999990");
    ht.body("{}");

    var mw = SlackSignature{ .secret = "s", .now_seconds = fixedNow };
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_SIG_INVALID, test_fixtures.last_code);
    try testing.expectEqualStrings("Missing signature header", test_fixtures.last_detail);
}

test "slack_signature short-circuits with UZ-WH-011 on stale timestamp (> 300s old)" {
    test_fixtures.reset();
    const secret = "s";
    const body = "{}";
    const ts_str = "1699999600"; // 400s before fixedNow
    var sig_buf: [3 + HmacSha256.mac_length * 2]u8 = undefined;
    const sig = signSlack(secret, ts_str, body, &sig_buf);

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-slack-request-timestamp", ts_str);
    ht.header("x-slack-signature", sig);
    ht.body(body);

    var mw = SlackSignature{ .secret = secret, .now_seconds = fixedNow };
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_TIMESTAMP_STALE, test_fixtures.last_code);
}

test "slack_signature short-circuits with UZ-WH-010 on tampered body" {
    test_fixtures.reset();
    const secret = "s";
    const ts_str = "1699999990";
    var sig_buf: [3 + HmacSha256.mac_length * 2]u8 = undefined;
    const sig = signSlack(secret, ts_str, "original-body", &sig_buf);

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-slack-request-timestamp", ts_str);
    ht.header("x-slack-signature", sig);
    ht.body("tampered-body"); // does not match the MAC we signed

    var mw = SlackSignature{ .secret = secret, .now_seconds = fixedNow };
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_SIG_INVALID, test_fixtures.last_code);
    try testing.expectEqualStrings("Invalid signature", test_fixtures.last_detail);
}

test "slack_signature short-circuits when signature lacks v0= prefix" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("x-slack-request-timestamp", "1699999990");
    ht.header("x-slack-signature", "abc123"); // no prefix
    ht.body("{}");

    var mw = SlackSignature{ .secret = "s", .now_seconds = fixedNow };
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_WEBHOOK_SIG_INVALID, test_fixtures.last_code);
}

test "isTimestampFresh: inclusive at boundary, rejects one second past" {
    // Covered by webhook_verify.isTimestampFresh in src/zombie/ but re-pinned
    // here on the local copy to prevent drift between the two implementations.
    try testing.expect(slack.isTimestampFresh("1699999700", 1700000000, 300));
    try testing.expect(!slack.isTimestampFresh("1699999699", 1700000000, 300));
    try testing.expect(slack.isTimestampFresh("1700000300", 1700000000, 300));
    try testing.expect(!slack.isTimestampFresh("1700000301", 1700000000, 300));
    try testing.expect(!slack.isTimestampFresh("not-a-number", 1700000000, 300));
    try testing.expect(!slack.isTimestampFresh("0", 1700000000, 300));
}
