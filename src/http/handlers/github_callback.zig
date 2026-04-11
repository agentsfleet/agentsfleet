// OAuth callback — does not use hx.authenticated(). GitHub redirects here with
// a ?code= query param; authentication is completed via OAuth exchange, not Bearer.
const std = @import("std");
const httpz = @import("httpz");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const secrets = @import("../../secrets/crypto.zig");
const error_codes = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const workspace_billing = @import("../../state/workspace_billing.zig");
const workspace_credit = @import("../../state/workspace_credit.zig");
const posthog_events = @import("../../observability/posthog_events.zig");
const common = @import("common.zig");

const log = std.log.scoped(.http);

pub const Context = common.Context;

pub fn handleGitHubCallback(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const qs = req.query() catch {
        common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "installation_id query param required", req_id);
        return;
    };
    const installation_id = qs.get("installation_id") orelse {
        common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "installation_id query param required", req_id);
        return;
    };

    const workspace_id = qs.get("state") orelse {
        common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "state query param required", req_id);
        return;
    };

    if (!common.requireUuidV7Id(res, req_id, workspace_id, "workspace_id")) return;

    const conn = ctx.pool.acquire() catch {
        log.err("workspace.db_acquire_fail error_code=UZ-INTERNAL-001 op=github_callback", .{});
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    const tenant_id = blk: {
        var existing = PgQuery.from(conn.query("SELECT tenant_id FROM workspaces WHERE workspace_id = $1", .{workspace_id}) catch {
            log.err("workspace.tenant_lookup_fail error_code=UZ-INTERNAL-002 workspace_id={s}", .{workspace_id});
            common.internalDbError(res, req_id);
            return;
        });
        defer existing.deinit();
        if (existing.next() catch null) |row| {
            const current_tenant = row.get([]u8, 0) catch {
                common.internalDbError(res, req_id);
                return;
            };
            break :blk alloc.dupe(u8, current_tenant) catch {
                common.internalOperationError(res, "Failed to allocate tenant id", req_id);
                return;
            };
        }
        break :blk id_format.generateTenantId(alloc) catch {
            common.internalOperationError(res, "Failed to allocate tenant id", req_id);
            return;
        };
    };
    _ = common.setTenantSessionContext(conn, tenant_id);

    _ = conn.exec(
        \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at, updated_at)
        \\VALUES ($1, 'GitHub App', 'callback', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ tenant_id, now_ms }) catch {
        common.internalOperationError(res, "Failed to upsert tenant", req_id);
        return;
    };

    workspace_billing.enforceFreeWorkspaceCreationAllowed(conn, tenant_id, workspace_id) catch |err| {
        if (workspace_billing.errorCode(err)) |code| {
            common.errorResponse(res, code, workspace_billing.errorMessage(err) orelse "Workspace billing failure", req_id);
            return;
        }
        common.internalOperationError(res, "Failed to validate free workspace limit", req_id);
        return;
    };

    {
        const repo_url = qs.get("repo_url") orelse "https://github.com/unknown/unknown";
        const default_branch = qs.get("default_branch") orelse "main";

        _ = conn.exec(
            \\INSERT INTO workspaces
            \\  (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
            \\VALUES ($1, $2, $3, $4, false, 1, $5, $5)
            \\ON CONFLICT (workspace_id) DO UPDATE
            \\SET tenant_id = EXCLUDED.tenant_id,
            \\    repo_url = EXCLUDED.repo_url,
            \\    default_branch = EXCLUDED.default_branch,
            \\    updated_at = EXCLUDED.updated_at
        , .{ workspace_id, tenant_id, repo_url, default_branch, now_ms }) catch {
            common.internalOperationError(res, "Failed to upsert workspace", req_id);
            return;
        };
    }

    workspace_billing.provisionFreeWorkspace(conn, alloc, workspace_id, "api") catch {
        common.internalOperationError(res, "Failed to provision free entitlement", req_id);
        return;
    };
    workspace_credit.provisionWorkspaceCredit(conn, alloc, workspace_id, "api") catch {
        common.internalOperationError(res, "Failed to provision free credit", req_id);
        return;
    };

    secrets.store(
        alloc,
        conn,
        workspace_id,
        "github_app_installation_id",
        installation_id,
        1,
    ) catch {
        log.err("workspace.store_installation_secret_fail error_code=UZ-INTERNAL-003 workspace_id={s}", .{workspace_id});
        common.internalOperationError(res, "Failed to store installation secret", req_id);
        return;
    };

    log.info("workspace.github_connected workspace_id={s} installation_id={s}", .{ workspace_id, installation_id });
    posthog_events.trackWorkspaceGithubConnected(ctx.posthog, workspace_id, installation_id, req_id);

    common.writeJson(res, .ok, .{
        .workspace_id = workspace_id,
        .installation_id = installation_id,
        .request_id = req_id,
    });
}
