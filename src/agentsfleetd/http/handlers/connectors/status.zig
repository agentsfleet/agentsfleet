//! GET /v1/workspaces/{ws}/connectors/{provider} — workspace-authed.
//!
//! The generic status handler: resolves `{provider}` against the registry
//! (unknown → 404 naming it), loads the workspace's `<provider>` vault
//! handle, and hands the parsed object (null when missing/unreadable) to the
//! provider's `respond_status` hook, which owns the body shape. Never
//! fabricates a connected state — every load failure reads as not connected.

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const vault = @import("../../../state/vault.zig");
const matchers = @import("../../route_matchers_connectors.zig");
const registry = @import("registry.zig");

const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";

pub fn innerStatus(hx: hx_mod.Hx, route: matchers.WorkspaceConnectorRoute) void {
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

    var parsed = vault.loadJson(hx.alloc, conn, route.workspace_id, spec.provider) catch return spec.respond_status(hx, null);
    defer parsed.deinit();
    spec.respond_status(hx, switch (parsed.value) {
        .object => |o| o,
        else => null,
    });
}
