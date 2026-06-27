//! POST /v1/workspaces/{ws}/connectors/github/connect — workspace-authed.
//!
//! Mints a single-use signed state bound to the workspace and returns the
//! GitHub App install URL the browser is redirected to. No token is created or
//! stored here; the round-trip finishes at `callback.zig`, which writes the
//! vault handle the credential broker mints from.

const std = @import("std");
const logging = @import("log");
const clock = @import("common").clock;
const common = @import("../../common.zig");
const hx_mod = @import("../../hx.zig");
const ec = @import("../../../../errors/error_registry.zig");
const state = @import("state.zig");

const log = logging.scoped(.connector_github);

pub const Context = common.Context;

const INSTALL_URL_FMT = "https://github.com/apps/{s}/installations/new?state={s}";
const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";
const S_NOT_CONFIGURED = "GitHub connect is not configured on this deployment";

pub fn innerConnectGithub(hx: hx_mod.Hx, workspace_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    // Both the signing secret and the App slug are platform config resolved at
    // boot; absent either, connect degrades closed rather than minting a state
    // that can never complete.
    const secret = hx.ctx.approval_signing_secret orelse {
        hx.fail(ec.ERR_CONNECTOR_NOT_CONFIGURED, S_NOT_CONFIGURED);
        return;
    };
    const slug = hx.ctx.github_app_slug orelse {
        hx.fail(ec.ERR_CONNECTOR_NOT_CONFIGURED, S_NOT_CONFIGURED);
        return;
    };

    const st = state.mint(hx.alloc, hx.ctx.queue, secret, workspace_id, clock.nowMillis()) catch {
        common.internalOperationError(hx.res, "Failed to start GitHub connect", hx.req_id);
        return;
    };
    defer hx.alloc.free(st);

    // state is base64url + '.' + hex — all URL-safe, so it rides the query
    // unescaped. The slug is the platform App's GitHub handle.
    const url = std.fmt.allocPrint(hx.alloc, INSTALL_URL_FMT, .{ slug, st }) catch {
        common.internalOperationError(hx.res, "Failed to build install URL", hx.req_id);
        return;
    };
    defer hx.alloc.free(url);

    log.debug("connect_initiated", .{ .workspace_id = workspace_id });
    hx.ok(.ok, .{ .install_url = url });
}
