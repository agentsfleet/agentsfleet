const std = @import("std");
const httpz = @import("httpz");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const logging = @import("log");
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

const log = logging.scoped(.http);

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

/// INSERT workspace row. Billing rolls up to the tenant, so new workspaces
/// inherit the tenant balance — no per-workspace credit provisioning here.
/// `repo_url` is left NULL at creation; binding to a repo is a separate step.
fn insertWorkspaceRow(conn: anytype, workspace_id: []const u8, tenant_id: []const u8, name: ?[]const u8, created_by: ?[]const u8, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO workspaces
        \\  (workspace_id, tenant_id, name, paused, created_by, version, created_at, updated_at)
        \\VALUES ($1, $2, $3, false, $4, 1, $5, $5)
    , .{ workspace_id, tenant_id, name, created_by, now_ms });
}

fn isUniqueViolation(conn: anytype) bool {
    const pg_err = conn.err orelse return false;
    return std.mem.eql(u8, pg_err.code, "23505");
}

/// Insert with caller-supplied name (single attempt) or with a server-generated
/// Heroku-style name (retry on per-tenant unique-violation).
fn insertAndProvision(conn: anytype, hx: hx_mod.Hx, workspace_id: []const u8, tenant_id: []const u8, name_opt: ?[]const u8, now_ms: i64) ?[]const u8 {
    if (name_opt) |name| {
        insertWorkspaceRow(conn, workspace_id, tenant_id, name, hx.principal.user_id, now_ms) catch {
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
        if (insertWorkspaceRow(conn, workspace_id, tenant_id, candidate, hx.principal.user_id, now_ms)) |_| {
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
    };

    // Empty body is allowed — `zombiectl workspace add` with no args POSTs `{}`
    // and lets the server pick a Heroku-style name (parity with signup).
    const body = req.body() orelse "{}";
    const parsed = std.json.parseFromSlice(Req, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON");
        return;
    };
    defer parsed.deinit();

    const name_raw = parsed.value.name orelse "";
    const name_trimmed = std.mem.trim(u8, name_raw, " \t\r\n");
    const name: ?[]const u8 = if (name_trimmed.len == 0) null else name_trimmed;
    // Every authenticated principal MUST carry tenant_id. The
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
        log.err("workspace_db_acquire_fail", .{
            .error_code = error_codes.ERR_INTERNAL_DB_UNAVAILABLE,
            .op = "create_workspace",
        });
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
    const final_name = insertAndProvision(conn, hx, workspace_id, tenant_id, name, now_ms) orelse return;

    log.info("workspace_created", .{
        .workspace_id = workspace_id,
        .tenant_id = tenant_id,
        .name = final_name,
    });
    hx.ctx.telemetry.capture(telemetry_mod.WorkspaceCreated, .{ .distinct_id = hx.principal.user_id orelse "", .workspace_id = workspace_id, .tenant_id = tenant_id, .request_id = hx.req_id });

    hx.ok(.created, .{
        .workspace_id = workspace_id,
        .name = final_name,
        .request_id = hx.req_id,
    });
}

