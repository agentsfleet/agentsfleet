//! GET /v1/tenants/me/workspaces — tenant-scoped workspace list.
//!
//! Returns stable oldest-first cursor pages. An exact `name` filter supports
//! create reconciliation without downloading or scanning the tenant's list.
//!
//! ONE statement per request: the principal-tenant resolve (user row first,
//! token claim as fallback — the same authority order as the authorize funnel)
//! is a CTE the page select joins, so the pre-merge tenant-resolve round trip
//! is gone. The LEFT JOIN LATERAL guarantees a resolved tenant always returns
//! at least one row — a tenant with zero workspaces gets a marker row (NULL
//! workspace columns, real tenant id), which is how the response still carries
//! `tenant_id` beside empty items. Zero rows means the tenant itself did not
//! resolve, which is the 403.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const keyset_cursor = @import("../../fleet_runtime/keyset_cursor.zig");

const Hx = hx_mod.Hx;

const DEFAULT_PAGE_LIMIT: u32 = 50;
const MAX_PAGE_LIMIT: u32 = 100;
const MAX_WORKSPACE_NAME_CODEPOINTS: usize = 128;
const TENANT_BUFFER_BYTES: usize = 64;
const DETAIL_TENANT_REQUIRED = "Tenant context required";
const DETAIL_MALFORMED_QUERY = "Malformed query string";
const DETAIL_INVALID_LIMIT = "Limit must be between 1 and 100";
const DETAIL_INVALID_CURSOR = "Invalid starting_after cursor";
const DETAIL_INVALID_NAME = "Name must be between 1 and 128 Unicode code points";

// The resolve CTE + marker-row join shared by every variant. `$1` OIDC
// subject or NULL, `$2` tenant claim or NULL; later binds are per-variant.
// The outer ORDER BY names output aliases so the plan cannot reorder the
// lateral's rows; the single marker row has NULL sort keys, so NULLS LAST
// keeps it harmless.
const SQL_PROLOGUE =
    \\WITH t AS (
    \\    SELECT COALESCE(
    \\        (SELECT u.tenant_id FROM core.users u WHERE u.oidc_subject = $1),
    \\        $2::uuid) AS tenant_id
    \\)
    \\SELECT w.id, w.name, w.created_at, t.tenant_id::text
    \\FROM t
    \\LEFT JOIN LATERAL (
    \\    SELECT ws.id::text AS id, ws.name, ws.created_at
    \\    FROM core.workspaces AS ws
    \\    WHERE ws.tenant_id = t.tenant_id
    \\
;
// The inner sort. It must precede the per-variant LIMIT: SQL rejects the other
// order outright, and the keyset needs the limit to cut a sorted set rather
// than an arbitrary one. Qualified `ws.` so the sort reads the uuid column and
// not the `::text` alias the select list binds to the same name — unqualified
// `id` here resolves to the alias, which sorts a different type than the
// boundary comparison below uses.
const SQL_INNER_ORDER =
    \\    ORDER BY ws.created_at ASC, ws.id ASC
    \\
;
const SQL_EPILOGUE =
    \\) w ON TRUE
    \\WHERE t.tenant_id IS NOT NULL
    \\ORDER BY w.created_at ASC NULLS LAST, w.id ASC
;

const SQL_FIRST = SQL_PROLOGUE ++ SQL_INNER_ORDER ++
    \\    LIMIT $3
    \\
++ SQL_EPILOGUE;
const SQL_AFTER = SQL_PROLOGUE ++
    \\      AND (ws.created_at, ws.id) > ($3, $4::uuid)
    \\
++ SQL_INNER_ORDER ++
    \\    LIMIT $5
    \\
++ SQL_EPILOGUE;
const SQL_FIRST_BY_NAME = SQL_PROLOGUE ++
    \\      AND ws.name = $3
    \\
++ SQL_INNER_ORDER ++
    \\    LIMIT $4
    \\
++ SQL_EPILOGUE;
const SQL_AFTER_BY_NAME = SQL_PROLOGUE ++
    \\      AND ws.name = $3
    \\      AND (ws.created_at, ws.id) > ($4, $5::uuid)
    \\
++ SQL_INNER_ORDER ++
    \\    LIMIT $6
    \\
++ SQL_EPILOGUE;

pub fn innerListTenantWorkspaces(hx: Hx, req: *httpz.Request) void {
    const binds = common.principalTenantBinds(hx.principal) orelse {
        hx.fail(ec.ERR_FORBIDDEN, DETAIL_TENANT_REQUIRED);
        return;
    };

    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, DETAIL_MALFORMED_QUERY);
        return;
    };
    const limit = parseLimit(qs) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, DETAIL_INVALID_LIMIT);
        return;
    };
    const cursor = parseCursor(qs) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, DETAIL_INVALID_CURSOR);
        return;
    };
    const name = parseName(qs) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, DETAIL_INVALID_NAME);
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    var tenant_buf: [TENANT_BUFFER_BYTES]u8 = undefined;
    var page = fetchWorkspacePage(
        conn,
        hx.alloc,
        binds,
        &tenant_buf,
        name,
        cursor,
        limit,
    ) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        // Zero rows: the tenant itself did not resolve (no user row, no claim
        // match) — same refusal the standalone resolve used to answer.
        hx.fail(ec.ERR_FORBIDDEN, DETAIL_TENANT_REQUIRED);
        return;
    };
    defer page.deinit(hx.alloc);

    hx.ok(.ok, .{
        .items = page.items,
        .tenant_id = page.tenant_id,
        .total = null,
        .next_cursor = page.next_cursor,
    });
}

const WorkspaceRow = struct {
    id: []const u8,
    name: ?[]const u8,
    created_at: i64,
};

fn freeWorkspaceRows(alloc: std.mem.Allocator, rows: []const WorkspaceRow) void {
    for (rows) |row| {
        alloc.free(row.id);
        if (row.name) |name| alloc.free(name);
    }
}

const WorkspacePage = struct {
    items: []WorkspaceRow,
    next_cursor: ?[]const u8,
    /// Points into the caller's tenant buffer — valid for the request scope.
    tenant_id: []const u8,

    fn deinit(self: *WorkspacePage, alloc: std.mem.Allocator) void {
        freeWorkspaceRows(alloc, self.items);
        alloc.free(self.items);
        if (self.next_cursor) |cursor| alloc.free(cursor);
    }
};

fn parseLimit(qs: anytype) !u32 {
    const raw = qs.get("limit") orelse return DEFAULT_PAGE_LIMIT;
    const parsed = std.fmt.parseInt(u32, raw, 10) catch return error.InvalidLimit;
    if (parsed == 0 or parsed > MAX_PAGE_LIMIT) return error.InvalidLimit;
    return parsed;
}

fn parseCursor(qs: anytype) !?keyset_cursor.Cursor {
    const raw = qs.get("starting_after") orelse return null;
    const cursor = keyset_cursor.parse(raw) catch return error.InvalidCursor;
    if (!id_format.isSupportedWorkspaceId(cursor.id)) return error.InvalidCursor;
    return cursor;
}

fn parseName(qs: anytype) !?[]const u8 {
    const name = qs.get("name") orelse return null;
    const view = std.unicode.Utf8View.init(name) catch return error.InvalidName;
    var iterator = view.iterator();
    var codepoints: usize = 0;
    while (iterator.nextCodepoint()) |codepoint| {
        if (codepoint == 0) return error.InvalidName;
        codepoints += 1;
    }
    if (codepoints == 0 or codepoints > MAX_WORKSPACE_NAME_CODEPOINTS) {
        return error.InvalidName;
    }
    return name;
}

/// Null return means the tenant did not resolve (the caller's 403); an empty
/// page for a real tenant returns with empty items and the tenant id.
fn fetchWorkspacePage(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    binds: common.TenantBinds,
    tenant_buf: []u8,
    name: ?[]const u8,
    cursor: ?keyset_cursor.Cursor,
    limit: u32,
) !?WorkspacePage {
    const query_limit: i64 = @intCast(limit + 1);
    if (name) |exact_name| {
        if (cursor) |after| {
            var q = PgQuery.from(try conn.query(
                SQL_AFTER_BY_NAME,
                .{ binds.subject, binds.claim, exact_name, after.created_at_ms, after.id, query_limit },
            ));
            defer q.deinit();
            return try collectWorkspacePage(alloc, &q, tenant_buf, limit);
        }
        var q = PgQuery.from(try conn.query(
            SQL_FIRST_BY_NAME,
            .{ binds.subject, binds.claim, exact_name, query_limit },
        ));
        defer q.deinit();
        return try collectWorkspacePage(alloc, &q, tenant_buf, limit);
    }
    if (cursor) |after| {
        var q = PgQuery.from(try conn.query(
            SQL_AFTER,
            .{ binds.subject, binds.claim, after.created_at_ms, after.id, query_limit },
        ));
        defer q.deinit();
        return try collectWorkspacePage(alloc, &q, tenant_buf, limit);
    }
    var q = PgQuery.from(try conn.query(SQL_FIRST, .{ binds.subject, binds.claim, query_limit }));
    defer q.deinit();
    return try collectWorkspacePage(alloc, &q, tenant_buf, limit);
}

fn collectWorkspacePage(
    alloc: std.mem.Allocator,
    query: *PgQuery,
    tenant_buf: []u8,
    limit: u32,
) !?WorkspacePage {
    var rows: std.ArrayList(WorkspaceRow) = .empty;
    const page_limit: usize = @intCast(limit);
    errdefer {
        freeWorkspaceRows(alloc, rows.items);
        rows.deinit(alloc);
    }

    var tenant_id: ?[]const u8 = null;
    var has_more = false;
    while (try query.next()) |row| {
        if (tenant_id == null) {
            const t = try row.get([]const u8, 3);
            if (t.len == 0 or t.len > tenant_buf.len) return error.InvalidTenantMapping;
            @memcpy(tenant_buf[0..t.len], t);
            tenant_id = tenant_buf[0..t.len];
        }
        const maybe_id = try row.get(?[]const u8, 0);
        const raw_id = maybe_id orelse continue; // zero-workspace marker row
        if (rows.items.len == page_limit) {
            has_more = true;
            break;
        }
        const id = try alloc.dupe(u8, raw_id);
        errdefer alloc.free(id);
        const raw_name = try row.get(?[]const u8, 1);
        const name: ?[]const u8 = if (raw_name) |value|
            try alloc.dupe(u8, value)
        else
            null;
        errdefer if (name) |value| alloc.free(value);
        try rows.append(alloc, .{
            .id = id,
            .name = name,
            .created_at = try row.get(i64, 2),
        });
    }
    const resolved_tenant = tenant_id orelse return null;

    const items = try rows.toOwnedSlice(alloc);
    errdefer {
        freeWorkspaceRows(alloc, items);
        alloc.free(items);
    }
    const next_cursor: ?[]const u8 = if (has_more and items.len > 0) blk: {
        const last = items[items.len - 1];
        break :blk try keyset_cursor.format(alloc, .{
            .created_at_ms = last.created_at,
            .id = last.id,
        });
    } else null;
    return .{ .items = items, .next_cursor = next_cursor, .tenant_id = resolved_tenant };
}
