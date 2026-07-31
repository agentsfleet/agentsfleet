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

/// Vault-handle field carrying the refresh token to exchange (single-sourced
/// with the connect callbacks and the broker's fingerprint exclusion set).
const FIELD_REFRESH_TOKEN = integration.FIELD_REFRESH_TOKEN;
/// Vault-handle field carrying the data-center accounts server this refresh
/// token is redeemable at (Zoho multi-DC only; absent for single-region
/// providers, which fall back to `cfg.token_endpoint`). Refreshing at the
/// wrong data center's accounts server fails `invalid_grant` exactly like the
/// initial exchange would.
const FIELD_ACCOUNTS_BASE: []const u8 = "accounts_base";
/// Optional vault-handle field overriding the token path appended to
/// `accounts_base` — per-handle configuration, not a provider branch.
const FIELD_TOKEN_PATH: []const u8 = "token_path";
/// Default token path under `accounts_base` (the Zoho shape) when the handle
/// carries no `token_path` of its own.
const DEFAULT_TOKEN_PATH: []const u8 = "/oauth/v2/token";

/// Token-endpoint response fields. The token twins reuse the vault-handle
/// field names — RFC 6749 names both sides of the wire identically.
const RESP_FIELD_ACCESS_TOKEN = integration.FIELD_ACCESS_TOKEN;
const RESP_FIELD_REFRESH_TOKEN = integration.FIELD_REFRESH_TOKEN;
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
/// Pub for the sibling test file, which pins the fallback behavior against it.
pub const DEFAULT_ACCESS_TTL_MS: i64 = 5 * 60 * 1000;
/// Upper bound accepted for a provider `expires_in` (seconds): ten years, far
/// past any real access-token lifetime. Beyond it — or negative, non-finite,
/// or non-numeric — the body is malformed input, never trusted into timestamp
/// arithmetic (`@intFromFloat` of a hostile float is a panic, and an
/// overflowing `now + ttl` is one too).
pub const MAX_EXPIRES_IN_S: i64 = 10 * 365 * 24 * 60 * 60;
/// Floats at/above this magnitude cannot be converted to i64 without trapping.
const MAX_SAFE_FLOAT_I64: f64 = 9.0e18;

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
    // invalid_grant exactly like the initial exchange would. The token path
    // rides the handle too (default: the Zoho shape) and is shape-checked so
    // a future handle writer can never widen where platform credentials are
    // POSTed (leading '/', no query/fragment/whitespace).
    const token_path = strField(obj, FIELD_TOKEN_PATH) orelse DEFAULT_TOKEN_PATH;
    if (!isValidTokenPath(token_path)) return .{ .mint_failed = .permanent };
    const owned_endpoint: ?[]u8 = if (strField(obj, FIELD_ACCOUNTS_BASE)) |base|
        try std.fmt.allocPrint(ctx.alloc, "{s}{s}", .{ base, token_path })
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

    if (resp.status == HTTP_OK) return parseAccess(ctx, refresh_token, resp.body);
    return classifyFailure(ctx.alloc, resp.status, resp.body);
}

/// Build the form-encoded refresh grant. Provider-issued opaque values are
/// percent-encoded before insertion so `+`, `&`, `=`, and `%` stay inside their
/// fields instead of changing the form shape at the token endpoint.
fn buildForm(alloc: std.mem.Allocator, refresh_token: []const u8, app: OauthApp) ![]u8 {
    const rt = try percentEncode(alloc, refresh_token);
    defer alloc.free(rt);
    const cid = try percentEncode(alloc, app.client_id);
    defer alloc.free(cid);
    const csec = try percentEncode(alloc, app.client_secret);
    defer alloc.free(csec);
    return std.fmt.allocPrint(
        alloc,
        "grant_type=refresh_token&refresh_token={s}&client_id={s}&client_secret={s}",
        .{ rt, cid, csec },
    );
}

fn percentEncode(alloc: std.mem.Allocator, raw: []const u8) ![]const u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    for (raw) |c| {
        if (isUnreserved(c)) {
            try out.append(alloc, c);
        } else {
            var buf: [3]u8 = undefined;
            const enc = try std.fmt.bufPrint(&buf, "%{X:0>2}", .{c});
            try out.appendSlice(alloc, enc);
        }
    }
    return out.toOwnedSlice(alloc);
}

fn isUnreserved(c: u8) bool {
    return (c >= 'A' and c <= 'Z') or (c >= 'a' and c <= 'z') or
        (c >= '0' and c <= '9') or c == '-' or c == '.' or c == '_' or c == '~';
}

fn parseAccess(ctx: MintCtx, posted_refresh_token: []const u8, body: []const u8) anyerror!Outcome {
    var parsed = std.json.parseFromSlice(std.json.Value, ctx.alloc, body, .{}) catch return .{ .mint_failed = .permanent };
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return .{ .mint_failed = .permanent },
    };
    const tok = strField(obj, RESP_FIELD_ACCESS_TOKEN) orelse return .{ .mint_failed = .permanent };
    // Absent `expires_in` → conservative default. PRESENT but non-numeric,
    // negative, non-finite, or beyond the cap → malformed body, same as a
    // missing access token — never fed into ttl arithmetic.
    const ttl_ms = ttl: {
        const v = obj.get(RESP_FIELD_EXPIRES_IN) orelse break :ttl DEFAULT_ACCESS_TTL_MS;
        const secs = intValue(v) orelse return .{ .mint_failed = .permanent };
        if (secs < 0 or secs > MAX_EXPIRES_IN_S) return .{ .mint_failed = .permanent };
        break :ttl secs * MS_PER_SECOND;
    };
    const owned_tok = try ctx.alloc.dupe(u8, tok);
    errdefer ctx.alloc.free(owned_tok);
    // A response refresh token that is absent, EMPTY (a malformed provider or
    // broken proxy must not poison the vault with an unusable credential), or
    // merely echoes the posted one is not a rotation; only a genuinely new
    // value is surfaced (and triggers the handler's vault write-back). Deduping
    // here keeps the handler simple — this is the one place that holds both
    // the posted and returned values.
    const rotated: ?[]const u8 = if (strField(obj, RESP_FIELD_REFRESH_TOKEN)) |rt|
        if (rt.len == 0 or std.mem.eql(u8, rt, posted_refresh_token)) null else try ctx.alloc.dupe(u8, rt)
    else
        null;
    return .{ .ok = .{
        .token = owned_tok,
        .expires_at_ms = ctx.now_ms + ttl_ms,
        .rotated_refresh_token = rotated,
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

fn isValidTokenPath(p: []const u8) bool {
    if (p.len == 0 or p[0] != '/') return false;
    for (p) |c| {
        if (c == '?' or c == '#' or c == ' ' or c == '\t' or c == '\r' or c == '\n') return false;
    }
    return true;
}

fn strField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (obj.get(key) orelse return null) {
        .string => |s| s,
        else => null,
    };
}

/// A JSON number as i64, or null when it is not one — including a float that
/// is non-finite or outside the safely-convertible range, where
/// `@intFromFloat` would be safety-checked illegal behavior on
/// provider-controlled bytes. String-typed numerics parse: real OAuth
/// servers emit `"expires_in":"3600"`, and rejecting them would kill mints
/// that succeeded before the hostile-input hardening (review find).
fn intValue(value: std.json.Value) ?i64 {
    return switch (value) {
        .integer => |n| n,
        .float => |n| if (std.math.isFinite(n) and @abs(n) < MAX_SAFE_FLOAT_I64) @intFromFloat(n) else null,
        .string, .number_string => |s| std.fmt.parseInt(i64, s, 10) catch null,
        else => null,
    };
}

// ── Tests ─────────────────────────────────────────────────────────────────────
// The mint-level suite lives in `integration_oauth_refresh_test.zig` (FLL-exempt;
// discovered from `tests.zig`). Only the private-helper test stays in-file.

test "oauth2_refresh mint: form-encodes refresh grant values" {
    const alloc = std.testing.allocator;
    const form = try buildForm(alloc, "rt+a&b=c%", .{ .client_id = "cid+1", .client_secret = "sec&ret=" });
    defer alloc.free(form);
    try std.testing.expectEqualStrings(
        "grant_type=refresh_token&refresh_token=rt%2Ba%26b%3Dc%25&client_id=cid%2B1&client_secret=sec%26ret%3D",
        form,
    );
}
