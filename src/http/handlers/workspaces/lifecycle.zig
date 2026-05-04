const std = @import("std");
const httpz = @import("httpz");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const obs_log = @import("../../../observability/logging.zig");
const telemetry_mod = @import("../../../observability/telemetry.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const heroku_names = @import("../../../state/heroku_names.zig");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");

/// Cap on Heroku-name retries when the caller didn't supply one. The
/// per-tenant unique partial index on `name` is the only collision source;
/// a single tenant can't realistically race against itself fast enough to
/// burn through this many random `<adj>-<noun>-<3digit>` candidates.
const MAX_NAME_ATTEMPTS: u8 = 8;

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

/// INSERT workspace row. Billing rolls up to the tenant, so new workspaces
/// inherit the tenant balance — no per-workspace credit provisioning here.
fn insertWorkspaceRow(conn: anytype, workspace_id: []const u8, tenant_id: []const u8, name: ?[]const u8, repo_url: []const u8, default_branch: []const u8, created_by: ?[]const u8, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO workspaces
        \\  (workspace_id, tenant_id, name, repo_url, default_branch, paused, created_by, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, false, $6, 1, $7, $7)
    , .{ workspace_id, tenant_id, name, repo_url, default_branch, created_by, now_ms });
}

fn isUniqueViolation(conn: anytype) bool {
    const pg_err = conn.err orelse return false;
    return std.mem.eql(u8, pg_err.code, "23505");
}

/// Insert with caller-supplied name (single attempt) or with a server-generated
/// Heroku-style name (retry on per-tenant unique-violation).
fn insertAndProvision(conn: anytype, hx: hx_mod.Hx, workspace_id: []const u8, tenant_id: []const u8, name_opt: ?[]const u8, repo_url: []const u8, default_branch: []const u8, now_ms: i64) ?[]const u8 {
    if (name_opt) |name| {
        insertWorkspaceRow(conn, workspace_id, tenant_id, name, repo_url, default_branch, hx.principal.user_id, now_ms) catch {
            common.internalOperationError(hx.res, "Failed to create workspace", hx.req_id);
            return null;
        };
        return name;
    }

    var attempt: u8 = 0;
    while (attempt < MAX_NAME_ATTEMPTS) : (attempt += 1) {
        const candidate = heroku_names.generate(hx.alloc) catch {
            common.internalOperationError(hx.res, "Failed to generate workspace name", hx.req_id);
            return null;
        };
        if (insertWorkspaceRow(conn, workspace_id, tenant_id, candidate, repo_url, default_branch, hx.principal.user_id, now_ms)) |_| {
            return candidate;
        } else |err| {
            hx.alloc.free(candidate);
            if (err == error.PG and isUniqueViolation(conn)) continue;
            common.internalOperationError(hx.res, "Failed to create workspace", hx.req_id);
            return null;
        }
    }
    common.internalOperationError(hx.res, "Workspace name generator exhausted retries", hx.req_id);
    return null;
}

pub fn innerCreateWorkspace(hx: hx_mod.Hx, req: *httpz.Request) void {
    const Req = struct {
        name: ?[]const u8 = null,
        repo_url: ?[]const u8 = null,
        default_branch: ?[]const u8 = null,
    };

    // Empty body is allowed — `zombiectl workspace add` with no args POSTs `{}`
    // and lets the server pick a Heroku-style name (parity with signup).
    const body = req.body() orelse "{}";
    const parsed = std.json.parseFromSlice(Req, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON");
        return;
    };
    defer parsed.deinit();

    const repo_url_raw = parsed.value.repo_url orelse "";
    const repo_url = std.mem.trim(u8, repo_url_raw, " \t\r\n");
    const name_raw = parsed.value.name orelse "";
    const name_trimmed = std.mem.trim(u8, name_raw, " \t\r\n");
    const name: ?[]const u8 = if (name_trimmed.len == 0) null else name_trimmed;
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
    const final_name = insertAndProvision(conn, hx, workspace_id, tenant_id, name, repo_url, default_branch, now_ms) orelse return;

    log.info("workspace.created workspace_id={s} tenant_id={s} name={s}", .{ workspace_id, tenant_id, final_name });
    hx.ctx.telemetry.capture(telemetry_mod.WorkspaceCreated, .{ .distinct_id = hx.principal.user_id orelse "", .workspace_id = workspace_id, .tenant_id = tenant_id, .repo_url = repo_url, .request_id = hx.req_id });

    hx.ok(.created, .{
        .workspace_id = workspace_id,
        .name = final_name,
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
