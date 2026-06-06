//! GET /v1/api-keys — paginated, tenant-scoped listing.

const std = @import("std");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const pagination = @import("../pagination.zig");

const logging = @import("log");
const log = logging.scoped(.api_keys_list);

const Hx = hx_mod.Hx;

const S_CREATED_AT_DESC_ID_DESC = "created_at DESC, id DESC";

const ListRow = struct {
    id: []const u8,
    key_name: []const u8,
    active: bool,
    created_at: i64,
    last_used_at: ?i64,
    revoked_at: ?i64,
};

const ListQuery = struct {
    page: i32 = 1,
    page_size: i32 = pagination.DEFAULT_PAGE_SIZE,
    order_sql: []const u8 = S_CREATED_AT_DESC_ID_DESC,
};

fn parseListQuery(req: *httpz.Request) ?ListQuery {
    const qs = req.query() catch return null;
    const pp = pagination.parsePageParams(qs) orelse return null;
    var out: ListQuery = .{ .page = pp.page, .page_size = pp.page_size };
    if (qs.get("sort")) |s| out.order_sql = sortClauseFor(s) orelse return null;
    return out;
}

pub fn sortClauseFor(raw: []const u8) ?[]const u8 {
    if (std.mem.eql(u8, raw, "created_at")) return "created_at ASC, id ASC";
    if (std.mem.eql(u8, raw, "-created_at")) return S_CREATED_AT_DESC_ID_DESC;
    if (std.mem.eql(u8, raw, "key_name")) return "key_name ASC, id ASC";
    if (std.mem.eql(u8, raw, "-key_name")) return "key_name DESC, id DESC";
    return null;
}

pub fn innerListApiKeys(hx: Hx, req: *httpz.Request) void {
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, "Tenant context required; bootstrap principals cannot list tenant API keys");
        return;
    };
    const q = parseListQuery(req) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "page must be a positive integer; page_size must be between 1 and 100; sort must be one of created_at|-created_at|key_name|-key_name");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const items = fetchPage(hx, conn, tenant_id, q) orelse return;
    const total = fetchTotal(hx, conn, tenant_id, items.len) orelse return;

    hx.ok(.ok, .{
        .items = items,
        .total = total,
        .page = q.page,
        .page_size = q.page_size,
    });
}

fn fetchPage(hx: Hx, conn: anytype, tenant_id: []const u8, q: ListQuery) ?[]ListRow {
    const offset: i64 = @as(i64, q.page - 1) * @as(i64, q.page_size);
    const limit: i64 = q.page_size;
    // EXTRACT(EPOCH) yields seconds; the ×1000 multiplier gives the millis
    // the API surfaces. Named so the magic number lives on a const line.
    const SEC_TO_MILLIS = 1000;
    const epoch_ms_cols = std.fmt.comptimePrint(
        "(EXTRACT(EPOCH FROM created_at) * {0d})::bigint, " ++
            "(EXTRACT(EPOCH FROM last_used_at) * {0d})::bigint, " ++
            "(EXTRACT(EPOCH FROM revoked_at) * {0d})::bigint ",
        .{SEC_TO_MILLIS},
    );
    // order_sql comes from sortClauseFor's fixed allowlist, never user input.
    const list_sql = std.fmt.allocPrint(hx.alloc, "SELECT id::text, key_name, active, " ++
        epoch_ms_cols ++
        "FROM core.api_keys WHERE tenant_id = $1::uuid " ++
        "ORDER BY {s} LIMIT $2 OFFSET $3", .{q.order_sql}) catch {
        common.internalOperationError(hx.res, "Query build failed", hx.req_id);
        return null;
    };
    var rows_q = PgQuery.from(conn.query(list_sql, .{ tenant_id, limit, offset }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    });
    defer rows_q.deinit();

    var items: std.ArrayListUnmanaged(ListRow) = .empty;
    while (rows_q.next() catch null) |row| {
        const id = hx.alloc.dupe(u8, row.get([]u8, 0) catch continue) catch continue;
        const key_name = hx.alloc.dupe(u8, row.get([]u8, 1) catch continue) catch continue;
        items.append(hx.alloc, .{
            .id = id,
            .key_name = key_name,
            .active = row.get(bool, 2) catch continue,
            .created_at = row.get(i64, 3) catch continue,
            .last_used_at = row.get(i64, 4) catch null,
            .revoked_at = row.get(i64, 5) catch null,
        }) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
    }
    return items.toOwnedSlice(hx.alloc) catch &[_]ListRow{};
}

fn fetchTotal(hx: Hx, conn: anytype, tenant_id: []const u8, fallback: usize) ?i64 {
    var q = PgQuery.from(conn.query(
        \\SELECT COUNT(*)::bigint FROM core.api_keys WHERE tenant_id = $1::uuid
    , .{tenant_id}) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    });
    defer q.deinit();
    const row = q.next() catch return @intCast(fallback);
    if (row == null) return @intCast(fallback);
    return row.?.get(i64, 0) catch @intCast(fallback);
}
