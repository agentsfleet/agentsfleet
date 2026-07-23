//! GET /v1/connectors/{provider}/callback — Bearer-less.
//!
//! The generic callback handler. The provider redirects the operator's
//! browser here; the signed single-use `state` is the ONLY trust anchor (no
//! Bearer on a cross-site redirect): verify + consume yields the bound
//! workspace, then the archetype decides what completion means — oauth2
//! exchanges the `code` for a token (deadline-armed via `bounded_fetch`) and
//! hands the body to the provider's `post_auth` hook; app_install hands the
//! request to the provider's `complete` hook (its inputs are vendor-bespoke).
//! Ends with a 302 back to the dashboard's connector card.

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
const HTTP_OK: u16 = 200;
const HEADER_LOCATION = "location";
const STATUS_FOUND: u16 = 302;
const DEST_PATH_FMT = "/w/{s}/integrations";
const S_STATE_INVALID = "Invalid or expired connect state";
// Callback wording is the shipped shorter form (no "on this deployment").
const NOT_CONFIGURED_FMT = "{s} connect is not configured";
const NOT_CONFIGURED_FALLBACK = "Connector is not configured";
const EXCHANGE_FAILED_FMT = "{s} token exchange failed";
const EXCHANGE_FAILED_FALLBACK = "Token exchange failed";
const VENDOR_DEADLINE_FMT = "{s} token exchange did not complete in time";
const VENDOR_DEADLINE_FALLBACK = "Token exchange did not complete in time";

pub fn innerCallback(hx: hx_mod.Hx, req: *httpz.Request, provider: []const u8) void {
    const spec = registry.lookup(provider) orelse return registry.respondUnknown(hx, provider);

    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Bad query string");
        return;
    };
    const raw_state = qs.get(Q_STATE) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Missing state");
        return;
    };
    const secret = hx.ctx.approval_signing_secret orelse return failFmt(hx, ec.ERR_CONNECTOR_NOT_CONFIGURED, NOT_CONFIGURED_FMT, NOT_CONFIGURED_FALLBACK, spec);

    switch (spec.archetype) {
        .oauth2 => |o| {
            const code = qs.get(Q_CODE) orelse {
                hx.fail(ec.ERR_INVALID_REQUEST, "Missing code");
                return;
            };
            const workspace_id = oauth2.consumeState(hx.alloc, hx.ctx.queue, o.flow, secret, raw_state, clock.nowMillis()) orelse {
                hx.fail(ec.ERR_CONNECTOR_STATE_INVALID, S_STATE_INVALID);
                return;
            };
            defer hx.alloc.free(workspace_id);

            // Multi-DC providers (Zoho) append `location` to the redirect —
            // absent for single-region providers.
            const location = qs.get(Q_LOCATION);

            completeOauth2(hx, spec, o, workspace_id, code, location) catch |err| {
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
            redirectToDashboard(hx, workspace_id);
        },
        .app_install => |a| {
            const workspace_id = connector_state.verifyConsume(hx.alloc, hx.ctx.queue, a.state, secret, raw_state, clock.nowMillis()) orelse {
                hx.fail(ec.ERR_CONNECTOR_STATE_INVALID, S_STATE_INVALID);
                return;
            };
            defer hx.alloc.free(workspace_id);

            // The hook owns validation + persistence + its failure responses
            // (installation callbacks carry vendor-bespoke inputs).
            if (a.complete(hx, workspace_id, raw_state, req)) redirectToDashboard(hx, workspace_id);
        },
    }
}

/// Creds under a short-lived acquire released BEFORE the vendor exchange —
/// a pool slot never rides a vendor call — then the deadline-armed exchange
/// and the provider's parse-and-persist hook.
fn completeOauth2(hx: hx_mod.Hx, spec: *const registry.ConnectorSpec, o: registry.Oauth2Data, workspace_id: []const u8, code: []const u8, location: ?[]const u8) anyerror!void {
    const creds = blk: {
        const conn: *pg.Conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
        defer hx.ctx.pool.release(conn);
        break :blk oauth2.loadAppCreds(hx.alloc, conn, hx.ctx.platform_admin_workspace_id, spec.provider) orelse return error.NotConfigured;
    };
    defer creds.deinit(hx.alloc);

    const redirect_uri = try connect_h.callbackUrl(hx, spec.provider);
    defer hx.alloc.free(redirect_uri);

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
    const url = std.fmt.allocPrint(hx.res.arena, "{s}" ++ DEST_PATH_FMT, .{ app_url, workspace_id }) catch {
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
