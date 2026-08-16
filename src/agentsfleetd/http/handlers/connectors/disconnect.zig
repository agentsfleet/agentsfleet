//! DELETE /v1/workspaces/{ws}/connectors/{provider} — workspace-authed.
//!
//! Removes agentsfleet's vaulted handle and reverse-routing rows. It never
//! revokes provider authorization or uninstalls an external app, so a user can
//! safely retry Connect after internal/external state drift.

const pg = @import("pg");
const logging = @import("log");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const vault = @import("../../../state/vault.zig");
const matchers = @import("../../route_matchers_connectors.zig");
const BindingTxn = @import("binding_tx.zig");
const registry = @import("registry.zig");
const sql = @import("sql.zig");

const log = logging.scoped(.connectors);
const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";

/// Remove a workspace connector idempotently. Provider-side authorization is
/// outside this resource and remains unchanged.
pub fn innerDisconnect(hx: hx_mod.Hx, route: matchers.WorkspaceConnectorRoute) void {
    const spec = registry.lookup(route.provider) orelse return registry.respondUnknown(hx, route.provider);
    const conn: *pg.Conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, route.workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    disconnectOnConn(conn, spec.provider, route.workspace_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    log.info("connector_disconnected", .{ .provider = spec.provider, .workspace_id = route.workspace_id });
    hx.noContent();
}

fn disconnectOnConn(conn: *pg.Conn, provider: []const u8, workspace_id: []const u8) !void {
    var txn = try BindingTxn.begin(conn, provider, workspace_id);
    defer txn.abort();
    _ = try conn.exec(sql.DELETE_WORKSPACE_INSTALLS, .{ provider, workspace_id });
    _ = try vault.deleteCredential(conn, workspace_id, provider);
    try txn.commit();
}
