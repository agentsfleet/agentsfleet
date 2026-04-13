//! Unit tests for oauth_state middleware. Split from oauth_state.zig per the
//! sibling-test convention used by webhook_hmac / slack_signature / jwks.

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const oauth_state = @import("oauth_state.zig");

const AuthCtx = auth_ctx.AuthCtx;
const OAuthState = oauth_state.OAuthState;
const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;

const test_fixtures = struct {
    var last_code: []const u8 = "";
    var last_detail: []const u8 = "";
    var write_count: usize = 0;
    var consume_call_count: usize = 0;
    var consume_last_nonce: []const u8 = "";
    var consume_returns: bool = true;
    var consume_errors: bool = false;

    fn reset() void {
        last_code = "";
        last_detail = "";
        write_count = 0;
        consume_call_count = 0;
        consume_last_nonce = "";
        consume_returns = true;
        consume_errors = false;
    }

    fn writeError(_: *httpz.Response, code: []const u8, detail: []const u8, _: []const u8) void {
        last_code = code;
        last_detail = detail;
        write_count += 1;
    }

    fn consumeNonce(_: *anyopaque, nonce: []const u8) anyerror!bool {
        consume_call_count += 1;
        consume_last_nonce = nonce;
        if (consume_errors) return error.TestRedisDown;
        return consume_returns;
    }
};

fn signedState(alloc: std.mem.Allocator, secret: []const u8, nonce: []const u8, workspace_id: []const u8) ![]const u8 {
    var mac: [HmacSha256.mac_length]u8 = undefined;
    var h = HmacSha256.init(secret);
    h.update(nonce);
    h.update(":");
    h.update(workspace_id);
    h.final(&mac);
    const hex = std.fmt.bytesToHex(mac, .lower);
    return std.fmt.allocPrint(alloc, "{s}.{s}.{s}", .{ nonce, workspace_id, &hex });
}

fn runOne(mw: *OAuthState, ht: anytype) !chain.Outcome {
    var ctx = AuthCtx{
        .alloc = testing.allocator,
        .res = ht.res,
        .req_id = "req_test",
        .write_error = test_fixtures.writeError,
    };
    return mw.execute(&ctx, ht.req);
}

fn makeMw(secret: []const u8) OAuthState {
    var dummy: u8 = 0;
    return .{
        .signing_secret = secret,
        .consume_ctx = @ptrCast(&dummy),
        .consume_nonce = test_fixtures.consumeNonce,
    };
}

test "oauth_state .next on valid state + nonce consume success" {
    test_fixtures.reset();
    const secret = "state-signing-secret";
    const state = try signedState(testing.allocator, secret, "nonce1234567890abcdef", "ws_alpha");
    defer testing.allocator.free(state);

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.query("state", state);

    var mw = makeMw(secret);
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), test_fixtures.write_count);
    try testing.expectEqual(@as(usize, 1), test_fixtures.consume_call_count);
    try testing.expectEqualStrings("nonce1234567890abcdef", test_fixtures.consume_last_nonce);
}

test "oauth_state short-circuits when state query param is missing" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mw = makeMw("s");
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(oauth_state.ERR_OAUTH_STATE_INVALID, test_fixtures.last_code);
    try testing.expectEqualStrings("state required", test_fixtures.last_detail);
    try testing.expectEqual(@as(usize, 0), test_fixtures.consume_call_count);
}

test "oauth_state short-circuits on malformed state (missing dots)" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.query("state", "justastringnoperiods");

    var mw = makeMw("s");
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(oauth_state.ERR_OAUTH_STATE_INVALID, test_fixtures.last_code);
    try testing.expectEqualStrings("Invalid state", test_fixtures.last_detail);
}

test "oauth_state short-circuits on HMAC mismatch (forged state)" {
    test_fixtures.reset();
    const real_secret = "real";
    const forger_secret = "forger";
    const state = try signedState(testing.allocator, forger_secret, "nonce1234567890abcdef", "ws_alpha");
    defer testing.allocator.free(state);

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.query("state", state);

    var mw = makeMw(real_secret);
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(oauth_state.ERR_OAUTH_STATE_INVALID, test_fixtures.last_code);
    try testing.expectEqualStrings("OAuth state invalid", test_fixtures.last_detail);
    // Consume must NOT be called when HMAC fails — we reject before touching Redis.
    try testing.expectEqual(@as(usize, 0), test_fixtures.consume_call_count);
}

test "oauth_state short-circuits when nonce has already been consumed (replay)" {
    test_fixtures.reset();
    test_fixtures.consume_returns = false; // backing store reports "not found"
    const secret = "s";
    const state = try signedState(testing.allocator, secret, "nonce1234567890abcdef", "ws_alpha");
    defer testing.allocator.free(state);

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.query("state", state);

    var mw = makeMw(secret);
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings("OAuth state expired or replayed", test_fixtures.last_detail);
    try testing.expectEqual(@as(usize, 1), test_fixtures.consume_call_count);
}

test "oauth_state short-circuits distinctly when nonce store is unavailable" {
    test_fixtures.reset();
    test_fixtures.consume_errors = true; // Redis unreachable
    const secret = "s";
    const state = try signedState(testing.allocator, secret, "nonce1234567890abcdef", "ws_alpha");
    defer testing.allocator.free(state);

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.query("state", state);

    var mw = makeMw(secret);
    const outcome = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings("Nonce store unavailable", test_fixtures.last_detail);
}
