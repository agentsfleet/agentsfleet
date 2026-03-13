const std = @import("std");
const zap = @import("zap");
const policy = @import("../../state/policy.zig");
const workspace_billing = @import("../../state/workspace_billing.zig");
const obs_log = @import("../../observability/logging.zig");
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

pub fn handleUpgradeWorkspaceToScale(ctx: *common.Context, r: zap.Request, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const Req = struct {
        subscription_id: []const u8,
    };

    const body = r.body orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const upgraded = workspace_billing.upgradeWorkspaceToScale(conn, alloc, workspace_id, .{
        .subscription_id = parsed.value.subscription_id,
        .actor = principal.user_id orelse "api",
    }) catch |err| switch (err) {
        error.InvalidSubscriptionId => {
            common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "subscription_id is required", req_id);
            return;
        },
        else => {
            common.internalOperationError(r, "Failed to upgrade workspace to Scale", req_id);
            return;
        },
    };
    defer alloc.free(upgraded.plan_sku);
    defer if (upgraded.subscription_id) |v| alloc.free(v);

    common.writeJson(r, .ok, .{
        .workspace_id = workspace_id,
        .plan_tier = upgraded.plan_tier.label(),
        .billing_status = upgraded.billing_status.label(),
        .plan_sku = upgraded.plan_sku,
        .subscription_id = upgraded.subscription_id,
        .request_id = req_id,
    });
}

pub fn handleCreateWorkspace(ctx: *common.Context, r: zap.Request) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };

    const Req = struct {
        repo_url: []const u8,
        default_branch: ?[]const u8 = null,
    };

    const body = r.body orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const repo_url = std.mem.trim(u8, parsed.value.repo_url, " \t\r\n");
    if (repo_url.len == 0) {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "repo_url is required", req_id);
        return;
    }
    const default_branch = normalizeDefaultBranch(parsed.value.default_branch);
    const tenant_id = principal.tenant_id orelse id_format.generateTenantId(alloc) catch {
        common.internalOperationError(r, "Failed to allocate tenant id", req_id);
        return;
    };

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    _ = common.setTenantSessionContext(conn, tenant_id);
    var tenant_q = conn.query(
        \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at)
        \\VALUES ($1, $2, 'managed', $3)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ tenant_id, "Workspace Tenant", now_ms }) catch {
        common.internalOperationError(r, "Failed to upsert tenant", req_id);
        return;
    };
    tenant_q.deinit();

    const workspace_id = generateWorkspaceId(alloc) catch {
        common.internalOperationError(r, "Failed to allocate workspace id", req_id);
        return;
    };

    var ws_q = conn.query(
        \\INSERT INTO workspaces
        \\  (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, false, 1, $5, $5)
    , .{ workspace_id, tenant_id, repo_url, default_branch, now_ms }) catch {
        common.internalOperationError(r, "Failed to create workspace", req_id);
        return;
    };
    ws_q.deinit();

    workspace_billing.provisionFreeWorkspace(conn, alloc, workspace_id, "api") catch {
        common.internalOperationError(r, "Failed to provision free entitlement", req_id);
        return;
    };

    const github_app_slug = std.process.getEnvVarOwned(alloc, "GITHUB_APP_SLUG") catch "usezombie";
    const install_url = buildInstallUrl(alloc, github_app_slug, workspace_id) catch {
        common.internalOperationError(r, "Failed to build install URL", req_id);
        return;
    };

    common.writeJson(r, .created, .{
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

pub fn handlePauseWorkspace(ctx: *common.Context, r: zap.Request, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const Req = struct {
        pause: bool,
        reason: []const u8,
        version: i64,
    };

    const body = r.body orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    policy.recordPolicyEvent(conn, workspace_id, null, .sensitive, .allow, "m1.pause_workspace", "api") catch |err| {
        obs_log.logWarnErr(.http, err, "policy event insert failed (non-fatal) workspace_id={s}", .{workspace_id});
    };

    const now_ms = std.time.milliTimestamp();
    var upd = conn.query(
        \\UPDATE workspaces
        \\SET paused = $1, paused_reason = $2, version = version + 1, updated_at = $3
        \\WHERE workspace_id = $4 AND version = $5
        \\RETURNING version
    , .{
        parsed.value.pause,
        if (parsed.value.pause) parsed.value.reason else null,
        now_ms,
        workspace_id,
        parsed.value.version,
    }) catch {
        common.internalDbError(r, req_id);
        return;
    };
    defer upd.deinit();

    const row = upd.next() catch null orelse {
        common.errorResponse(r, .conflict, error_codes.ERR_WORKSPACE_NOT_FOUND, "Workspace not found or version conflict", req_id);
        return;
    };

    const new_version = row.get(i64, 0) catch 0;
    log.info("workspace {} pause={} workspace_id={s}", .{ parsed.value.pause, parsed.value.pause, workspace_id });

    common.writeJson(r, .ok, .{
        .workspace_id = workspace_id,
        .paused = parsed.value.pause,
        .version = new_version,
        .request_id = req_id,
    });
}

pub fn handleSyncSpecs(ctx: *common.Context, r: zap.Request, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const billing_state = workspace_billing.reconcileWorkspaceBilling(conn, alloc, workspace_id, std.time.milliTimestamp(), "api") catch {
        common.internalOperationError(r, "Failed to reconcile workspace billing state", req_id);
        return;
    };
    defer alloc.free(billing_state.plan_sku);
    defer if (billing_state.subscription_id) |v| alloc.free(v);

    var ws = conn.query(
        "SELECT repo_url, default_branch FROM workspaces WHERE workspace_id = $1",
        .{workspace_id},
    ) catch {
        common.internalDbError(r, req_id);
        return;
    };
    defer ws.deinit();

    const ws_row = ws.next() catch null orelse {
        common.errorResponse(r, .not_found, error_codes.ERR_WORKSPACE_NOT_FOUND, "Workspace not found", req_id);
        return;
    };

    const repo_url = ws_row.get([]u8, 0) catch "";
    _ = repo_url;

    var count_result = conn.query(
        "SELECT COUNT(*) FROM specs WHERE workspace_id = $1 AND status = 'pending'",
        .{workspace_id},
    ) catch {
        common.internalDbError(r, req_id);
        return;
    };
    defer count_result.deinit();

    const total_pending: i64 = if (count_result.next() catch null) |crow|
        crow.get(i64, 0) catch 0
    else
        0;

    log.info("sync workspace_id={s} total_pending={d}", .{ workspace_id, total_pending });

    common.writeJson(r, .ok, .{
        .synced_count = @as(i64, 0),
        .total_pending = total_pending,
        .specs = &[_]u8{},
        .plan_tier = billing_state.plan_tier.label(),
        .billing_status = billing_state.billing_status.label(),
        .plan_sku = billing_state.plan_sku,
        .grace_expires_at = billing_state.grace_expires_at,
        .request_id = req_id,
    });
}
