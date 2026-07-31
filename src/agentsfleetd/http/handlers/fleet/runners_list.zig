//! GET /v1/fleets/runners — operator-plane read of the fleet.
//!
//! Gated by the `runner:read` scope (route_scopes.zig). Keyset-paginated over
//! the composite `(created_at, id)` key, newest first — the sole order; the
//! sortable Host column left with the table that used it. Each row carries a
//! DERIVED `liveness` (never the stored auth `status`, never the
//! `token_hash`), the assigned policy, the host's reported capability, and the
//! degraded verdict — row → item decoding lives in `runner_row.zig`, shared
//! with the single-runner read so the two surfaces cannot drift.

const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const paging = @import("../../pagination.zig");
const keyset_cursor = @import("../../../fleet_runtime/keyset_cursor.zig");
const id_format = @import("../../../types/id_format.zig");
const sql = @import("sql.zig");
const runner_row = @import("runner_row.zig");
const protocol = @import("contract").protocol;
const constants = @import("common");

const Hx = hx_mod.Hx;

const MSG_RETIRED_PARAMS = "page, page_size and sort are retired on this list; page with starting_after and limit";
const MSG_BAD_LIMIT = "limit must be an integer between 1 and 100";
const MSG_BAD_CURSOR = "starting_after must be a cursor from a previous page";
const MSG_RUNNER_LIST_BUILD_FAILED = "Failed to build the runner list";

const QUERY_PAGE = "page";
const QUERY_PAGE_SIZE = "page_size";
const QUERY_SORT = "sort";

const ListQuery = struct {
    cursor: ?keyset_cursor.Cursor = null,
    limit: u32,
};

pub fn innerListFleetRunners(hx: Hx, req: *httpz.Request) void {
    const q = parseListQuery(req) orelse return failParse(hx, req);

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const total = fetchTotal(conn) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    const now_ms = constants.clock.nowMillis();
    const items = fetchPage(hx, conn, q, now_ms) orelse return;

    const next_cursor: ?[]const u8 = if (items.len == q.limit and items.len > 0) blk: {
        const last = items[items.len - 1];
        break :blk keyset_cursor.format(hx.alloc, .{ .created_at_ms = last.created_at, .id = last.id }) catch {
            common.internalOperationError(hx.res, MSG_RUNNER_LIST_BUILD_FAILED, hx.req_id);
            return;
        };
    } else null;

    hx.ok(.ok, .{ .items = items, .total = total, .next_cursor = next_cursor });
}

/// Name the precise refusal: a retired parameter is called out as retired, a
/// bad limit names its range, a bad cursor names the fix.
fn failParse(hx: Hx, req: *httpz.Request) void {
    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, MSG_BAD_CURSOR);
        return;
    };
    if (qs.get(QUERY_PAGE) != null or qs.get(QUERY_PAGE_SIZE) != null or qs.get(QUERY_SORT) != null) {
        hx.fail(ec.ERR_INVALID_REQUEST, MSG_RETIRED_PARAMS);
        return;
    }
    if (paging.parseLimit(qs.get(paging.QUERY_LIMIT))) |_| {} else |_| {
        hx.fail(ec.ERR_INVALID_REQUEST, MSG_BAD_LIMIT);
        return;
    }
    hx.fail(ec.ERR_INVALID_REQUEST, MSG_BAD_CURSOR);
}

fn parseListQuery(req: *httpz.Request) ?ListQuery {
    const qs = req.query() catch return null;
    if (qs.get(QUERY_PAGE) != null or qs.get(QUERY_PAGE_SIZE) != null or qs.get(QUERY_SORT) != null) return null;
    const limit = paging.parseLimit(qs.get(paging.QUERY_LIMIT)) catch return null;
    var out: ListQuery = .{ .limit = limit };
    if (qs.get(paging.QUERY_STARTING_AFTER)) |raw| {
        const parsed = keyset_cursor.parse(raw) catch return null;
        // The id half seeks a ::uuid bind; refusing a non-UUID here keeps a
        // crafted cursor at 400 instead of a Postgres cast error's 500.
        if (!id_format.isUuid(parsed.id)) return null;
        out.cursor = parsed;
    }
    return out;
}

fn fetchTotal(conn: anytype) !i64 {
    var q = PgQuery.from(try conn.query(sql.SELECT_RUNNER_COUNT, .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.DbRowShape;
    return try row.get(i64, 0);
}

fn fetchPage(hx: Hx, conn: anytype, q: ListQuery, now_ms: i64) ?[]runner_row.RunnerItem {
    const limit: i64 = @intCast(q.limit);
    var rows_q = if (q.cursor) |cursor|
        PgQuery.from(conn.query(sql.SELECT_RUNNER_KEYSET_AFTER, .{ protocol.RUNNER_LEASE_STATUS_ACTIVE, now_ms, cursor.created_at_ms, cursor.id, limit }) catch {
            common.internalDbError(hx.res, hx.req_id);
            return null;
        })
    else
        PgQuery.from(conn.query(sql.SELECT_RUNNER_KEYSET_FIRST, .{ protocol.RUNNER_LEASE_STATUS_ACTIVE, now_ms, limit }) catch {
            common.internalDbError(hx.res, hx.req_id);
            return null;
        });
    defer rows_q.deinit();

    return runner_row.collectItems(hx.alloc, &rows_q, now_ms) catch |err| switch (err) {
        error.OutOfMemory => {
            common.internalOperationError(hx.res, MSG_RUNNER_LIST_BUILD_FAILED, hx.req_id);
            return null;
        },
        else => {
            common.internalDbError(hx.res, hx.req_id);
            return null;
        },
    };
}

test {
    _ = runner_row; // pulls the shared decode module's tests into discovery
}
