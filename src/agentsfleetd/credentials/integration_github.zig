//! The `github` integration: mint a short-lived GitHub App installation token.
//! Builds + RS256-signs an App JSON Web Token (JWT), exchanges it at GitHub's
//! installation access-token endpoint, and maps the result to a typed `Outcome`.
//! The RS256 signer, the outbound HTTP boundary, and the clock are injected via
//! `MintCtx`, so this stays std-only and unit-testable with a fake key + fake
//! GitHub (no network). The App private key is read only from `ctx.platform` —
//! never logged, never returned (RULE VLT).

const std = @import("std");
const integration = @import("integration.zig");

const MintCtx = integration.MintCtx;
const Outcome = integration.Outcome;
const GithubApp = integration.GithubApp;

/// Vault-handle field naming the App installation to mint for.
const FIELD_INSTALLATION_ID: []const u8 = "installation_id";

const GITHUB_API_BASE = "https://api.github.com";
const ACCESS_TOKEN_PATH_PREFIX = "/app/installations/";
const ACCESS_TOKEN_PATH_SUFFIX = "/access_tokens";
const ACCEPT_GITHUB = "application/vnd.github+json";
const USER_AGENT = "agentsfleetd";

/// App JWT lifetime (RFC 7519 seconds). `iat` is back-dated to absorb clock skew
/// against GitHub; `exp` stays under GitHub's 10-minute ceiling.
const JWT_IAT_BACKDATE_S: i64 = 60;
const JWT_EXP_AHEAD_S: i64 = 540;

/// Installation tokens last one hour. We bound expiry locally (now + this) rather
/// than parse GitHub's RFC3339 string: a conservative floor re-mints early, never
/// late, and keeps a date parser off the security path.
const INSTALL_TOKEN_TTL_MS: i64 = 60 * 60 * 1000;

const JWT_HEADER_RS256 = "{\"alg\":\"RS256\",\"typ\":\"JWT\"}";

/// Response field carrying the minted installation token.
const RESP_FIELD_TOKEN: []const u8 = "token";

const HTTP_CREATED: u16 = 201;
const HTTP_UNAUTHORIZED: u16 = 401;
const HTTP_NOT_FOUND: u16 = 404;
/// Status at/above this is an upstream fault (retryable); below it (other 4xx) is
/// a permanent client-side failure.
const HTTP_SERVER_ERROR_FLOOR: u16 = 500;

/// RSA signature scratch (4096-bit key ⇒ 512 bytes).
const MAX_SIG_LEN = 512;

/// Mint an installation token. `reconnect_required` when the install is gone
/// (401/404) or the handle lacks an installation id; `mint_failed{transient}` on a
/// transport error or 5xx; `mint_failed{permanent}` on a malformed body, other
/// 4xx, or an unconfigured platform.
pub fn mint(ctx: MintCtx) anyerror!Outcome {
    const obj = switch (ctx.handle) {
        .object => |o| o,
        else => return .{ .mint_failed = .permanent },
    };
    const installation_id = strField(obj, FIELD_INSTALLATION_ID) orelse return .reconnect_required;
    const app = ctx.platform.github orelse return .{ .mint_failed = .permanent }; // platform unconfigured

    const jwt = try buildAppJwt(ctx, app);
    defer ctx.alloc.free(jwt);

    const url = try std.fmt.allocPrint(
        ctx.alloc,
        GITHUB_API_BASE ++ ACCESS_TOKEN_PATH_PREFIX ++ "{s}" ++ ACCESS_TOKEN_PATH_SUFFIX,
        .{installation_id},
    );
    defer ctx.alloc.free(url);

    const resp = ctx.http.post(ctx.alloc, .{
        .url = url,
        .bearer = jwt,
        .accept = ACCEPT_GITHUB,
        .user_agent = USER_AGENT,
        .body = "",
    }) catch return .{ .mint_failed = .transient }; // network / timeout → retryable
    defer ctx.alloc.free(resp.body);

    return switch (resp.status) {
        HTTP_CREATED => parseToken(ctx, resp.body),
        HTTP_UNAUTHORIZED, HTTP_NOT_FOUND => .reconnect_required,
        else => classifyHttpFailure(resp.status),
    };
}

fn classifyHttpFailure(status: u16) Outcome {
    return .{ .mint_failed = if (status >= HTTP_SERVER_ERROR_FLOOR) .transient else .permanent };
}

fn parseToken(ctx: MintCtx, body: []const u8) anyerror!Outcome {
    var parsed = std.json.parseFromSlice(std.json.Value, ctx.alloc, body, .{}) catch return .{ .mint_failed = .permanent };
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return .{ .mint_failed = .permanent },
    };
    const tok = strField(obj, RESP_FIELD_TOKEN) orelse return .{ .mint_failed = .permanent };
    return .{ .ok = .{
        .token = try ctx.alloc.dupe(u8, tok),
        .expires_at_ms = ctx.now_ms + INSTALL_TOKEN_TTL_MS,
    } };
}

/// `b64url(header).b64url(claims).b64url(signature)` — iss=app_id, iat back-dated,
/// exp bounded. The signing input is RS256-signed via the injected signer.
fn buildAppJwt(ctx: MintCtx, app: GithubApp) ![]u8 {
    const enc = std.base64.url_safe_no_pad.Encoder;
    const now_s = @divFloor(ctx.now_ms, 1000);

    var claims_buf: [256]u8 = undefined;
    const claims = try std.fmt.bufPrint(&claims_buf, "{{\"iat\":{d},\"exp\":{d},\"iss\":\"{s}\"}}", .{
        now_s - JWT_IAT_BACKDATE_S,
        now_s + JWT_EXP_AHEAD_S,
        app.app_id,
    });

    const hdr_len = enc.calcSize(JWT_HEADER_RS256.len);
    const claims_len = enc.calcSize(claims.len);
    const signing_input_len = hdr_len + 1 + claims_len;
    const signing_input = try ctx.alloc.alloc(u8, signing_input_len);
    defer ctx.alloc.free(signing_input);
    _ = enc.encode(signing_input[0..hdr_len], JWT_HEADER_RS256);
    signing_input[hdr_len] = '.';
    _ = enc.encode(signing_input[hdr_len + 1 ..], claims);

    var sig_buf: [MAX_SIG_LEN]u8 = undefined;
    const sig = try ctx.sign(&sig_buf, app.private_key_pem, signing_input);

    const sig_b64_len = enc.calcSize(sig.len);
    const jwt = try ctx.alloc.alloc(u8, signing_input_len + 1 + sig_b64_len);
    @memcpy(jwt[0..signing_input_len], signing_input);
    jwt[signing_input_len] = '.';
    _ = enc.encode(jwt[signing_input_len + 1 ..], sig);
    return jwt;
}

fn strField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const v = obj.get(key) orelse return null;
    return switch (v) {
        .string => |s| s,
        else => null,
    };
}

// ── Tests (shared harness: testing.FakeGitHub + fake signer, no network) ──────

const testing = @import("testing.zig");
const Retry = integration.Retry;

const HANDLE_GH = "{\"integration\":\"github\",\"installation_id\":\"42\"}";
const TEST_NOW_MS: i64 = 1_700_000_000_000;

const ExpectTag = enum { ok, reconnect, failed };

test "github mint: status → outcome mapping incl. retry class (Dimensions 2.1/2.2)" {
    const alloc = std.testing.allocator;
    const cases = [_]struct { status: u16, tag: ExpectTag, retry: ?Retry }{
        .{ .status = 201, .tag = .ok, .retry = null },
        .{ .status = 401, .tag = .reconnect, .retry = null }, // JWT rejected
        .{ .status = 404, .tag = .reconnect, .retry = null }, // installation gone
        .{ .status = 503, .tag = .failed, .retry = .transient }, // upstream fault → retry
        .{ .status = 429, .tag = .failed, .retry = .permanent }, // other 4xx → permanent (coarse)
        .{ .status = 422, .tag = .failed, .retry = .permanent },
    };
    for (cases) |c| {
        var gh = testing.FakeGitHub{ .alloc = alloc, .status = c.status };
        defer gh.deinit();
        var h = try testing.parse(alloc, HANDLE_GH);
        defer h.deinit();
        const out = try mint(testing.githubCtx(alloc, h.value, &gh, TEST_NOW_MS));
        switch (c.tag) {
            .ok => {
                try std.testing.expect(out == .ok);
                alloc.free(out.ok.token);
            },
            .reconnect => try std.testing.expect(out == .reconnect_required),
            .failed => {
                try std.testing.expect(out == .mint_failed);
                try std.testing.expectEqual(c.retry.?, out.mint_failed);
            },
        }
    }
}

test "github mint: 201 → token with local expiry; URL targets the install; bearer is a 3-part JWT (Dimension 2.1)" {
    const alloc = std.testing.allocator;
    var gh = testing.FakeGitHub{ .alloc = alloc, .status = 201, .resp_body = "{\"token\":\"ghs_minted\",\"expires_at\":\"2026-06-26T16:30:00Z\"}" };
    defer gh.deinit();
    var h = try testing.parse(alloc, HANDLE_GH);
    defer h.deinit();

    const out = try mint(testing.githubCtx(alloc, h.value, &gh, TEST_NOW_MS));
    try std.testing.expect(out == .ok);
    defer alloc.free(out.ok.token);
    try std.testing.expectEqualStrings("ghs_minted", out.ok.token);
    try std.testing.expectEqual(TEST_NOW_MS + INSTALL_TOKEN_TTL_MS, out.ok.expires_at_ms);
    try std.testing.expect(std.mem.indexOf(u8, gh.url, "/app/installations/42/access_tokens") != null);
    try std.testing.expectEqual(@as(usize, 2), std.mem.count(u8, gh.bearer, "."));
}

test "github mint: the App private key never reaches the outbound request or the token (Dimension 2.3)" {
    const alloc = std.testing.allocator;
    var gh = testing.FakeGitHub{ .alloc = alloc, .status = 201 };
    defer gh.deinit();
    var h = try testing.parse(alloc, HANDLE_GH);
    defer h.deinit();

    const out = try mint(testing.githubCtx(alloc, h.value, &gh, TEST_NOW_MS));
    try std.testing.expect(out == .ok);
    defer alloc.free(out.ok.token);
    const key = testing.fake_app.private_key_pem;
    try std.testing.expect(std.mem.indexOf(u8, gh.bearer, key) == null);
    try std.testing.expect(std.mem.indexOf(u8, gh.url, key) == null);
    try std.testing.expect(std.mem.indexOf(u8, out.ok.token, key) == null);
}

test "github mint: a transport error is a transient mint_failed (failure injection)" {
    const alloc = std.testing.allocator;
    var gh = testing.FakeGitHub{ .alloc = alloc, .fail_with = error.ConnectionRefused };
    defer gh.deinit();
    var h = try testing.parse(alloc, HANDLE_GH);
    defer h.deinit();
    const out = try mint(testing.githubCtx(alloc, h.value, &gh, TEST_NOW_MS));
    try std.testing.expect(out == .mint_failed);
    try std.testing.expectEqual(Retry.transient, out.mint_failed);
}

test "github mint: a handle with no installation id reconnects without calling GitHub" {
    const alloc = std.testing.allocator;
    var gh = testing.FakeGitHub{ .alloc = alloc, .status = 201 };
    defer gh.deinit();
    var h = try testing.parse(alloc, "{\"integration\":\"github\"}");
    defer h.deinit();
    try std.testing.expect((try mint(testing.githubCtx(alloc, h.value, &gh, TEST_NOW_MS))) == .reconnect_required);
    try std.testing.expectEqual(@as(usize, 0), gh.calls); // GitHub never called
}

test "github mint: an unconfigured platform (no App key) fails permanent" {
    const alloc = std.testing.allocator;
    var gh = testing.FakeGitHub{ .alloc = alloc, .status = 201 };
    defer gh.deinit();
    var h = try testing.parse(alloc, HANDLE_GH);
    defer h.deinit();
    var ctx = testing.githubCtx(alloc, h.value, &gh, TEST_NOW_MS);
    ctx.platform = .{}; // no github app configured
    const out = try mint(ctx);
    try std.testing.expect(out == .mint_failed);
    try std.testing.expectEqual(Retry.permanent, out.mint_failed);
}
