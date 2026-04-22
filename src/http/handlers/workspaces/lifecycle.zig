const std = @import("std");
const httpz = @import("httpz");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const obs_log = @import("../../../observability/logging.zig");
const telemetry_mod = @import("../../../observability/telemetry.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");

const log = std.log.scoped(.http);

fn generateWorkspaceId(alloc: std.mem.Allocator) ![]const u8 {
    return id_format.generateWorkspaceId(alloc);
}

/// Best-effort tenant existence probe. Returns true when the row is
/// present; returns true on DB error too so we fail open — the FK
/// constraint on the subsequent INSERT is the authoritative gate, this
/// is just a cleaner surface for the common "stale claim" case.
fn tenantExists(conn: anytype, tenant_id: []const u8) bool {
    var q = PgQuery.from(conn.query(
        "SELECT 1 FROM tenants WHERE tenant_id = $1 LIMIT 1",
        .{tenant_id},
    ) catch return true);
    defer q.deinit();
    const row = q.next() catch return true;
    return row != null;
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

/// INSERT workspace row. Billing rolls up to the tenant, so new workspaces
/// inherit the tenant balance — no per-workspace credit provisioning here.
fn insertAndProvision(conn: anytype, hx: hx_mod.Hx, workspace_id: []const u8, tenant_id: []const u8, repo_url: []const u8, default_branch: []const u8, now_ms: i64) bool {
    _ = conn.exec(
        \\INSERT INTO workspaces
        \\  (workspace_id, tenant_id, repo_url, default_branch, paused, created_by, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, false, $5, 1, $6, $6)
    , .{ workspace_id, tenant_id, repo_url, default_branch, hx.principal.user_id, now_ms }) catch {
        common.internalOperationError(hx.res, "Failed to create workspace", hx.req_id);
        return false;
    };
    return true;
}

pub fn innerCreateWorkspace(hx: hx_mod.Hx, req: *httpz.Request) void {
    const Req = struct {
        repo_url: []const u8,
        default_branch: ?[]const u8 = null,
    };

    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(Req, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON");
        return;
    };
    defer parsed.deinit();

    const repo_url = std.mem.trim(u8, parsed.value.repo_url, " \t\r\n");
    if (repo_url.len == 0) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "repo_url is required");
        return;
    }
    const default_branch = normalizeDefaultBranch(parsed.value.default_branch);
    // M11_006: every authenticated principal MUST carry tenant_id. The
    // signup webhook writes it back to Clerk publicMetadata after
    // `bootstrapPersonalAccount`; a null tenant_id here means either an
    // unprovisioned Clerk session (reject — caller should refresh and
    // retry once the webhook lands) or a misconfigured JWT template
    // (reject — operator bug).
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(error_codes.ERR_UNAUTHORIZED, "Missing tenant context on session");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        log.err("workspace.db_acquire_fail error_code=UZ-INTERNAL-001 op=create_workspace", .{});
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    _ = common.setTenantSessionContext(conn, tenant_id);

    // Defensive probe: a JWT claim that names an unknown tenant (stale
    // session, hand-crafted token, replay after tenant deletion) would
    // otherwise 500 on the workspace FK constraint. Converting to a
    // clean 401 keeps the handler predictable for integration tests +
    // the alpha client.
    if (!tenantExists(conn, tenant_id)) {
        hx.fail(error_codes.ERR_UNAUTHORIZED, "Tenant on session does not exist");
        return;
    }

    const workspace_id = generateWorkspaceId(hx.alloc) catch {
        common.internalOperationError(hx.res, "Failed to allocate workspace id", hx.req_id);
        return;
    };
    if (!insertAndProvision(conn, hx, workspace_id, tenant_id, repo_url, default_branch, now_ms)) return;

    const github_app_slug = std.process.getEnvVarOwned(hx.alloc, "GITHUB_APP_SLUG") catch "usezombie";
    const install_url = buildInstallUrl(hx.alloc, github_app_slug, workspace_id) catch {
        common.internalOperationError(hx.res, "Failed to build install URL", hx.req_id);
        return;
    };

    log.info("workspace.created workspace_id={s} tenant_id={s} repo_url={s}", .{ workspace_id, tenant_id, repo_url });
    hx.ctx.telemetry.capture(telemetry_mod.WorkspaceCreated, .{ .distinct_id = hx.principal.user_id orelse "", .workspace_id = workspace_id, .tenant_id = tenant_id, .repo_url = repo_url, .request_id = hx.req_id });

    hx.ok(.created, .{
        .workspace_id = workspace_id,
        .repo_url = repo_url,
        .default_branch = default_branch,
        .install_url = install_url,
        .request_id = hx.req_id,
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
