//! POST /v1/workspaces/{ws}/connectors/slack/connect — workspace-authed.
//!
//! Mints a single-use signed state bound to the workspace and returns the Slack
//! OAuth authorize URL the browser is redirected to. No token here — the
//! round-trip finishes at `callback.zig`, which exchanges the code + vaults the
//! bot token. Drives the shared OAuth-2.0 mechanism with the Slack `Spec`.

const std = @import("std");
const logging = @import("log");
const clock = @import("common").clock;
const common = @import("../../common.zig");
const hx_mod = @import("../../hx.zig");
const ec = @import("../../../../errors/error_registry.zig");
const oauth2 = @import("../oauth2.zig");
const spec = @import("spec.zig");

const log = logging.scoped(.connector_slack);

const S_ACCESS_DENIED = "Workspace access denied";
const S_NOT_CONFIGURED = "Slack connect is not configured on this deployment";
const CALLBACK_PATH = "/v1/connectors/slack/callback";

pub fn innerConnectSlack(hx: hx_mod.Hx, workspace_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_ACCESS_DENIED);
        return;
    }

    // State is signed with the platform signing secret (domain-separated to
    // Slack in the Spec); the OAuth app's client_id comes from the admin vault.
    const secret = hx.ctx.approval_signing_secret orelse {
        hx.fail(ec.ERR_CONNECTOR_NOT_CONFIGURED, S_NOT_CONFIGURED);
        return;
    };
    const creds = oauth2.loadAppCreds(hx.alloc, conn, hx.ctx.platform_admin_workspace_id, spec.PROVIDER) orelse {
        hx.fail(ec.ERR_CONNECTOR_NOT_CONFIGURED, S_NOT_CONFIGURED);
        return;
    };
    defer creds.deinit(hx.alloc);

    const st = oauth2.mintState(hx.alloc, hx.ctx.queue, spec.SPEC, secret, workspace_id, clock.nowMillis()) catch {
        common.internalOperationError(hx.res, "Failed to start Slack connect", hx.req_id);
        return;
    };
    defer hx.alloc.free(st);

    const redirect_uri = std.fmt.allocPrint(hx.alloc, "{s}{s}", .{ hx.ctx.api_url, CALLBACK_PATH }) catch {
        common.internalOperationError(hx.res, "Failed to build Slack connect URL", hx.req_id);
        return;
    };
    defer hx.alloc.free(redirect_uri);

    const url = oauth2.authorizeUrl(hx.alloc, spec.SPEC, creds.client_id, redirect_uri, st) catch {
        common.internalOperationError(hx.res, "Failed to build Slack authorize URL", hx.req_id);
        return;
    };
    defer hx.alloc.free(url);

    log.debug("connect_initiated", .{ .workspace_id = workspace_id });
    hx.ok(.ok, .{ .install_url = url });
}
