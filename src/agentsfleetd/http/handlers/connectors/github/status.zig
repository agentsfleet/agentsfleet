//! GET /v1/workspaces/{ws}/connectors/github — workspace-authed.
//!
//! Reports the GitHub connector state for the workspace: "connected" when the
//! `fleet:github` vault handle exists, else "not_connected". Never fabricates a
//! connected state — a missing/unreadable handle reads as not connected.
//! (reconnect_required is surfaced by mint failures at use time, not here.)

const pg = @import("pg");
const common = @import("../../common.zig");
const hx_mod = @import("../../hx.zig");
const ec = @import("../../../../errors/error_registry.zig");
const vault = @import("../../../../state/vault.zig");
const credential_key = @import("../../../../fleet_runtime/credential_key.zig");

const INTEGRATION_GITHUB = "github";
const FIELD_INSTALLATION_ID = "installation_id";
const STATUS_CONNECTED = "connected";
const STATUS_NOT_CONNECTED = "not_connected";
const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";

pub fn innerGithubStatus(hx: hx_mod.Hx, workspace_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    const status = if (handleExists(hx, conn, workspace_id)) STATUS_CONNECTED else STATUS_NOT_CONNECTED;
    hx.ok(.ok, .{ .status = status });
}

fn handleExists(hx: hx_mod.Hx, conn: *pg.Conn, workspace_id: []const u8) bool {
    const key = credential_key.allocKeyName(hx.alloc, INTEGRATION_GITHUB) catch return false;
    defer hx.alloc.free(key);
    var parsed = vault.loadJson(hx.alloc, conn, workspace_id, key) catch return false;
    defer parsed.deinit();
    return parsed.value == .object and parsed.value.object.get(FIELD_INSTALLATION_ID) != null;
}
