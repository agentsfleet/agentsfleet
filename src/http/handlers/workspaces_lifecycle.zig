const std = @import("std");
const httpz = @import("httpz");
const workspace_billing = @import("../../state/workspace_billing.zig");
const workspace_credit = @import("../../state/workspace_credit.zig");
const obs_log = @import("../../observability/logging.zig");
const posthog_events = @import("../../observability/posthog_events.zig");
const error_codes = @import("../../errors/codes.zig");
const id_format = @import("../../types/id_format.zig");
const common = @import("common.zig");

const log = std.log.scoped(.http);

fn generateWorkspaceId(alloc: std.mem.Allocator) ![]const u8 {
    return id_format.generateWorkspaceId(alloc);
}

fn normalizeDefaultBranch(default_branch: ?[]const u8) []const u8 {
    const raw = default_branch orelse return "main";
    const trimmed = std.mem.trim(u8, raw, " \t\r\n");
    if (trimmed.len == 0) return "main";
    return trimmed;
}

fn buildInstallUrl(alloc: std.mem.Allocator, app_slug: []const u8, workspace_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(
        alloc,
        "https://github.com/apps/{s}/installations/new?state={s}",
        .{ app_slug, workspace_id },
    );
}

pub fn handleCreateWorkspace(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthErrorWithTracking(res, req_id, err, ctx.posthog);
        return;
    };

    const Req = struct {
        repo_url: []const u8,
        default_branch: ?[]const u8 = null,
    };

    const body = req.body() orelse {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const repo_url = std.mem.trim(u8, parsed.value.repo_url, " \t\r\n");
    if (repo_url.len == 0) {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "repo_url is required", req_id);
        return;
    }
    const default_branch = normalizeDefaultBranch(parsed.value.default_branch);
    const tenant_id = principal.tenant_id orelse id_format.generateTenantId(alloc) catch {
        common.internalOperationError(res, "Failed to allocate tenant id", req_id);
        return;
    };

    const conn = ctx.pool.acquire() catch {
        log.err("workspace.db_acquire_fail error_code=UZ-INTERNAL-001 op=create_workspace", .{});
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    _ = common.setTenantSessionContext(conn, tenant_id);
    _ = conn.exec(
        \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at)
        \\VALUES ($1, $2, 'managed', $3)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ tenant_id, "Workspace Tenant", now_ms }) catch {
        log.err("workspace.tenant_upsert_fail error_code=UZ-INTERNAL-003 tenant_id={s}", .{tenant_id});
        common.internalOperationError(res, "Failed to upsert tenant", req_id);
        return;
    };

    workspace_billing.enforceFreeWorkspaceCreationAllowed(conn, tenant_id, null) catch |err| {
        if (workspace_billing.errorCode(err)) |code| {
            log.err("workspace.billing_enforcement_fail tenant_id={s} error_code={s}", .{ tenant_id, code });
            posthog_events.trackApiError(ctx.posthog, principal.user_id orelse "", code, workspace_billing.errorMessage(err) orelse "Workspace billing failure", req_id);
            common.errorResponse(res, .forbidden, code, workspace_billing.errorMessage(err) orelse "Workspace billing failure", req_id);
            return;
        }
        log.err("workspace.billing_validation_fail error_code=UZ-INTERNAL-003 tenant_id={s}", .{tenant_id});
        common.internalOperationError(res, "Failed to validate free workspace limit", req_id);
        return;
    };

    const workspace_id = generateWorkspaceId(alloc) catch {
        common.internalOperationError(res, "Failed to allocate workspace id", req_id);
        return;
    };

    _ = conn.exec(
        \\INSERT INTO workspaces
        \\  (workspace_id, tenant_id, repo_url, default_branch, paused, created_by, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, false, $5, 1, $6, $6)
    , .{ workspace_id, tenant_id, repo_url, default_branch, principal.user_id, now_ms }) catch {
        common.internalOperationError(res, "Failed to create workspace", req_id);
        return;
    };

    workspace_billing.provisionFreeWorkspace(conn, alloc, workspace_id, "api") catch {
        common.internalOperationError(res, "Failed to provision free entitlement", req_id);
        return;
    };
    workspace_credit.provisionWorkspaceCredit(conn, alloc, workspace_id, "api") catch {
        common.internalOperationError(res, "Failed to provision free credit", req_id);
        return;
    };

    const github_app_slug = std.process.getEnvVarOwned(alloc, "GITHUB_APP_SLUG") catch "usezombie";
    const install_url = buildInstallUrl(alloc, github_app_slug, workspace_id) catch {
        common.internalOperationError(res, "Failed to build install URL", req_id);
        return;
    };

    log.info("workspace.created workspace_id={s} tenant_id={s} repo_url={s}", .{ workspace_id, tenant_id, repo_url });
    posthog_events.trackWorkspaceCreated(ctx.posthog, principal.user_id orelse "", workspace_id, tenant_id, repo_url, req_id);

    common.writeJson(res, .created, .{
        .workspace_id = workspace_id,
        .repo_url = repo_url,
        .default_branch = default_branch,
        .install_url = install_url,
        .request_id = req_id,
    });
}

test "normalizeDefaultBranch falls back to main for null/blank input" {
    try std.testing.expectEqualStrings("main", normalizeDefaultBranch(null));
    try std.testing.expectEqualStrings("main", normalizeDefaultBranch(""));
    try std.testing.expectEqualStrings("main", normalizeDefaultBranch("   "));
}

test "normalizeDefaultBranch trims provided value" {
    try std.testing.expectEqualStrings("trunk", normalizeDefaultBranch("  trunk\t"));
}

test "buildInstallUrl renders GitHub app install URL" {
    const alloc = std.testing.allocator;
    const url = try buildInstallUrl(alloc, "usezombie", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21");
    defer alloc.free(url);
    try std.testing.expectEqualStrings(
        "https://github.com/apps/usezombie/installations/new?state=0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21",
        url,
    );
}
