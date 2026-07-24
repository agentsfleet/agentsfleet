//! Shared OAuth 2.0 authorization-code connector mechanism — the reusable flow
//! for every token-exchange connector (Slack now; Zoho/Jira/Linear later reuse
//! it as data). A connector is a `Spec` (endpoints + scopes + state binding);
//! its thin `connectors/<name>/{connect,callback}.zig` handlers call
//! `authorizeUrl`/`mintState`/`consumeState`/`exchange` with that Spec, then do
//! the provider-specific response parsing + vault-handle shaping themselves.
//!
//! GitHub is intentionally NOT on this mechanism — it is a GitHub App
//! *installation*, not an OAuth-2.0 code exchange (a different shape).

const std = @import("std");
const pg = @import("pg");
const bounded_fetch = @import("bounded_fetch.zig");
const connector_state = @import("state.zig");
const queue_redis = @import("../../../queue/redis.zig");
const vault = @import("../../../state/vault.zig");

/// Per-connector OAuth 2.0 descriptor. Endpoints + scopes are provider data;
/// `state` binds the connector's install-state domain (see `state.zig`).
pub const Spec = struct {
    provider: []const u8,
    authorize_endpoint: []const u8,
    token_endpoint: []const u8,
    scopes: []const u8,
    /// Provider-specific authorization query tail, without a leading `&`.
    /// Values are compile-time constants that are already URL-safe (for
    /// example Atlassian's `audience=api.atlassian.com`).
    authorize_extra_query: []const u8 = "",
    state: connector_state.Config,
};

/// Platform app credentials for one connector, resolved from the admin-workspace
/// vault `<provider>-app` entry. One OAuth app per connector, all tenants.
pub const AppCreds = struct {
    client_id: []const u8,
    client_secret: []const u8,

    pub fn deinit(self: AppCreds, alloc: std.mem.Allocator) void {
        alloc.free(self.client_id);
        alloc.free(self.client_secret);
    }
};

/// Raw token-exchange response. The caller parses the provider-specific JSON
/// (Slack's `access_token`/`bot_user_id`/`team`, …) and owns `.body`.
pub const ExchangeResult = struct {
    status: u16,
    body: []const u8,
};

/// Mint a signed single-use install-state bound to `workspace_id`, in the
/// connector's state domain. Caller owns the returned slice.
pub fn mintState(
    alloc: std.mem.Allocator,
    queue: *queue_redis.Client,
    spec: Spec,
    secret: []const u8,
    workspace_id: []const u8,
    now_ms: i64,
) ![]const u8 {
    return connector_state.mint(alloc, queue, spec.state, secret, workspace_id, now_ms);
}

/// Verify + single-use consume an install-state; returns the bound workspace_id
/// (caller owns) or null on any failure.
pub fn consumeState(
    alloc: std.mem.Allocator,
    queue: *queue_redis.Client,
    spec: Spec,
    secret: []const u8,
    state: []const u8,
    now_ms: i64,
) ?[]const u8 {
    return connector_state.verifyConsume(alloc, queue, spec.state, secret, state, now_ms);
}

/// Build the provider authorize URL. `scopes`/`client_id`/`state` are URL-safe
/// (scope slugs, `[A-Za-z0-9.]` client ids, base64url+hex state); only
/// `redirect_uri` carries reserved chars and is percent-encoded. Caller owns it.
pub fn authorizeUrl(
    alloc: std.mem.Allocator,
    spec: Spec,
    client_id: []const u8,
    redirect_uri: []const u8,
    state: []const u8,
) ![]const u8 {
    const redir = try percentEncode(alloc, redirect_uri);
    defer alloc.free(redir);
    const scope = try percentEncode(alloc, spec.scopes);
    defer alloc.free(scope);
    if (spec.authorize_extra_query.len > 0) {
        return std.fmt.allocPrint(alloc, "{s}?response_type=code&client_id={s}&scope={s}&redirect_uri={s}&state={s}&{s}", .{
            spec.authorize_endpoint, client_id, scope, redir, state, spec.authorize_extra_query,
        });
    }
    return std.fmt.allocPrint(alloc, "{s}?response_type=code&client_id={s}&scope={s}&redirect_uri={s}&state={s}", .{
        spec.authorize_endpoint, client_id, scope, redir, state,
    });
}

/// Exchange an authorization `code` for tokens at the connector's token endpoint
/// (form-encoded POST), deadline-armed via `bounded_fetch` (fail-closed: a
/// deadline that cannot be enforced refuses the call). Returns the raw status +
/// body for provider-specific parsing. `io` is injected so tests can drive a
/// loopback server. Never logs the body — it carries the client secret +
/// minted token (RULE VLT).
pub fn exchange(
    alloc: std.mem.Allocator,
    io: std.Io,
    sched: *bounded_fetch.Scheduler,
    spec: Spec,
    creds: AppCreds,
    code: []const u8,
    redirect_uri: []const u8,
) !ExchangeResult {
    const body = try buildExchangeForm(alloc, creds, code, redirect_uri);
    defer alloc.free(body);

    const headers = [_]std.http.Header{
        .{ .name = "content-type", .value = "application/x-www-form-urlencoded" },
        .{ .name = "accept", .value = "application/json" },
    };
    const res = try bounded_fetch.fetch(alloc, io, sched, .{
        .url = spec.token_endpoint,
        .method = .POST,
        .payload = body,
        .extra_headers = &headers,
        .deadline_ms = bounded_fetch.TOKEN_EXCHANGE_DEADLINE_MS,
        .provider = spec.provider,
        .class = .token_exchange,
    });
    return .{ .status = res.status, .body = res.body };
}

fn buildExchangeForm(alloc: std.mem.Allocator, creds: AppCreds, code: []const u8, redirect_uri: []const u8) ![]u8 {
    const cid = try percentEncode(alloc, creds.client_id);
    defer alloc.free(cid);
    const csec = try percentEncode(alloc, creds.client_secret);
    defer alloc.free(csec);
    const encoded_code = try percentEncode(alloc, code);
    defer alloc.free(encoded_code);
    const redir = try percentEncode(alloc, redirect_uri);
    defer alloc.free(redir);
    return std.fmt.allocPrint(
        alloc,
        "grant_type=authorization_code&client_id={s}&client_secret={s}&code={s}&redirect_uri={s}",
        .{ cid, csec, encoded_code, redir },
    );
}

pub const APP_VAULT_KEY_SUFFIX = "-app";
const FIELD_CLIENT_ID = "client_id";
const FIELD_CLIENT_SECRET = "client_secret";

/// Load + parse the admin-workspace `<provider>-app` vault entry — the shared
/// platform-app secret bag (client id/secret for the OAuth exchange, plus any
/// provider-specific fields like Slack's `signing_secret` the ingress reads).
/// Caller owns the returned `Parsed` (`.deinit()`); null = unconfigured/missing.
/// The single site that builds the `<provider>-app` key (RULE UFS).
pub fn loadAppVaultJson(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    admin_ws_id: []const u8,
    provider: []const u8,
) ?std.json.Parsed(std.json.Value) {
    if (admin_ws_id.len == 0) return null;
    const key = std.fmt.allocPrint(alloc, "{s}" ++ APP_VAULT_KEY_SUFFIX, .{provider}) catch return null;
    defer alloc.free(key);
    return vault.loadJson(alloc, conn, admin_ws_id, key) catch return null;
}

/// Resolve a connector's platform app creds from the admin-workspace vault
/// `<provider>-app` entry (one OAuth app per connector, serving all tenants).
/// Read on-demand — connect/callback are rare browser flows, not a hot path —
/// so a new connector needs no `Context` field, just a vaulted secret. Caller
/// owns the result via `AppCreds.deinit`. Null = unconfigured/missing → the
/// handler fails closed.
pub fn loadAppCreds(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    admin_ws_id: []const u8,
    provider: []const u8,
) ?AppCreds {
    var parsed = loadAppVaultJson(alloc, conn, admin_ws_id, provider) orelse return null;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return null,
    };
    const cid = strField(obj, FIELD_CLIENT_ID) orelse return null;
    const csec = strField(obj, FIELD_CLIENT_SECRET) orelse return null;

    // `errdefer` does NOT fire on `return null` from a `?T` function (only on an
    // error-typed return), so free `cid_owned` explicitly if the second dupe fails
    // — otherwise the partial init leaks on OOM (RULE: free on ALL paths).
    const cid_owned = alloc.dupe(u8, cid) catch return null;
    const csec_owned = alloc.dupe(u8, csec) catch {
        alloc.free(cid_owned);
        return null;
    };
    return .{ .client_id = cid_owned, .client_secret = csec_owned };
}

fn strField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const v = obj.get(key) orelse return null;
    return switch (v) {
        .string => |s| s,
        else => null,
    };
}

/// RFC 3986 percent-encode: pass unreserved bytes, `%XX` everything else.
/// BUFFER GATE: ArrayList(u8) — append-as-you-go, bounded ~3x input (short URLs).
fn percentEncode(alloc: std.mem.Allocator, raw: []const u8) ![]const u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    for (raw) |c| {
        if (isUnreserved(c)) {
            try out.append(alloc, c);
        } else {
            var buf: [3]u8 = undefined;
            const enc = try std.fmt.bufPrint(&buf, "%{X:0>2}", .{c}); // "%XX" fits [3]u8
            try out.appendSlice(alloc, enc);
        }
    }
    return out.toOwnedSlice(alloc);
}

fn isUnreserved(c: u8) bool {
    return (c >= 'A' and c <= 'Z') or (c >= 'a' and c <= 'z') or
        (c >= '0' and c <= '9') or c == '-' or c == '.' or c == '_' or c == '~';
}

// ── Tests (pure URL building + encoding; exchange is integration-gated) ───────

const testing = std.testing;
const T_SPEC = Spec{
    .provider = "test",
    .authorize_endpoint = "https://example.test/authorize",
    .token_endpoint = "https://example.test/token",
    .scopes = "read,write",
    .state = .{ .domain_prefix = "test:v1:", .nonce_prefix = "connect:test:nonce:" },
};

test "authorizeUrl: composes endpoint + params, percent-encodes redirect_uri" {
    const url = try authorizeUrl(testing.allocator, T_SPEC, "CID123", "https://app.test/cb", "st.mac");
    defer testing.allocator.free(url);
    try testing.expectEqualStrings(
        "https://example.test/authorize?response_type=code&client_id=CID123&scope=read%2Cwrite&redirect_uri=https%3A%2F%2Fapp.test%2Fcb&state=st.mac",
        url,
    );
}

test "percentEncode: unreserved pass through, reserved become %XX" {
    const enc = try percentEncode(testing.allocator, "a-b_c.d~e/f:g h");
    defer testing.allocator.free(enc);
    try testing.expectEqualStrings("a-b_c.d~e%2Ff%3Ag%20h", enc);
}

test "buildExchangeForm: form-encodes code exchange values" {
    const form = try buildExchangeForm(testing.allocator, .{ .client_id = "cid+1", .client_secret = "sec&ret=" }, "co+de&x", "https://app.test/cb?x=1&y=2");
    defer testing.allocator.free(form);
    try testing.expectEqualStrings(
        "grant_type=authorization_code&client_id=cid%2B1&client_secret=sec%26ret%3D&code=co%2Bde%26x&redirect_uri=https%3A%2F%2Fapp.test%2Fcb%3Fx%3D1%26y%3D2",
        form,
    );
}
