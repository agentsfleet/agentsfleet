//! GET /v1/api-keys — paginated, tenant-scoped listing.

const std = @import("std");
const sql = @import("sql.zig");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const pagination = @import("../pagination.zig");

const logging = @import("log");
const log = logging.scoped(.api_keys_list);

const Hx = hx_mod.Hx;

const S_CREATED_AT_DESC_UID_DESC = "created_at DESC, uid DESC";

const ListRow = struct {
    id: []const u8,
    key_name: []const u8,
    active: bool,
    created_at: i64,
    last_used_at: ?i64,
    revoked_at: ?i64,
};

const PageRows = struct {
    items: []ListRow,
    total: i64,
};

const ListQuery = struct {
    page: i32 = 1,
    page_size: i32 = pagination.DEFAULT_PAGE_SIZE,
    order_sql: []const u8 = S_CREATED_AT_DESC_UID_DESC,
};

fn parseListQuery(req: *httpz.Request) ?ListQuery {
    const qs = req.query() catch return null;
    const pp = pagination.parsePageParams(qs) orelse return null;
    var out: ListQuery = .{ .page = pp.page, .page_size = pp.page_size };
    if (qs.get("sort")) |s| out.order_sql = sortClauseFor(s) orelse return null;
    return out;
}

pub fn sortClauseFor(raw: []const u8) ?[]const u8 {
    if (std.mem.eql(u8, raw, "created_at")) return "created_at ASC, uid ASC";
    if (std.mem.eql(u8, raw, "-created_at")) return S_CREATED_AT_DESC_UID_DESC;
    if (std.mem.eql(u8, raw, "key_name")) return "key_name ASC, uid ASC";
    if (std.mem.eql(u8, raw, "-key_name")) return "key_name DESC, uid DESC";
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

    const page = fetchPage(hx, conn, tenant_id, q) orelse return;

    hx.ok(.ok, .{
        .items = page.items,
        .total = page.total,
        .page = q.page,
        .page_size = q.page_size,
    });
}

fn fetchPage(hx: Hx, conn: anytype, tenant_id: []const u8, q: ListQuery) ?PageRows {
    const offset: i64 = @as(i64, q.page - 1) * @as(i64, q.page_size);
    const limit: i64 = q.page_size;
    // order_sql comes from sortClauseFor's fixed allowlist, never user input.
    const list_sql = std.fmt.allocPrint(hx.alloc, sql.SELECT_TENANT_KEY_PAGE_FMT, .{ q.order_sql, q.order_sql }) catch {
        common.internalOperationError(hx.res, "Query build failed", hx.req_id);
        return null;
    };
    var rows_q = PgQuery.from(conn.query(list_sql, .{ tenant_id, limit, offset }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    });
    defer rows_q.deinit();

    var items: std.ArrayList(ListRow) = .empty;
    var total: i64 = 0;
    while (rows_q.next() catch null) |row| {
        const row_total = row.get(i64, 6) catch total;
        if (total == 0) total = row_total;
        if (row.get(bool, 7) catch false) {
            total = row_total;
            continue;
        }
        const id = hx.alloc.dupe(u8, row.get([]u8, 0) catch continue) catch continue;
        const key_name = hx.alloc.dupe(u8, row.get([]u8, 1) catch continue) catch continue;
        items.append(hx.alloc, .{
            .id = id,
            .key_name = key_name,
            .active = row.get(bool, 2) catch continue,
            .created_at = row.get(i64, 3) catch continue,
            .last_used_at = row.get(i64, 4) catch null,
            .revoked_at = row.get(i64, 5) catch null,
        }) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
    }
    return .{ .items = items.toOwnedSlice(hx.alloc) catch &[_]ListRow{}, .total = total };
}
