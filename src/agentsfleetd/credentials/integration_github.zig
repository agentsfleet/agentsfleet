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

/// RSA signature scratch (4096-bit key ⇒ 512 bytes).
const MAX_SIG_LEN = 512;

/// Mint an installation token. `reconnect_required` when the install is gone
/// (401/404) or the handle lacks an installation id; `mint_failed` on a transport
/// error, a non-2xx other status, an unparseable body, or an unconfigured platform.
pub fn mint(ctx: MintCtx) anyerror!Outcome {
    const obj = switch (ctx.handle) {
        .object => |o| o,
        else => return .mint_failed,
    };
    const installation_id = strField(obj, FIELD_INSTALLATION_ID) orelse return .reconnect_required;
    const app = ctx.platform.github orelse return .mint_failed; // platform unconfigured

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
    }) catch return .mint_failed;
    defer ctx.alloc.free(resp.body);

    return switch (resp.status) {
        HTTP_CREATED => parseToken(ctx, resp.body),
        HTTP_UNAUTHORIZED, HTTP_NOT_FOUND => .reconnect_required,
        else => .mint_failed,
    };
}

fn parseToken(ctx: MintCtx, body: []const u8) anyerror!Outcome {
    var parsed = std.json.parseFromSlice(std.json.Value, ctx.alloc, body, .{}) catch return .mint_failed;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return .mint_failed,
    };
    const tok = strField(obj, RESP_FIELD_TOKEN) orelse return .mint_failed;
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

// ── Tests (pure — fake signer + fake GitHub, throwaway key, no network) ───────

const FAKE_APP = GithubApp{
    .app_id = "123456",
    // pin test: a distinctive non-secret marker; the key-never-leaks test asserts
    // this string never reaches the outbound request or the minted token.
    .private_key_pem = "FAKE_PRIVATE_KEY_MATERIAL_zzz",
};

/// Fake RS256 signer — returns a fixed marker (real signing is proven separately
/// in `rs256_sign.zig`); here we exercise JWT assembly + exchange + outcome mapping.
fn fakeSign(out: []u8, private_key_pem: []const u8, signing_input: []const u8) anyerror![]const u8 {
    _ = private_key_pem;
    _ = signing_input;
    const marker = "FAKESIG";
    @memcpy(out[0..marker.len], marker);
    return out[0..marker.len];
}

/// Fake GitHub: replies with a canned status + body, and captures the outbound
/// url + bearer so tests can assert on them.
const FakeGitHub = struct {
    alloc: std.mem.Allocator,
    status: u16,
    resp_body: []const u8,
    url: []u8 = &.{},
    bearer: []u8 = &.{},

    fn post(ptr: *anyopaque, alloc: std.mem.Allocator, req: integration.HttpRequest) anyerror!integration.HttpResponse {
        const self: *FakeGitHub = @ptrCast(@alignCast(ptr));
        self.url = try self.alloc.dupe(u8, req.url);
        self.bearer = try self.alloc.dupe(u8, req.bearer);
        return .{ .status = self.status, .body = try alloc.dupe(u8, self.resp_body) };
    }

    fn exchange(self: *FakeGitHub) integration.HttpExchange {
        return .{ .ptr = self, .postFn = post };
    }

    fn deinit(self: *FakeGitHub) void {
        if (self.url.len != 0) self.alloc.free(self.url);
        if (self.bearer.len != 0) self.alloc.free(self.bearer);
    }
};

fn ghCtx(alloc: std.mem.Allocator, handle: std.json.Value, gh: *FakeGitHub, now_ms: i64) MintCtx {
    return .{
        .alloc = alloc,
        .handle = handle,
        .now_ms = now_ms,
        .platform = .{ .github = FAKE_APP },
        .http = gh.exchange(),
        .sign = fakeSign,
    };
}

fn parseHandle(alloc: std.mem.Allocator, comptime json: []const u8) !std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, alloc, json, .{});
}

const HANDLE_GH = "{\"integration\":\"github\",\"installation_id\":\"42\"}";
const TEST_NOW_MS: i64 = 1_700_000_000_000;

test "github mint: 201 → installation token with future expiry; JWT targets the install (Dimension 2.1)" {
    const alloc = std.testing.allocator;
    var gh = FakeGitHub{ .alloc = alloc, .status = HTTP_CREATED, .resp_body = "{\"token\":\"ghs_minted\",\"expires_at\":\"2026-06-26T16:30:00Z\"}" };
    defer gh.deinit();
    var h = try parseHandle(alloc, HANDLE_GH);
    defer h.deinit();

    const out = try mint(ghCtx(alloc, h.value, &gh, TEST_NOW_MS));
    try std.testing.expect(out == .ok);
    defer alloc.free(out.ok.token);
    try std.testing.expectEqualStrings("ghs_minted", out.ok.token);
    try std.testing.expectEqual(TEST_NOW_MS + INSTALL_TOKEN_TTL_MS, out.ok.expires_at_ms);
    // URL targets the right installation; bearer is a 3-segment JWT carrying app_id.
    try std.testing.expect(std.mem.indexOf(u8, gh.url, "/app/installations/42/access_tokens") != null);
    try std.testing.expectEqual(@as(usize, 2), std.mem.count(u8, gh.bearer, "."));
}

test "github mint: 404 / 401 (install gone or JWT rejected) → reconnect_required (Dimension 2.2)" {
    const alloc = std.testing.allocator;
    for ([_]u16{ HTTP_NOT_FOUND, HTTP_UNAUTHORIZED }) |status| {
        var gh = FakeGitHub{ .alloc = alloc, .status = status, .resp_body = "{\"message\":\"gone\"}" };
        defer gh.deinit();
        var h = try parseHandle(alloc, HANDLE_GH);
        defer h.deinit();
        try std.testing.expect((try mint(ghCtx(alloc, h.value, &gh, TEST_NOW_MS))) == .reconnect_required);
    }
}

test "github mint: the App private key never reaches the outbound request or the token (Dimension 2.3)" {
    const alloc = std.testing.allocator;
    var gh = FakeGitHub{ .alloc = alloc, .status = HTTP_CREATED, .resp_body = "{\"token\":\"ghs_minted\"}" };
    defer gh.deinit();
    var h = try parseHandle(alloc, HANDLE_GH);
    defer h.deinit();

    const out = try mint(ghCtx(alloc, h.value, &gh, TEST_NOW_MS));
    try std.testing.expect(out == .ok);
    defer alloc.free(out.ok.token);
    const key = FAKE_APP.private_key_pem;
    try std.testing.expect(std.mem.indexOf(u8, gh.bearer, key) == null);
    try std.testing.expect(std.mem.indexOf(u8, gh.url, key) == null);
    try std.testing.expect(std.mem.indexOf(u8, out.ok.token, key) == null);
}

test "github mint: a GitHub 5xx is a retryable mint_failed, not a reconnect" {
    const alloc = std.testing.allocator;
    var gh = FakeGitHub{ .alloc = alloc, .status = 503, .resp_body = "upstream" };
    defer gh.deinit();
    var h = try parseHandle(alloc, HANDLE_GH);
    defer h.deinit();
    try std.testing.expect((try mint(ghCtx(alloc, h.value, &gh, TEST_NOW_MS))) == .mint_failed);
}

test "github mint: a handle with no installation id reconnects without calling GitHub" {
    const alloc = std.testing.allocator;
    var gh = FakeGitHub{ .alloc = alloc, .status = HTTP_CREATED, .resp_body = "{}" };
    defer gh.deinit();
    var h = try parseHandle(alloc, "{\"integration\":\"github\"}");
    defer h.deinit();
    try std.testing.expect((try mint(ghCtx(alloc, h.value, &gh, TEST_NOW_MS))) == .reconnect_required);
    try std.testing.expectEqual(@as(usize, 0), gh.url.len); // GitHub never called
}

test "github mint: an unconfigured platform (no App key) fails closed" {
    const alloc = std.testing.allocator;
    var gh = FakeGitHub{ .alloc = alloc, .status = HTTP_CREATED, .resp_body = "{}" };
    defer gh.deinit();
    var h = try parseHandle(alloc, HANDLE_GH);
    defer h.deinit();
    var ctx = ghCtx(alloc, h.value, &gh, TEST_NOW_MS);
    ctx.platform = .{}; // no github app configured
    try std.testing.expect((try mint(ctx)) == .mint_failed);
}
