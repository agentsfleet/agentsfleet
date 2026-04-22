// OAuth callback — none policy. GitHub redirects here with a ?code= query
// param; authentication is completed via the OAuth exchange, not Bearer.
const std = @import("std");
const httpz = @import("httpz");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const secrets = @import("../../../secrets/crypto.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const telemetry_mod = @import("../../../observability/telemetry.zig");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");

const log = std.log.scoped(.http);

pub const Context = common.Context;

pub fn innerGitHubCallback(hx: hx_mod.Hx, req: *httpz.Request) void {
    const qs = req.query() catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "installation_id query param required");
        return;
    };
    const installation_id = qs.get("installation_id") orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "installation_id query param required");
        return;
    };

    const workspace_id = qs.get("state") orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "state query param required");
        return;
    };

    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const conn = hx.ctx.pool.acquire() catch {
        log.err("workspace.db_acquire_fail error_code=UZ-INTERNAL-001 op=github_callback", .{});
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // M11_006: the OAuth `state` must resolve to an existing workspace
    // row. Post-bootstrap-removal we never fabricate a tenant here —
    // the workspace is created via `POST /v1/workspaces` ahead of the
    // GitHub App install, carrying the authenticated user's tenant_id.
    // A missing row means a tampered `state` or a stale install link;
    // reject with 403 rather than invent state.
    const tenant_id = blk: {
        var existing = PgQuery.from(conn.query("SELECT tenant_id FROM workspaces WHERE workspace_id = $1", .{workspace_id}) catch {
            log.err("workspace.tenant_lookup_fail error_code=UZ-INTERNAL-002 workspace_id={s}", .{workspace_id});
            common.internalDbError(hx.res, hx.req_id);
            return;
        });
        defer existing.deinit();
        if (existing.next() catch null) |row| {
            const current_tenant = row.get([]u8, 0) catch {
                common.internalDbError(hx.res, hx.req_id);
                return;
            };
            break :blk hx.alloc.dupe(u8, current_tenant) catch {
                common.internalOperationError(hx.res, "Failed to allocate tenant id", hx.req_id);
                return;
            };
        }
        hx.fail(error_codes.ERR_UNAUTHORIZED, "Unknown workspace in OAuth state");
        return;
    };
    defer hx.alloc.free(tenant_id);
    _ = common.setTenantSessionContext(conn, tenant_id);

    secrets.store(
        hx.alloc,
        conn,
        workspace_id,
        "github_app_installation_id",
        installation_id,
        1,
    ) catch {
        log.err("workspace.store_installation_secret_fail error_code=UZ-INTERNAL-003 workspace_id={s}", .{workspace_id});
        common.internalOperationError(hx.res, "Failed to store installation secret", hx.req_id);
        return;
    };

    log.info("workspace.github_connected workspace_id={s} installation_id={s}", .{ workspace_id, installation_id });
    hx.ctx.telemetry.capture(telemetry_mod.WorkspaceGithubConnected, .{ .workspace_id = workspace_id, .installation_id = installation_id, .request_id = hx.req_id });

    hx.ok(.ok, .{
        .workspace_id = workspace_id,
        .installation_id = installation_id,
        .request_id = hx.req_id,
    });
}
