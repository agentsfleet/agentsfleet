//! Tests for the `oauth2_refresh` mint strategy — extracted from
//! `integration_oauth_refresh.zig` (FLL: production files stay ≤350 lines;
//! `_test.zig` is exempt). Shared harness: `testing.FakeGitHub` stands in as
//! the provider token endpoint. Covers the exchange happy path, the multi-DC
//! endpoint selection, expiry fallback, failure classification, and the
//! rotated-refresh-token capture (persist-if-rotated).

const std = @import("std");
const integration = @import("integration.zig");
const oauth_refresh = @import("integration_oauth_refresh.zig");
const testing = @import("testing.zig");

const mint = oauth_refresh.mint;
const MintCtx = integration.MintCtx;
const OAuth2Refresh = integration.OAuth2Refresh;
const OauthApp = integration.OauthApp;
const Retry = integration.Retry;

const MS_PER_S: i64 = 1000;
const TEST_NOW_MS: i64 = 1_700_000_000_000;
const TEST_EXPIRES_IN_S: i64 = 3600;
const TEST_EXPIRES_IN_TEXT = std.fmt.comptimePrint("{d}", .{TEST_EXPIRES_IN_S});
const HANDLE_ZOHO = "{\"integration\":\"zoho\",\"refresh_token\":\"rt_zoho_abc\"}";
const HANDLE_ZOHO_EU = "{\"integration\":\"zoho\",\"refresh_token\":\"rt_zoho_abc\",\"accounts_base\":\"https://accounts.zoho.eu\"}";
const TEST_CFG = OAuth2Refresh{ .token_endpoint = "https://accounts.test/oauth/v2/token", .app = testAppSelector };

fn testAppSelector(p: integration.PlatformSecrets) ?OauthApp {
    return p.zoho;
}

/// A `MintCtx` wired with a fake token endpoint + the fake oauth app, over `handle`.
fn refreshCtx(alloc: std.mem.Allocator, handle: std.json.Value, vendor: *testing.FakeGitHub) MintCtx {
    return .{
        .alloc = alloc,
        .handle = handle,
        .now_ms = TEST_NOW_MS,
        .platform = .{ .zoho = testing.fake_oauth_app },
        .http = vendor.exchange(),
        .sign = testing.fakeSign,
    };
}

test "oauth2_refresh mint: 200 → access token with local expiry; refresh token posted, not returned (Dimension 3.3)" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = "{\"access_token\":\"at_fresh\",\"expires_in\":" ++ TEST_EXPIRES_IN_TEXT ++ "}" };
    defer vendor.deinit();
    var h = try testing.parse(alloc, HANDLE_ZOHO);
    defer h.deinit();

    const out = try mint(refreshCtx(alloc, h.value, &vendor), TEST_CFG);
    try std.testing.expect(out == .ok);
    defer alloc.free(out.ok.token);
    try std.testing.expectEqualStrings("at_fresh", out.ok.token);
    try std.testing.expectEqual(TEST_NOW_MS + TEST_EXPIRES_IN_S * MS_PER_S, out.ok.expires_at_ms);
    // The refresh token is the request credential (posted) but never the result:
    // the runner-facing token carries only the fresh access token.
    try std.testing.expect(std.mem.indexOf(u8, vendor.body, "rt_zoho_abc") != null);
    try std.testing.expect(std.mem.indexOf(u8, out.ok.token, "rt_zoho_abc") == null);
    // Auth is in the body, not a bearer header (the broker sends no bearer).
    try std.testing.expectEqual(@as(usize, 0), vendor.bearer.len);
}

test "oauth2_refresh mint: test_refresh_response_rotates_token — a new refresh_token in the response is surfaced" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = "{\"access_token\":\"at_fresh\",\"expires_in\":" ++ TEST_EXPIRES_IN_TEXT ++ ",\"refresh_token\":\"rt_rotated_new\"}" };
    defer vendor.deinit();
    var h = try testing.parse(alloc, HANDLE_ZOHO);
    defer h.deinit();

    const out = try mint(refreshCtx(alloc, h.value, &vendor), TEST_CFG);
    try std.testing.expect(out == .ok);
    defer alloc.free(out.ok.token);
    // The rotated token rides alongside the fresh access token, caller-owned.
    try std.testing.expect(out.ok.rotated_refresh_token != null);
    defer alloc.free(out.ok.rotated_refresh_token.?);
    try std.testing.expectEqualStrings("rt_rotated_new", out.ok.rotated_refresh_token.?);
    try std.testing.expectEqualStrings("at_fresh", out.ok.token);
}

test "oauth2_refresh mint: test_refresh_response_no_rotation — an absent or echoed refresh_token yields null" {
    const alloc = std.testing.allocator;
    // (a) The response omits refresh_token entirely (non-rotating provider).
    {
        var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = "{\"access_token\":\"at_fresh\",\"expires_in\":" ++ TEST_EXPIRES_IN_TEXT ++ "}" };
        defer vendor.deinit();
        var h = try testing.parse(alloc, HANDLE_ZOHO);
        defer h.deinit();
        const out = try mint(refreshCtx(alloc, h.value, &vendor), TEST_CFG);
        try std.testing.expect(out == .ok);
        defer alloc.free(out.ok.token);
        try std.testing.expect(out.ok.rotated_refresh_token == null);
    }
    // (b) The response echoes the posted token unchanged — not a rotation, no
    // needless write-back.
    {
        var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = "{\"access_token\":\"at_fresh\",\"expires_in\":" ++ TEST_EXPIRES_IN_TEXT ++ ",\"refresh_token\":\"rt_zoho_abc\"}" };
        defer vendor.deinit();
        var h = try testing.parse(alloc, HANDLE_ZOHO);
        defer h.deinit();
        const out = try mint(refreshCtx(alloc, h.value, &vendor), TEST_CFG);
        try std.testing.expect(out == .ok);
        defer alloc.free(out.ok.token);
        try std.testing.expect(out.ok.rotated_refresh_token == null);
    }
    // (c) An EMPTY refresh_token is a malformed rotation, never persisted — a
    // broken provider/proxy must not poison the vault with an unusable token.
    {
        var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = "{\"access_token\":\"at_fresh\",\"expires_in\":" ++ TEST_EXPIRES_IN_TEXT ++ ",\"refresh_token\":\"\"}" };
        defer vendor.deinit();
        var h = try testing.parse(alloc, HANDLE_ZOHO);
        defer h.deinit();
        const out = try mint(refreshCtx(alloc, h.value, &vendor), TEST_CFG);
        try std.testing.expect(out == .ok);
        defer alloc.free(out.ok.token);
        try std.testing.expect(out.ok.rotated_refresh_token == null);
    }
}

test "oauth2_refresh mint: should leak nothing at every allocation-failure point (exhaustive injection)" {
    // mint() deliberately converts some internal failures into outcomes rather
    // than propagating (transport → mint_failed), so this is a FailingAllocator
    // sweep instead of std.testing.checkAllAllocationFailures: fail every
    // allocation index in turn and let the backing testing.allocator assert
    // zero leaks whichever exit (ok / outcome / error) each injection takes.
    // Proves the parseAccess errdefer chain (owned token freed when the rotated
    // dupe fails) and every earlier partial-build path.
    const alloc = std.testing.allocator;
    var fail_index: usize = 0;
    var swept_past_all_sites = false;
    while (fail_index < 64) : (fail_index += 1) {
        var failing = std.testing.FailingAllocator.init(alloc, .{ .fail_index = fail_index });
        var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = "{\"access_token\":\"at_fresh\",\"expires_in\":" ++ TEST_EXPIRES_IN_TEXT ++ ",\"refresh_token\":\"rt_rotated_new\"}" };
        defer vendor.deinit();
        var h = try testing.parse(alloc, HANDLE_ZOHO);
        defer h.deinit();
        const out = mint(refreshCtx(failing.allocator(), h.value, &vendor), TEST_CFG) catch continue;
        switch (out) {
            .ok => |m| {
                failing.allocator().free(m.token);
                if (m.rotated_refresh_token) |rt| failing.allocator().free(rt);
            },
            else => {},
        }
        // An iteration that completed WITHOUT inducing a failure means the
        // fail_index exceeded mint's real allocation count — every site has
        // now been failed once, so the sweep is provably exhaustive.
        if (!failing.has_induced_failure) {
            swept_past_all_sites = true;
            break;
        }
    }
    try std.testing.expect(swept_past_all_sites);
}

test "oauth2_refresh mint: a handle with accounts_base refreshes at ITS data center, not cfg.token_endpoint" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = "{\"access_token\":\"at_eu\",\"expires_in\":" ++ TEST_EXPIRES_IN_TEXT ++ "}" };
    defer vendor.deinit();
    var h = try testing.parse(alloc, HANDLE_ZOHO_EU);
    defer h.deinit();

    const out = try mint(refreshCtx(alloc, h.value, &vendor), TEST_CFG);
    try std.testing.expect(out == .ok);
    defer alloc.free(out.ok.token);
    // The EU handle's stored accounts_base wins over TEST_CFG's single-region
    // default — this is the exact bug class greptile flagged: a hardcoded
    // token_endpoint means every non-US refresh dies invalid_grant.
    try std.testing.expectEqualStrings("https://accounts.zoho.eu/oauth/v2/token", vendor.url);
}

test "oauth2_refresh mint: a handle without accounts_base falls back to cfg.token_endpoint" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = "{\"access_token\":\"at_default\",\"expires_in\":" ++ TEST_EXPIRES_IN_TEXT ++ "}" };
    defer vendor.deinit();
    var h = try testing.parse(alloc, HANDLE_ZOHO);
    defer h.deinit();

    const out = try mint(refreshCtx(alloc, h.value, &vendor), TEST_CFG);
    try std.testing.expect(out == .ok);
    defer alloc.free(out.ok.token);
    try std.testing.expectEqualStrings(TEST_CFG.token_endpoint, vendor.url);
}

test "oauth2_refresh mint: missing expires_in falls back to the conservative floor" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200, .resp_body = "{\"access_token\":\"at_no_ttl\"}" };
    defer vendor.deinit();
    var h = try testing.parse(alloc, HANDLE_ZOHO);
    defer h.deinit();
    const out = try mint(refreshCtx(alloc, h.value, &vendor), TEST_CFG);
    try std.testing.expect(out == .ok);
    defer alloc.free(out.ok.token);
    try std.testing.expectEqual(TEST_NOW_MS + oauth_refresh.DEFAULT_ACCESS_TTL_MS, out.ok.expires_at_ms);
}

test "oauth2_refresh mint: invalid_grant → reconnect_required in a single attempt (Dimension 3.2)" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 400, .resp_body = "{\"error\":\"invalid_grant\"}" };
    defer vendor.deinit();
    var h = try testing.parse(alloc, HANDLE_ZOHO);
    defer h.deinit();
    const out = try mint(refreshCtx(alloc, h.value, &vendor), TEST_CFG);
    try std.testing.expect(out == .reconnect_required);
    // No retry storm: exactly one bounded exchange per mint request.
    try std.testing.expectEqual(@as(usize, 1), vendor.calls);
}

test "oauth2_refresh mint: status → outcome mapping (transient 5xx, permanent other-4xx)" {
    const alloc = std.testing.allocator;
    const cases = [_]struct { status: u16, retry: Retry }{
        .{ .status = 503, .retry = .transient }, // upstream fault → retry
        .{ .status = 429, .retry = .permanent }, // other 4xx, no invalid_grant → permanent
    };
    for (cases) |c| {
        var vendor = testing.FakeGitHub{ .alloc = alloc, .status = c.status, .resp_body = "{\"error\":\"temporarily_unavailable\"}" };
        defer vendor.deinit();
        var h = try testing.parse(alloc, HANDLE_ZOHO);
        defer h.deinit();
        const out = try mint(refreshCtx(alloc, h.value, &vendor), TEST_CFG);
        try std.testing.expect(out == .mint_failed);
        try std.testing.expectEqual(c.retry, out.mint_failed);
    }
}

test "oauth2_refresh mint: a handle without a refresh token reconnects without calling the vendor" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200 };
    defer vendor.deinit();
    var h = try testing.parse(alloc, "{\"integration\":\"zoho\"}");
    defer h.deinit();
    try std.testing.expect((try mint(refreshCtx(alloc, h.value, &vendor), TEST_CFG)) == .reconnect_required);
    try std.testing.expectEqual(@as(usize, 0), vendor.calls);
}

test "oauth2_refresh mint: an unconfigured platform app fails permanent without a call" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .status = 200 };
    defer vendor.deinit();
    var h = try testing.parse(alloc, HANDLE_ZOHO);
    defer h.deinit();
    var ctx = refreshCtx(alloc, h.value, &vendor);
    ctx.platform = .{}; // no zoho app configured
    const out = try mint(ctx, TEST_CFG);
    try std.testing.expect(out == .mint_failed);
    try std.testing.expectEqual(Retry.permanent, out.mint_failed);
    try std.testing.expectEqual(@as(usize, 0), vendor.calls);
}

test "oauth2_refresh mint: a transport error is a transient mint_failed (failure injection)" {
    const alloc = std.testing.allocator;
    var vendor = testing.FakeGitHub{ .alloc = alloc, .fail_with = error.ConnectionRefused };
    defer vendor.deinit();
    var h = try testing.parse(alloc, HANDLE_ZOHO);
    defer h.deinit();
    const out = try mint(refreshCtx(alloc, h.value, &vendor), TEST_CFG);
    try std.testing.expect(out == .mint_failed);
    try std.testing.expectEqual(Retry.transient, out.mint_failed);
}
