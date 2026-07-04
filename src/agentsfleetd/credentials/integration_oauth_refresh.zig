//! The `oauth2_refresh` mint strategy: exchange a vaulted refresh token for a
//! fresh short-lived access token at the provider token endpoint. Serves the
//! refresh-token connectors (Zoho Desk, Jira, Linear) on the shared credential
//! broker — the declarative twin of the `github` App-JWT mint.
//!
//! The refresh token is read only from the vault handle and the client secret
//! only from `ctx.platform`; neither is logged nor returned (RULE VLT). The
//! runner receives ONLY the minted access token. std-only + unit-testable via the
//! injected `MintCtx` HTTP boundary (no network).

const std = @import("std");
const integration = @import("integration.zig");

const MintCtx = integration.MintCtx;
const Outcome = integration.Outcome;
const OAuth2Refresh = integration.OAuth2Refresh;
const OauthApp = integration.OauthApp;

/// Vault-handle field carrying the refresh token to exchange.
const FIELD_REFRESH_TOKEN: []const u8 = "refresh_token";
/// Vault-handle field carrying the data-center accounts server this refresh
/// token is redeemable at (Zoho multi-DC only; absent for single-region
/// providers, which fall back to `cfg.token_endpoint`). Refreshing at the
/// wrong data center's accounts server fails `invalid_grant` exactly like the
/// initial exchange would.
const FIELD_ACCOUNTS_BASE: []const u8 = "accounts_base";
const ZOHO_TOKEN_PATH: []const u8 = "/oauth/v2/token";

/// Token-endpoint response fields.
const RESP_FIELD_ACCESS_TOKEN: []const u8 = "access_token";
const RESP_FIELD_EXPIRES_IN: []const u8 = "expires_in";
const RESP_FIELD_ERROR: []const u8 = "error";

/// OAuth 2.0 error code meaning the refresh token is dead — revoked or expired —
/// so the user must reconnect. Distinct from a transport fault (which retries).
const ERR_INVALID_GRANT: []const u8 = "invalid_grant";

const CONTENT_TYPE_FORM: []const u8 = "application/x-www-form-urlencoded";
const ACCEPT_JSON: []const u8 = "application/json";

const HTTP_OK: u16 = 200;
/// Status at/above this is an upstream fault (retryable); below it (other 4xx) is
/// a permanent client-side failure — bad client creds or a malformed request.
const HTTP_SERVER_ERROR_FLOOR: u16 = 500;

const MS_PER_SECOND: i64 = 1000;
/// Access-token lifetime (ms) when the response omits `expires_in`. A conservative
/// floor re-mints early, never late — mirrors the github mint's local-expiry floor.
const DEFAULT_ACCESS_TTL_MS: i64 = 5 * 60 * 1000;

/// Mint a fresh access token from the handle's refresh token. `reconnect_required`
/// when the handle lacks a refresh token or the vendor reports `invalid_grant`
/// (revoked); `mint_failed{transient}` on a transport error or 5xx;
/// `mint_failed{permanent}` on a malformed body, other 4xx, or an unconfigured
/// platform app.
pub fn mint(ctx: MintCtx, cfg: OAuth2Refresh) anyerror!Outcome {
    const obj = switch (ctx.handle) {
        .object => |o| o,
        else => return .{ .mint_failed = .permanent },
    };
    const refresh_token = strField(obj, FIELD_REFRESH_TOKEN) orelse return .reconnect_required;
    const app = cfg.app(ctx.platform) orelse return .{ .mint_failed = .permanent }; // platform unconfigured

    const body = try buildForm(ctx.alloc, refresh_token, app);
    defer ctx.alloc.free(body);

    // The handle's own accounts_base (multi-DC providers) wins over the
    // single-region default — refreshing at the wrong data center fails
    // invalid_grant exactly like the initial exchange would.
    const owned_endpoint: ?[]u8 = if (strField(obj, FIELD_ACCOUNTS_BASE)) |base|
        try std.fmt.allocPrint(ctx.alloc, "{s}{s}", .{ base, ZOHO_TOKEN_PATH })
    else
        null;
    defer if (owned_endpoint) |ep| ctx.alloc.free(ep);
    const effective_endpoint = owned_endpoint orelse cfg.token_endpoint;

    const resp = ctx.http.post(ctx.alloc, .{
        .url = effective_endpoint,
        .bearer = null, // token-endpoint auth rides the form body, not a bearer header
        .accept = ACCEPT_JSON,
        .content_type = CONTENT_TYPE_FORM,
        .body = body,
    }) catch return .{ .mint_failed = .transient }; // network / timeout → retryable
    defer ctx.alloc.free(resp.body);

    if (resp.status == HTTP_OK) return parseAccess(ctx, resp.body);
    return classifyFailure(ctx.alloc, resp.status, resp.body);
}

/// Build the form-encoded refresh grant. Provider-issued opaque values are sent
/// verbatim, exactly as `connectors/oauth2.zig`'s code exchange does against these
/// same token endpoints — the refresh grant is that exchange's twin.
fn buildForm(alloc: std.mem.Allocator, refresh_token: []const u8, app: OauthApp) ![]u8 {
    return std.fmt.allocPrint(
        alloc,
        "grant_type=refresh_token&refresh_token={s}&client_id={s}&client_secret={s}",
        .{ refresh_token, app.client_id, app.client_secret },
    );
}

fn parseAccess(ctx: MintCtx, body: []const u8) anyerror!Outcome {
    var parsed = std.json.parseFromSlice(std.json.Value, ctx.alloc, body, .{}) catch return .{ .mint_failed = .permanent };
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return .{ .mint_failed = .permanent },
    };
    const tok = strField(obj, RESP_FIELD_ACCESS_TOKEN) orelse return .{ .mint_failed = .permanent };
    const ttl_ms = if (intField(obj, RESP_FIELD_EXPIRES_IN)) |secs| secs * MS_PER_SECOND else DEFAULT_ACCESS_TTL_MS;
    return .{ .ok = .{
        .token = try ctx.alloc.dupe(u8, tok),
        .expires_at_ms = ctx.now_ms + ttl_ms,
    } };
}

/// A dead refresh token (`invalid_grant`, any status) is `reconnect_required`;
/// everything else classifies by status like the github mint.
fn classifyFailure(alloc: std.mem.Allocator, status: u16, body: []const u8) Outcome {
    if (isInvalidGrant(alloc, body)) return .reconnect_required;
    return .{ .mint_failed = if (status >= HTTP_SERVER_ERROR_FLOOR) .transient else .permanent };
}

fn isInvalidGrant(alloc: std.mem.Allocator, body: []const u8) bool {
    var parsed = std.json.parseFromSlice(std.json.Value, alloc, body, .{}) catch return false;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return false,
    };
    const err = strField(obj, RESP_FIELD_ERROR) orelse return false;
    return std.mem.eql(u8, err, ERR_INVALID_GRANT);
}

fn strField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (obj.get(key) orelse return null) {
        .string => |s| s,
        else => null,
    };
}

fn intField(obj: std.json.ObjectMap, key: []const u8) ?i64 {
    return switch (obj.get(key) orelse return null) {
        .integer => |n| n,
        .float => |n| @intFromFloat(n),
        else => null,
    };
}

// ── Tests (shared harness: testing.FakeGitHub as the token endpoint) ──────────

const testing = @import("testing.zig");
const Retry = integration.Retry;

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
    try std.testing.expectEqual(TEST_NOW_MS + TEST_EXPIRES_IN_S * MS_PER_SECOND, out.ok.expires_at_ms);
    // The refresh token is the request credential (posted) but never the result:
    // the runner-facing token carries only the fresh access token.
    try std.testing.expect(std.mem.indexOf(u8, vendor.body, "rt_zoho_abc") != null);
    try std.testing.expect(std.mem.indexOf(u8, out.ok.token, "rt_zoho_abc") == null);
    // Auth is in the body, not a bearer header (the broker sends no bearer).
    try std.testing.expectEqual(@as(usize, 0), vendor.bearer.len);
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
    try std.testing.expectEqual(TEST_NOW_MS + DEFAULT_ACCESS_TTL_MS, out.ok.expires_at_ms);
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
