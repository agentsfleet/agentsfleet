//! GET /v1/workspaces/{ws}/fleets — paginated list of fleets in a workspace.
//!
//! Composite keyset pagination: cursor encodes (created_at_ms, id) so multiple
//! fleets installed on the same millisecond are not silently skipped. See
//! `src/agentsfleetd/fleet_runtime/keyset_cursor.zig` for the cursor format.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const keyset_cursor = @import("../../../fleet_runtime/keyset_cursor.zig");

const log = logging.scoped(.fleet_api);

const Hx = hx_mod.Hx;

const DEFAULT_LIST_PAGE_LIMIT: u32 = 20;
const MAX_LIST_PAGE_LIMIT: u32 = 100;

pub fn innerListFleets(hx: Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }

    const qs = req.query() catch null;
    const limit = if (qs) |q| parseLimitFromQs(q) else DEFAULT_LIST_PAGE_LIMIT;
    const cursor = if (qs) |q| q.get("cursor") else null;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const page = fetchFleetPageOnConn(conn, hx.alloc, workspace_id, cursor, limit) catch |err| {
        if (err == error.InvalidCursor) {
            hx.fail(ec.ERR_INVALID_REQUEST, "Invalid cursor format");
            return;
        }
        log.err("list_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer {
        for (page.rows) |r| {
            hx.alloc.free(r.id);
            hx.alloc.free(r.name);
            hx.alloc.free(r.status);
            if (r.triggers_raw) |t| hx.alloc.free(t);
        }
        hx.alloc.free(page.rows);
        if (page.next_cursor) |nc| hx.alloc.free(nc);
    }

    writeListResponse(hx, page);
}

fn writeListResponse(hx: Hx, page: FleetPage) void {
    // Per-row Parsed handles outlive the response emit; freed when this scope exits.
    var parsed_triggers = hx.alloc.alloc(?std.json.Parsed(std.json.Value), page.rows.len) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer {
        for (parsed_triggers) |maybe_p| if (maybe_p) |p| p.deinit();
        hx.alloc.free(parsed_triggers);
    }

    var items = hx.alloc.alloc(FleetListItem, page.rows.len) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer hx.alloc.free(items);

    for (page.rows, 0..) |row, i| {
        parsed_triggers[i] = if (row.triggers_raw) |raw|
            std.json.parseFromSlice(std.json.Value, hx.alloc, raw, .{}) catch null
        else
            null;
        items[i] = .{
            .id = row.id,
            .name = row.name,
            .status = row.status,
            .created_at = row.created_at,
            .updated_at = row.updated_at,
            .triggers = if (parsed_triggers[i]) |p| p.value else null,
            .events_processed = row.events_processed,
            .budget_used_nanos = row.budget_used_nanos,
        };
    }

    hx.ok(.ok, .{ .items = items, .total = items.len, .cursor = page.next_cursor });
}

/// Wire shape per row — `triggers` projects `config_json->'x-agentsfleet'->'triggers'`
/// from the stored config so consumers can render a per-trigger card without a
/// follow-up fetch. `null` means the column has no triggers entry yet (legacy
/// rows pre-§1 reshape — should not exist post-v2.0 but the field stays optional).
const FleetListItem = struct {
    id: []const u8,
    name: []const u8,
    status: []const u8,
    created_at: i64,
    updated_at: i64,
    triggers: ?std.json.Value,
    events_processed: i64,
    budget_used_nanos: i64,
};

fn parseLimitFromQs(qs: anytype) u32 {
    const limit_str = qs.get("limit") orelse return DEFAULT_LIST_PAGE_LIMIT;
    const parsed = std.fmt.parseInt(u32, limit_str, 10) catch return DEFAULT_LIST_PAGE_LIMIT;
    // Treat limit=0 as the default. With LIMIT 0 the cursor guard reports
    // no more pages, so callers would stop paginating even when rows exist.
    return if (parsed == 0) DEFAULT_LIST_PAGE_LIMIT else parsed;
}

const FleetListRow = struct {
    id: []const u8,
    name: []const u8,
    status: []const u8,
    created_at: i64,
    updated_at: i64,
    /// Raw JSONB projected from `config_json->'x-agentsfleet'->'triggers'`. `null`
    /// when the column has no entry (should not occur post-§1 reshape).
    triggers_raw: ?[]const u8 = null,
    /// Count of `core.fleet_events` rows for this fleet (lifetime).
    events_processed: i64 = 0,
    /// Sum of `credit_deducted_nanos` across this fleet's telemetry rows.
    budget_used_nanos: i64 = 0,
};

const FleetPage = struct {
    rows: []FleetListRow,
    next_cursor: ?[]const u8,
};

fn fetchFleetPageOnConn(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    cursor: ?[]const u8,
    limit: u32,
) !FleetPage {
    const page_limit = @min(limit, MAX_LIST_PAGE_LIMIT);
    return if (cursor) |c|
        fetchFleetPageAfter(conn, alloc, workspace_id, c, page_limit)
    else
        fetchFleetPageFirst(conn, alloc, workspace_id, page_limit);
}

fn fetchFleetPageFirst(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    limit: u32,
) !FleetPage {
    var q = PgQuery.from(try conn.query(
        \\SELECT id::text, name, status, created_at, updated_at,
        \\       (config_json->'x-agentsfleet'->'triggers')::text,
        \\       (SELECT COUNT(*) FROM core.fleet_events ev WHERE ev.fleet_id = core.fleets.id)::bigint,
        \\       (SELECT COALESCE(SUM(te.credit_deducted_nanos), 0)::bigint
        \\          FROM core.fleet_execution_telemetry te WHERE te.fleet_id = core.fleets.id::text)
        \\FROM core.fleets
        \\WHERE workspace_id = $1::uuid
        \\ORDER BY created_at DESC, id DESC
        \\LIMIT $2
    , .{ workspace_id, @as(i64, @intCast(limit)) }));
    defer q.deinit();
    return collectFleetPage(alloc, &q, limit);
}

fn fetchFleetPageAfter(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    cursor: []const u8,
    limit: u32,
) !FleetPage {
    const parsed = keyset_cursor.parse(cursor) catch return error.InvalidCursor;

    var q = PgQuery.from(try conn.query(
        \\SELECT id::text, name, status, created_at, updated_at,
        \\       (config_json->'x-agentsfleet'->'triggers')::text,
        \\       (SELECT COUNT(*) FROM core.fleet_events ev WHERE ev.fleet_id = core.fleets.id)::bigint,
        \\       (SELECT COALESCE(SUM(te.credit_deducted_nanos), 0)::bigint
        \\          FROM core.fleet_execution_telemetry te WHERE te.fleet_id = core.fleets.id::text)
        \\FROM core.fleets
        \\WHERE workspace_id = $1::uuid
        \\  AND (created_at < $2 OR (created_at = $2 AND id::text < $3))
        \\ORDER BY created_at DESC, id DESC
        \\LIMIT $4
    , .{ workspace_id, parsed.created_at_ms, parsed.id, @as(i64, @intCast(limit)) }));
    defer q.deinit();
    return collectFleetPage(alloc, &q, limit);
}

fn collectFleetPage(alloc: std.mem.Allocator, q: *PgQuery, limit: u32) !FleetPage {
    var rows: std.ArrayList(FleetListRow) = .empty;
    errdefer {
        for (rows.items) |r| {
            alloc.free(r.id);
            alloc.free(r.name);
            alloc.free(r.status);
            if (r.triggers_raw) |t| alloc.free(t);
        }
        rows.deinit(alloc);
    }
    while (try q.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(id);
        const name = try alloc.dupe(u8, try row.get([]const u8, 1));
        errdefer alloc.free(name);
        const status = try alloc.dupe(u8, try row.get([]const u8, 2));
        errdefer alloc.free(status);
        const created_at = try row.get(i64, 3);
        const updated_at = try row.get(i64, 4);
        const triggers_raw_opt = try row.get(?[]const u8, 5);
        const triggers_raw: ?[]const u8 = if (triggers_raw_opt) |raw| try alloc.dupe(u8, raw) else null;
        errdefer if (triggers_raw) |t| alloc.free(t);
        const events_processed = try row.get(i64, 6);
        const budget_used_nanos = try row.get(i64, 7);
        try rows.append(alloc, .{
            .id = id,
            .name = name,
            .status = status,
            .created_at = created_at,
            .updated_at = updated_at,
            .triggers_raw = triggers_raw,
            .events_processed = events_processed,
            .budget_used_nanos = budget_used_nanos,
        });
    }
    const owned = try rows.toOwnedSlice(alloc);
    errdefer {
        for (owned) |r| {
            alloc.free(r.id);
            alloc.free(r.name);
            alloc.free(r.status);
            if (r.triggers_raw) |t| alloc.free(t);
        }
        alloc.free(owned);
    }

    const next_cursor: ?[]const u8 = if (owned.len == limit and owned.len > 0) blk: {
        const last = owned[owned.len - 1];
        break :blk try keyset_cursor.format(alloc, .{ .created_at_ms = last.created_at, .id = last.id });
    } else null;

    return .{ .rows = owned, .next_cursor = next_cursor };
}
