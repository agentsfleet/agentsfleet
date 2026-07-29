//! GET /v1/api-keys — keyset-paginated, tenant-scoped listing.
//!
//! The full sort allowlist survives the keyset migration: the cursor carries
//! the boundary SORT VALUE (a timestamp or a key name) beside the row id, so
//! any ordering pages without loss. A cursor whose form does not match the
//! requested sort is refused — resuming a key_name walk with a created_at
//! boundary would silently paginate a different order.

const std = @import("std");
const sql = @import("sql.zig");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const paging = @import("../../pagination.zig");
const keyset_cursor = @import("../../../fleet_runtime/keyset_cursor.zig");

const logging = @import("log");
const log = logging.scoped(.api_keys_list);

const Hx = hx_mod.Hx;

const S_CREATED_AT_DESC_UID_DESC = "created_at DESC, uid DESC";
const SORT_DEFAULT = "-created_at";
const MSG_RETIRED_PARAMS = "page and page_size are retired on this list; page with starting_after and limit";
const MSG_BAD_QUERY = "limit must be between 1 and 100; sort must be one of created_at|-created_at|key_name|-key_name; starting_after must be a cursor issued under the same sort";
const MSG_LIST_BUILD_FAILED = "Failed to build the key list";

const QUERY_PAGE = "page";
const QUERY_PAGE_SIZE = "page_size";
const QUERY_SORT = "sort";

/// Row-value comparators for the two seek directions; interpolated into the
/// `{s}` slot beside the allowlisted ORDER BY, never from user input.
const CMP_FORWARD_ASC = ">";
const CMP_FORWARD_DESC = "<";

const SortKey = enum { created_at, key_name };

const SortSpec = struct {
    order_sql: []const u8,
    cmp: []const u8,
    key: SortKey,
};

pub fn sortSpecFor(raw: []const u8) ?SortSpec {
    if (std.mem.eql(u8, raw, "created_at")) return .{ .order_sql = "created_at ASC, uid ASC", .cmp = CMP_FORWARD_ASC, .key = .created_at };
    if (std.mem.eql(u8, raw, SORT_DEFAULT)) return .{ .order_sql = S_CREATED_AT_DESC_UID_DESC, .cmp = CMP_FORWARD_DESC, .key = .created_at };
    if (std.mem.eql(u8, raw, "key_name")) return .{ .order_sql = "key_name ASC, uid ASC", .cmp = CMP_FORWARD_ASC, .key = .key_name };
    if (std.mem.eql(u8, raw, "-key_name")) return .{ .order_sql = "key_name DESC, uid DESC", .cmp = CMP_FORWARD_DESC, .key = .key_name };
    return null;
}

const ListRow = struct {
    id: []const u8,
    key_name: []const u8,
    active: bool,
    created_at: i64,
    last_used_at: ?i64,
    revoked_at: ?i64,
};

const ListQuery = struct {
    cursor: ?keyset_cursor.SortCursor = null,
    limit: u32,
    spec: SortSpec,
};

fn parseListQuery(alloc: std.mem.Allocator, req: *httpz.Request) ?ListQuery {
    const qs = req.query() catch return null;
    const limit = paging.parseLimit(qs.get(paging.QUERY_LIMIT)) catch return null;
    const spec = if (qs.get(QUERY_SORT)) |s| sortSpecFor(s) orelse return null else sortSpecFor(SORT_DEFAULT).?;
    var out: ListQuery = .{ .limit = limit, .spec = spec };
    if (qs.get(paging.QUERY_STARTING_AFTER)) |raw| {
        const cursor = keyset_cursor.parseSort(alloc, raw) catch return null;
        // The cursor's form must match the active sort key, or the seek would
        // resume a different ordering.
        const matches = switch (cursor.sort) {
            .ts => spec.key == .created_at,
            .text => spec.key == .key_name,
        };
        if (!matches) return null;
        out.cursor = cursor;
    }
    return out;
}

pub fn innerListApiKeys(hx: Hx, req: *httpz.Request) void {
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, "Tenant context required; bootstrap principals cannot list tenant API keys");
        return;
    };
    if (hasRetiredPageParams(req)) {
        hx.fail(ec.ERR_INVALID_REQUEST, MSG_RETIRED_PARAMS);
        return;
    }
    const q = parseListQuery(hx.alloc, req) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, MSG_BAD_QUERY);
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const total = fetchTotal(conn, tenant_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    const items = fetchPage(hx, conn, tenant_id, q) orelse return;

    const next_cursor: ?[]const u8 = if (items.len == q.limit and items.len > 0) blk: {
        const last = items[items.len - 1];
        const boundary: keyset_cursor.SortCursor = switch (q.spec.key) {
            .created_at => .{ .sort = .{ .ts = last.created_at }, .id = last.id },
            .key_name => .{ .sort = .{ .text = last.key_name }, .id = last.id },
        };
        break :blk keyset_cursor.formatSort(hx.alloc, boundary) catch {
            common.internalOperationError(hx.res, MSG_LIST_BUILD_FAILED, hx.req_id);
            return;
        };
    } else null;

    hx.ok(.ok, .{ .items = items, .total = total, .next_cursor = next_cursor });
}

fn hasRetiredPageParams(req: *httpz.Request) bool {
    const qs = req.query() catch return false;
    return qs.get(QUERY_PAGE) != null or qs.get(QUERY_PAGE_SIZE) != null;
}

fn fetchTotal(conn: anytype, tenant_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(sql.SELECT_TENANT_KEY_COUNT, .{tenant_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.DbRowShape;
    return try row.get(i64, 0);
}

fn fetchPage(hx: Hx, conn: anytype, tenant_id: []const u8, q: ListQuery) ?[]ListRow {
    // Both `{s}` slots come from sortSpecFor's fixed allowlist, never user input.
    const limit: i64 = @intCast(q.limit);
    var rows_q = if (q.cursor) |cursor| switch (cursor.sort) {
        .ts => |ts| blk: {
            const stmt = std.fmt.allocPrint(hx.alloc, sql.SELECT_TENANT_KEY_KEYSET_AFTER_CREATED_FMT, .{ q.spec.cmp, q.spec.order_sql }) catch {
                common.internalOperationError(hx.res, MSG_LIST_BUILD_FAILED, hx.req_id);
                return null;
            };
            break :blk PgQuery.from(conn.query(stmt, .{ tenant_id, ts, cursor.id, limit }) catch {
                common.internalDbError(hx.res, hx.req_id);
                return null;
            });
        },
        .text => |text| blk: {
            const stmt = std.fmt.allocPrint(hx.alloc, sql.SELECT_TENANT_KEY_KEYSET_AFTER_NAME_FMT, .{ q.spec.cmp, q.spec.order_sql }) catch {
                common.internalOperationError(hx.res, MSG_LIST_BUILD_FAILED, hx.req_id);
                return null;
            };
            break :blk PgQuery.from(conn.query(stmt, .{ tenant_id, text, cursor.id, limit }) catch {
                common.internalDbError(hx.res, hx.req_id);
                return null;
            });
        },
    } else blk: {
        const stmt = std.fmt.allocPrint(hx.alloc, sql.SELECT_TENANT_KEY_KEYSET_FIRST_FMT, .{q.spec.order_sql}) catch {
            common.internalOperationError(hx.res, MSG_LIST_BUILD_FAILED, hx.req_id);
            return null;
        };
        break :blk PgQuery.from(conn.query(stmt, .{ tenant_id, limit }) catch {
            common.internalDbError(hx.res, hx.req_id);
            return null;
        });
    };
    defer rows_q.deinit();

    return collectRows(hx.alloc, &rows_q) catch |err| switch (err) {
        error.OutOfMemory => {
            common.internalOperationError(hx.res, MSG_LIST_BUILD_FAILED, hx.req_id);
            return null;
        },
        else => {
            common.internalDbError(hx.res, hx.req_id);
            return null;
        },
    };
}

/// Drain the page into owned rows. A row that fails to decode is skipped and
/// logged; a mid-iteration transport error propagates so the caller fails
/// closed. `alloc` is the request arena.
fn collectRows(alloc: std.mem.Allocator, q: *PgQuery) ![]ListRow {
    var items: std.ArrayList(ListRow) = .empty;
    errdefer items.deinit(alloc);
    while (try q.next()) |row| {
        const item = readRow(alloc, row) catch |err| {
            log.warn("key_row_decode_skipped", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err) });
            continue;
        };
        try items.append(alloc, item);
    }
    return items.toOwnedSlice(alloc);
}

fn readRow(alloc: std.mem.Allocator, row: anytype) !ListRow {
    const active = try row.get(bool, 2);
    const created_at = try row.get(i64, 3);
    const last_used_at = try row.get(?i64, 4);
    const revoked_at = try row.get(?i64, 5);
    const id = try alloc.dupe(u8, try row.get([]u8, 0));
    errdefer alloc.free(id);
    const key_name = try alloc.dupe(u8, try row.get([]u8, 1));
    errdefer alloc.free(key_name);
    return .{
        .id = id,
        .key_name = key_name,
        .active = active,
        .created_at = created_at,
        .last_used_at = last_used_at,
        .revoked_at = revoked_at,
    };
}

test "sortSpecFor: every allowlisted sort maps to an order, a direction and a key" {
    const desc = sortSpecFor("-created_at").?;
    try std.testing.expectEqual(SortKey.created_at, desc.key);
    try std.testing.expectEqualStrings(CMP_FORWARD_DESC, desc.cmp);
    const asc = sortSpecFor("created_at").?;
    try std.testing.expectEqualStrings(CMP_FORWARD_ASC, asc.cmp);
    const name_asc = sortSpecFor("key_name").?;
    try std.testing.expectEqual(SortKey.key_name, name_asc.key);
    const name_desc = sortSpecFor("-key_name").?;
    try std.testing.expectEqualStrings(CMP_FORWARD_DESC, name_desc.cmp);
    try std.testing.expect(sortSpecFor("host_id") == null);
    try std.testing.expect(sortSpecFor("") == null);
}
