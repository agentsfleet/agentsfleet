//! GET /v1/workspaces/{ws}/events — workspace-aggregate event history.
//!
//! Replaces the deleted `workspaces/activity.zig`. Same listing shape
//! as the per-zombie endpoint, scoped to workspace, with optional
//! `zombie_id` filter for drill-down. Reads `core.zombie_events`
//! filtered by `workspace_id` (RLS-protected via the tenant context
//! set by `authorizeWorkspaceAndSetTenantContext`).

const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const events_store = @import("../../../state/zombie_events_store.zig");

const log = logging.scoped(.http_workspace_events);

const LIMIT_DEFAULT: u32 = 50;
const LIMIT_MAX: u32 = 200;

pub fn innerListWorkspaceEvents(
    hx: hx_mod.Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }

    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "malformed query string");
        return;
    };

    const params = parseFilterParams(hx, qs) catch return;
    if (params.zombie_id) |zid| {
        if (!id_format.isSupportedWorkspaceId(zid)) {
            hx.fail(ec.ERR_INVALID_REQUEST, "zombie_id must be a UUIDv7");
            return;
        }
    }

    const filter = buildFilter(hx, params) catch return;
    defer if (filter.actor_like) |al| hx.alloc.free(al);

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const rows = events_store.listForWorkspace(conn, hx.alloc, workspace_id, params.zombie_id, filter) catch |err| {
        if (err == error.InvalidCursor) {
            hx.fail(ec.ERR_INVALID_REQUEST, "invalid cursor");
            return;
        }
        log.err("list_for_workspace_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err) });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    writeListResponse(hx, rows, filter.limit);
}

const FilterParams = struct {
    limit: u32,
    cursor: ?[]const u8,
    actor: ?[]const u8,
    since_raw: ?[]const u8,
    zombie_id: ?[]const u8,
};

fn parseFilterParams(hx: hx_mod.Hx, qs: anytype) error{Failed}!FilterParams {
    const limit = parseLimit(qs) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "limit must be between 1 and 200");
        return error.Failed;
    };
    const cursor = qs.get("cursor");
    const actor = qs.get("actor");
    const since_raw = qs.get("since");
    const zombie_id = qs.get("zombie_id");

    if (cursor != null and since_raw != null) {
        hx.fail(ec.ERR_INVALID_REQUEST, "since_and_cursor_mutually_exclusive");
        return error.Failed;
    }

    return .{
        .limit = limit,
        .cursor = cursor,
        .actor = actor,
        .since_raw = since_raw,
        .zombie_id = zombie_id,
    };
}

fn buildFilter(hx: hx_mod.Hx, params: FilterParams) error{Failed}!events_store.Filter {
    var since_ms: ?i64 = null;
    if (params.since_raw) |raw| {
        since_ms = events_store.parseSince(raw, std.time.milliTimestamp()) catch {
            hx.fail(ec.ERR_INVALID_REQUEST, "invalid_since_format: use Go-style duration (15s, 30m, 2h, 7d) or RFC 3339 (YYYY-MM-DDTHH:MM:SSZ)");
            return error.Failed;
        };
    }

    var actor_like: ?[]u8 = null;
    if (params.actor) |a| {
        actor_like = events_store.globToLike(hx.alloc, a) catch {
            common.internalDbError(hx.res, hx.req_id);
            return error.Failed;
        };
    }

    return events_store.Filter{
        .limit = params.limit,
        .cursor = params.cursor,
        .actor_like = actor_like,
        .since_ms = since_ms,
    };
}

fn writeListResponse(hx: hx_mod.Hx, rows: []events_store.EventRow, limit: u32) void {
    const next_cursor: ?[]u8 = if (rows.len == limit and rows.len > 0) blk: {
        const last = rows[rows.len - 1];
        break :blk events_store.makeCursor(hx.alloc, last.created_at, last.event_id) catch {
            common.internalDbError(hx.res, hx.req_id);
            return;
        };
    } else null;

    hx.ok(.ok, .{ .items = rows, .next_cursor = next_cursor });
}

fn parseLimit(qs: anytype) !u32 {
    const raw = qs.get("limit") orelse return LIMIT_DEFAULT;
    const n = std.fmt.parseInt(u32, raw, 10) catch return error.InvalidLimit;
    if (n == 0 or n > LIMIT_MAX) return error.InvalidLimit;
    return n;
}
