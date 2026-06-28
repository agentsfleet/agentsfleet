//! GET /v1/connectors/github/callback?installation_id=&state= — Bearer-less.
//!
//! GitHub redirects the operator's browser here after the App install. The
//! signed `state` is the only trust anchor (no Bearer on a cross-site redirect):
//! verify + single-use consume yields the bound workspace. On success write the
//! `fleet:github` vault handle the broker mints from, then 302 back to the
//! dashboard. No token is ever handled — only the installation id.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");
const clock = @import("common").clock;
const common = @import("../../common.zig");
const hx_mod = @import("../../hx.zig");
const ec = @import("../../../../errors/error_registry.zig");
const vault = @import("../../../../state/vault.zig");
const credential_key = @import("../../../../fleet_runtime/credential_key.zig");
const state_mod = @import("state.zig");

const log = logging.scoped(.connector_github);

const INTEGRATION_GITHUB = "github";
const Q_INSTALLATION_ID = "installation_id";
const Q_STATE = "state";
// The exact vault-handle shape the broker reads (integration_github.zig).
const HANDLE_FMT = "{{\"integration\":\"github\",\"installation_id\":\"{s}\"}}";
const DEST_PATH = "/credentials?connector=github";
const HEADER_LOCATION = "location";
const STATUS_FOUND: u16 = 302;
const MAX_INSTALLATION_ID_LEN: usize = 32;

pub fn innerGithubCallback(hx: hx_mod.Hx, req: *httpz.Request) void {
    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Bad query string");
        return;
    };
    const raw_state = qs.get(Q_STATE) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Missing state");
        return;
    };
    const installation_id = qs.get(Q_INSTALLATION_ID) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Missing installation_id");
        return;
    };
    if (!isNumericId(installation_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed installation_id");
        return;
    }

    const secret = hx.ctx.approval_signing_secret orelse {
        hx.fail(ec.ERR_CONNECTOR_NOT_CONFIGURED, "GitHub connect is not configured");
        return;
    };

    const workspace_id = state_mod.verifyConsume(
        hx.alloc,
        hx.ctx.queue,
        secret,
        raw_state,
        clock.nowMillis(),
    ) orelse {
        hx.fail(ec.ERR_CONNECTOR_STATE_INVALID, "Invalid or expired connect state");
        return;
    };
    defer hx.alloc.free(workspace_id);

    storeHandle(hx, workspace_id, installation_id) catch {
        common.internalOperationError(hx.res, "Failed to store GitHub connection", hx.req_id);
        return;
    };

    log.info("github_connected", .{ .workspace_id = workspace_id });
    redirectToDashboard(hx);
}

fn storeHandle(hx: hx_mod.Hx, workspace_id: []const u8, installation_id: []const u8) !void {
    const conn: *pg.Conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
    defer hx.ctx.pool.release(conn);

    const key = try credential_key.allocKeyName(hx.alloc, INTEGRATION_GITHUB);
    defer hx.alloc.free(key);
    const handle = try std.fmt.allocPrint(hx.alloc, HANDLE_FMT, .{installation_id});
    defer hx.alloc.free(handle);

    try vault.storeJsonPlaintext(hx.alloc, conn, workspace_id, key, handle);
}

fn redirectToDashboard(hx: hx_mod.Hx) void {
    // url lives on the request arena (hx.alloc) — owned for the response write.
    const url = std.fmt.allocPrint(hx.alloc, "{s}{s}", .{ hx.ctx.app_url, DEST_PATH }) catch {
        // The connection succeeded; a redirect-build failure is cosmetic, so
        // return 200 rather than a 500 over a missing app_url.
        hx.ok(.ok, .{ .status = "connected" });
        return;
    };
    hx.res.status = STATUS_FOUND;
    hx.res.header(HEADER_LOCATION, url);
    hx.res.body = "";
}

fn isNumericId(s: []const u8) bool {
    if (s.len == 0 or s.len > MAX_INSTALLATION_ID_LEN) return false;
    for (s) |c| if (c < '0' or c > '9') return false;
    return true;
}
