//! GET /v1/connectors/{provider}/callback — legacy callback relay; and
//! POST /v1/connectors/{provider}/callback — Bearer-authenticated completion.
//!
//! Providers now return directly to the dashboard relay. The old API URL stays
//! as a redirect-only compatibility endpoint: it never exchanges a code, reads
//! a state binding, or writes connector data. The dashboard forwards the
//! browser's current Bearer token to `POST /callback`; signed state then binds that
//! authenticated principal and workspace before the provider flow persists.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const clock = @import("common").clock;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const registry = @import("registry.zig");
const oauth2 = @import("oauth2.zig");
const connector_state = @import("state.zig");
const connect_h = @import("connect.zig");

const Q_CODE = "code";
const Q_STATE = "state";
const Q_LOCATION = "location";
const Q_INSTALLATION_ID = "installation_id";
const Q_CALLBACK_SOURCE = "callback_source";
const HTTP_OK: u16 = 200;
const HEADER_LOCATION = "location";
const STATUS_FOUND: u16 = 302;
const DEST_PATH_FMT = "/w/{s}/integrations";
const FMT_STRING_PREFIX = "{s}";
const S_BAD_QUERY_STRING = "Bad query string";
const S_MISSING_STATE = "Missing state";
const S_RELAY_FAILED = "Failed to relay connector callback";
const S_STATE_INVALID = "Invalid or expired connect state";
// Callback wording is the shipped shorter form (no "on this deployment").
const NOT_CONFIGURED_FMT = "{s} connect is not configured";
const NOT_CONFIGURED_FALLBACK = "Connector is not configured";
const EXCHANGE_FAILED_FMT = "{s} token exchange failed";
const EXCHANGE_FAILED_FALLBACK = "Token exchange failed";
const VENDOR_DEADLINE_FMT = "{s} token exchange did not complete in time";
const VENDOR_DEADLINE_FALLBACK = "Token exchange did not complete in time";
const S_IDENTITY_REQUIRED = "Connector callback requires a user identity";
const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";
const S_CALLBACK_SOURCE_INVALID = "Invalid connector callback source";
const CALLBACK_RELAY_PATH_FMT = "/api/connectors/{s}/callback";
const CALLBACK_SOURCE_LEGACY_API = "legacy_api";

/// Compatibility endpoint for an already-registered API callback URL. It
/// forwards the provider's success parameters to the dashboard, which adds the
/// Bearer token before asking the backend to complete. Its fixed source marker
/// lets the backend echo the legacy redirect URI during token exchange. No
/// state is consumed here.
pub fn innerCallbackRelay(hx: hx_mod.Hx, req: *httpz.Request, provider: []const u8) void {
    _ = registry.lookup(provider) orelse return registry.respondUnknown(hx, provider);
    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, S_BAD_QUERY_STRING);
        return;
    };
    const state = qs.get(Q_STATE) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MISSING_STATE);
        return;
    };
    const url = buildRelayUrl(hx, provider, qs.get(Q_CODE), state, qs.get(Q_LOCATION), qs.get(Q_INSTALLATION_ID)) catch {
        common.internalOperationError(hx.res, S_RELAY_FAILED, hx.req_id);
        return;
    };
    defer hx.alloc.free(url);
    const location = hx.res.arena.dupe(u8, url) catch {
        common.internalOperationError(hx.res, S_RELAY_FAILED, hx.req_id);
        return;
    };
    hx.res.status = STATUS_FOUND;
    hx.res.header(HEADER_LOCATION, location);
    hx.res.body = "";
}

/// The only callback endpoint that exchanges a provider code or mutates a
/// connector. It requires the same identity that started the state.
pub fn innerComplete(hx: hx_mod.Hx, req: *httpz.Request, provider: []const u8) void {
    const spec = registry.lookup(provider) orelse return registry.respondUnknown(hx, provider);

    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, S_BAD_QUERY_STRING);
        return;
    };
    const raw_state = qs.get(Q_STATE) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MISSING_STATE);
        return;
    };
    const redirect_uri = callbackUri(hx, provider, qs.get(Q_CALLBACK_SOURCE)) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, S_CALLBACK_SOURCE_INVALID);
        return;
    };
    defer hx.alloc.free(redirect_uri);
    const secret = hx.ctx.approval_signing_secret orelse return failFmt(hx, ec.ERR_CONNECTOR_NOT_CONFIGURED, NOT_CONFIGURED_FMT, NOT_CONFIGURED_FALLBACK, spec);

    switch (spec.archetype) {
        .oauth2 => |o| {
            const code = qs.get(Q_CODE) orelse {
                hx.fail(ec.ERR_INVALID_REQUEST, "Missing code");
                return;
            };
            const verified = verifyAuthorizedState(hx, o.flow.state, secret, raw_state) orelse return;
            defer verified.deinit(hx.alloc);

            // Multi-DC providers (Zoho) append `location` to the redirect —
            // absent for single-region providers.
            const location = qs.get(Q_LOCATION);

            completeOauth2(hx, spec, o, verified.workspace_id, redirect_uri, code, location) catch |err| {
                switch (err) {
                    error.NotConfigured => failFmt(hx, ec.ERR_CONNECTOR_NOT_CONFIGURED, NOT_CONFIGURED_FMT, NOT_CONFIGURED_FALLBACK, spec),
                    error.ExchangeFailed => failFmt(hx, o.exchange_failed_code, EXCHANGE_FAILED_FMT, EXCHANGE_FAILED_FALLBACK, spec),
                    // The armed deadline fired mid-exchange, the deadline could
                    // not be enforced and the call was refused, or the vendor
                    // was unreachable (dial/transport failure) — upstream-call
                    // failures all; no vault write happened (the exchange
                    // precedes it) and the connect is safe to restart.
                    error.DeadlineExceeded, error.SchedulerUnavailable, error.VendorUnreachable => failFmt(hx, ec.ERR_CONNECTOR_VENDOR_DEADLINE, VENDOR_DEADLINE_FMT, VENDOR_DEADLINE_FALLBACK, spec),
                    else => common.internalOperationError(hx.res, "Failed to complete connector connection", hx.req_id),
                }
                return;
            };
            redirectToDashboard(hx, verified.workspace_id);
        },
        .app_install => |a| {
            const verified = verifyAuthorizedState(hx, a.state, secret, raw_state) orelse return;
            defer verified.deinit(hx.alloc);

            // The hook owns validation + persistence + its failure responses
            // (installation callbacks carry vendor-bespoke inputs).
            if (a.complete(hx, verified.workspace_id, raw_state, redirect_uri, req)) redirectToDashboard(hx, verified.workspace_id);
        },
    }
}

fn verifyAuthorizedState(hx: hx_mod.Hx, cfg: connector_state.Config, secret: []const u8, raw_state: []const u8) ?connector_state.VerifiedState {
    const subject = hx.principal.user_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, S_IDENTITY_REQUIRED);
        return null;
    };
    const verified = connector_state.verify(hx.alloc, cfg, secret, raw_state, clock.nowMillis()) orelse {
        hx.fail(ec.ERR_CONNECTOR_STATE_INVALID, S_STATE_INVALID);
        return null;
    };
    if (!connector_state.subjectMatches(cfg, secret, verified, subject)) {
        verified.deinit(hx.alloc);
        hx.fail(ec.ERR_CONNECTOR_STATE_INVALID, S_STATE_INVALID);
        return null;
    }
    var db = hx.db() catch {
        verified.deinit(hx.alloc);
        return null;
    };
    defer db.end();
    if (!common.authorizeWorkspace(db.conn, hx.principal, verified.workspace_id)) {
        verified.deinit(hx.alloc);
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return null;
    }
    if (!connector_state.consume(hx.ctx.queue, cfg, verified)) {
        verified.deinit(hx.alloc);
        hx.fail(ec.ERR_CONNECTOR_STATE_INVALID, S_STATE_INVALID);
        return null;
    }
    return verified;
}

fn buildRelayUrl(hx: hx_mod.Hx, provider: []const u8, code: ?[]const u8, state: []const u8, location: ?[]const u8, installation_id: ?[]const u8) ![]const u8 {
    const app_url = std.mem.trimEnd(u8, hx.ctx.app_url, "/");
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(hx.alloc);
    const prefix = try std.fmt.allocPrint(hx.alloc, FMT_STRING_PREFIX ++ CALLBACK_RELAY_PATH_FMT, .{ app_url, provider });
    defer hx.alloc.free(prefix);
    try out.appendSlice(hx.alloc, prefix);
    if (code) |value| try appendRelayParam(hx.alloc, &out, Q_CODE, value);
    try appendRelayParam(hx.alloc, &out, Q_STATE, state);
    try appendRelayParam(hx.alloc, &out, Q_CALLBACK_SOURCE, CALLBACK_SOURCE_LEGACY_API);
    if (location) |value| try appendRelayParam(hx.alloc, &out, Q_LOCATION, value);
    if (installation_id) |value| try appendRelayParam(hx.alloc, &out, Q_INSTALLATION_ID, value);
    return out.toOwnedSlice(hx.alloc);
}

fn callbackUri(hx: hx_mod.Hx, provider: []const u8, source: ?[]const u8) ![]const u8 {
    if (source) |value| {
        if (!std.mem.eql(u8, value, CALLBACK_SOURCE_LEGACY_API)) return error.InvalidCallbackSource;
        return connect_h.legacyCallbackUrl(hx, provider);
    }
    return connect_h.callbackUrl(hx, provider);
}

fn appendRelayParam(alloc: std.mem.Allocator, out: *std.ArrayList(u8), key: []const u8, value: []const u8) !void {
    const encoded = try percentEncode(alloc, value);
    defer alloc.free(encoded);
    try out.append(alloc, if (out.items.len == 0 or std.mem.indexOfScalar(u8, out.items, '?') == null) '?' else '&');
    try out.appendSlice(alloc, key);
    try out.append(alloc, '=');
    try out.appendSlice(alloc, encoded);
}

fn percentEncode(alloc: std.mem.Allocator, raw: []const u8) ![]const u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    for (raw) |c| {
        const unreserved = (c >= 'A' and c <= 'Z') or (c >= 'a' and c <= 'z') or (c >= '0' and c <= '9') or c == '-' or c == '.' or c == '_' or c == '~';
        if (unreserved) {
            try out.append(alloc, c);
        } else {
            var buf: [3]u8 = undefined;
            try out.appendSlice(alloc, try std.fmt.bufPrint(&buf, "%{X:0>2}", .{c}));
        }
    }
    return out.toOwnedSlice(alloc);
}

/// Creds under a short-lived acquire released BEFORE the vendor exchange —
/// a pool slot never rides a vendor call — then the deadline-armed exchange
/// and the provider's parse-and-persist hook.
fn completeOauth2(hx: hx_mod.Hx, spec: *const registry.ConnectorSpec, o: registry.Oauth2Data, workspace_id: []const u8, redirect_uri: []const u8, code: []const u8, location: ?[]const u8) anyerror!void {
    const creds = blk: {
        const conn: *pg.Conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
        defer hx.ctx.pool.release(conn);
        break :blk oauth2.loadAppCreds(hx.alloc, conn, hx.ctx.platform_admin_workspace_id, spec.provider) orelse return error.NotConfigured;
    };
    defer creds.deinit(hx.alloc);

    // Effective flow: production uses the provider's real token endpoint,
    // overridden per-request for multi-DC providers (Zoho) via `location` —
    // the code is only redeemable at the data-center-specific accounts
    // server that issued it. An integration test points
    // `connector_oauth_token_endpoint_override` at a loopback fake-provider
    // so the exchange never dials the real vendor; the override always wins.
    var eff_flow = o.flow;
    if (o.resolve_token_endpoint) |resolve| eff_flow.token_endpoint = resolve(location);
    if (hx.ctx.connector_oauth_token_endpoint_override) |ep| eff_flow.token_endpoint = ep;
    const result = try oauth2.exchange(hx.alloc, hx.ctx.io, hx.ctx.deadline_scheduler, eff_flow, creds, code, redirect_uri);
    defer hx.alloc.free(result.body);
    if (result.status != HTTP_OK) return error.ExchangeFailed;

    try o.post_auth(hx, workspace_id, result.body, location);
}

fn redirectToDashboard(hx: hx_mod.Hx, workspace_id: []const u8) void {
    // The Location value must outlive the handler: httpz writes response
    // headers AFTER the dispatcher's per-request arena (hx.alloc) is freed, so
    // it lives on res.arena (owned until the response is written).
    const app_url = std.mem.trimEnd(u8, hx.ctx.app_url, "/");
    const url = std.fmt.allocPrint(hx.res.arena, FMT_STRING_PREFIX ++ DEST_PATH_FMT, .{ app_url, workspace_id }) catch {
        // The connection succeeded; a redirect-build failure is cosmetic, so
        // return 200 rather than a 500 over a missing app_url.
        hx.ok(.ok, .{ .status = "connected" });
        return;
    };
    hx.res.status = STATUS_FOUND;
    hx.res.header(HEADER_LOCATION, url);
    hx.res.body = "";
}

/// `hx.fail` with a display-name-interpolated detail (static fallback on OOM).
fn failFmt(hx: hx_mod.Hx, code: []const u8, comptime fmt: []const u8, fallback: []const u8, spec: *const registry.ConnectorSpec) void {
    const detail = std.fmt.allocPrint(hx.alloc, fmt, .{spec.display_name}) catch return hx.fail(code, fallback);
    defer hx.alloc.free(detail);
    hx.fail(code, detail);
}
