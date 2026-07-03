//! POST /v1/workspaces/{ws}/connectors/{provider}/connect — workspace-authed.
//!
//! The generic connect handler: resolves `{provider}` against the registry
//! (unknown → 404 naming it) and dispatches on the archetype. oauth2 mints a
//! single-use signed state and returns the provider authorize URL; app_install
//! mints the same state and returns the vendor install URL. No token is
//! created or stored here — the round-trip finishes at the generic callback.
//! Per-provider deltas live in the registry entry's data + hooks, never here.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");
const clock = @import("common").clock;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const matchers = @import("../../route_matchers_connectors.zig");
const registry = @import("registry.zig");
const oauth2 = @import("oauth2.zig");
const connector_state = @import("state.zig");

const log = logging.scoped(.connectors);

const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";
const S_CONNECT_START_FAILED = "Failed to start connector connect";
const EV_CONNECT_INITIATED = "connect_initiated";
// Detail strings interpolate the registry display name so each provider keeps
// its shipped wording ("Slack connect is not configured on this deployment").
const NOT_CONFIGURED_FMT = "{s} connect is not configured on this deployment";
const NOT_CONFIGURED_FALLBACK = "Connector is not configured on this deployment";
/// The one site that spells the callback path shape (RULE UFS); the generic
/// callback route serves it for every provider.
const CALLBACK_PATH_FMT = "/v1/connectors/{s}/callback";

pub fn innerConnect(hx: hx_mod.Hx, route: matchers.WorkspaceConnectorRoute) void {
    const spec = registry.lookup(route.provider) orelse return registry.respondUnknown(hx, route.provider);

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, route.workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    switch (spec.archetype) {
        .oauth2 => |o| {
            const secret = hx.ctx.approval_signing_secret orelse return failNotConfigured(hx, spec);
            connectOauth2(hx, conn, spec, o, route.workspace_id, secret);
        },
        .app_install => |a| {
            const secret = hx.ctx.approval_signing_secret orelse return failNotConfigured(hx, spec);
            connectAppInstall(hx, spec, a, route.workspace_id, secret);
        },
    }
}

/// oauth2: platform app creds from the admin vault (fail-loud when the
/// `<provider>-app` bag is absent) → signed state → provider authorize URL.
fn connectOauth2(hx: hx_mod.Hx, conn: *pg.Conn, spec: *const registry.ConnectorSpec, o: registry.Oauth2Data, workspace_id: []const u8, secret: []const u8) void {
    const creds = oauth2.loadAppCreds(hx.alloc, conn, hx.ctx.platform_admin_workspace_id, spec.provider) orelse
        return failNotConfigured(hx, spec);
    defer creds.deinit(hx.alloc);

    const st = oauth2.mintState(hx.alloc, hx.ctx.queue, o.flow, secret, workspace_id, clock.nowMillis()) catch {
        common.internalOperationError(hx.res, S_CONNECT_START_FAILED, hx.req_id);
        return;
    };
    defer hx.alloc.free(st);

    const redirect_uri = callbackUrl(hx, spec.provider) catch {
        common.internalOperationError(hx.res, "Failed to build connector callback URL", hx.req_id);
        return;
    };
    defer hx.alloc.free(redirect_uri);

    const url = oauth2.authorizeUrl(hx.alloc, o.flow, creds.client_id, redirect_uri, st) catch {
        common.internalOperationError(hx.res, "Failed to build connector authorize URL", hx.req_id);
        return;
    };
    defer hx.alloc.free(url);

    log.debug(EV_CONNECT_INITIATED, .{ .provider = spec.provider, .workspace_id = workspace_id });
    hx.ok(.ok, .{ .install_url = url });
}

/// app_install: signed state → the provider's install URL (built by the
/// registry hook from platform config).
fn connectAppInstall(hx: hx_mod.Hx, spec: *const registry.ConnectorSpec, a: registry.AppInstallData, workspace_id: []const u8, secret: []const u8) void {
    const st = connector_state.mint(hx.alloc, hx.ctx.queue, a.state, secret, workspace_id, clock.nowMillis()) catch {
        common.internalOperationError(hx.res, S_CONNECT_START_FAILED, hx.req_id);
        return;
    };
    defer hx.alloc.free(st);

    const url = a.build_install_url(hx, st) catch |err| switch (err) {
        error.NotConfigured => return failNotConfigured(hx, spec),
        error.OutOfMemory => {
            common.internalOperationError(hx.res, "Failed to build install URL", hx.req_id);
            return;
        },
    };
    defer hx.alloc.free(url);

    log.debug(EV_CONNECT_INITIATED, .{ .provider = spec.provider, .workspace_id = workspace_id });
    hx.ok(.ok, .{ .install_url = url });
}

/// The absolute callback URL registered with the provider — one site builds
/// it for connect (redirect_uri) and the callback's own exchange.
pub fn callbackUrl(hx: hx_mod.Hx, provider: []const u8) ![]const u8 {
    const path = try std.fmt.allocPrint(hx.alloc, CALLBACK_PATH_FMT, .{provider});
    defer hx.alloc.free(path);
    return std.fmt.allocPrint(hx.alloc, "{s}{s}", .{ hx.ctx.api_url, path });
}

/// 503 UZ-CONN-001 with the provider's shipped wording.
fn failNotConfigured(hx: hx_mod.Hx, spec: *const registry.ConnectorSpec) void {
    const detail = std.fmt.allocPrint(hx.alloc, NOT_CONFIGURED_FMT, .{spec.display_name}) catch
        return hx.fail(ec.ERR_CONNECTOR_NOT_CONFIGURED, NOT_CONFIGURED_FALLBACK);
    defer hx.alloc.free(detail);
    hx.fail(ec.ERR_CONNECTOR_NOT_CONFIGURED, detail);
}
