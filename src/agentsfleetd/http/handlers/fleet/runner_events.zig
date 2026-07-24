//! GET /v1/fleets/runners/{id}/events — platform-admin runner history.

const std = @import("std");
const sql = @import("sql.zig");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const pagination = @import("../pagination.zig");
const protocol = @import("contract").protocol;
const runner_events = @import("../../../fleet/runner_events.zig");

const Hx = hx_mod.Hx;
const QUERY_EVENT_TYPE = "event_type";
const QUERY_SINCE = "since";
const QUERY_UNTIL = "until";
const S_BAD_QUERY = "page must be a positive integer; page_size must be between 1 and 100; event_type must be a runner event type; since/until must be millis";

pub fn innerListFleetRunnerEvents(hx: Hx, req: *httpz.Request, runner_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, runner_id, "runner_id")) return;
    const q = parseListQuery(req) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, S_BAD_QUERY);
        return;
    };
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const exists = runnerExists(conn, runner_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    if (!exists) {
        hx.fail(ec.ERR_RUNNER_NOT_FOUND, "Runner not found");
        return;
    }

    const page = runner_events.listForRunner(conn, hx.alloc, runner_id, q.filter, q.page, q.page_size) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    hx.ok(.ok, protocol.RunnerEventsResponse{ .items = page.items, .total = page.total, .page = q.page, .page_size = q.page_size });
}

const ListQuery = struct {
    page: i32 = 1,
    page_size: i32 = pagination.DEFAULT_PAGE_SIZE,
    filter: runner_events.Filter = .{},
};

fn parseListQuery(req: *httpz.Request) ?ListQuery {
    const qs = req.query() catch return null;
    const pp = pagination.parsePageParams(qs) orelse return null;
    var out = ListQuery{ .page = pp.page, .page_size = pp.page_size };
    if (qs.get(QUERY_EVENT_TYPE)) |raw| out.filter.event_type = std.meta.stringToEnum(protocol.RunnerEventType, raw) orelse return null;
    if (qs.get(QUERY_SINCE)) |raw| out.filter.since = std.fmt.parseInt(i64, raw, 10) catch return null;
    if (qs.get(QUERY_UNTIL)) |raw| out.filter.until = std.fmt.parseInt(i64, raw, 10) catch return null;
    if (out.filter.since) |since| {
        if (out.filter.until) |until| {
            if (until < since) return null;
        }
    }
    return out;
}

fn runnerExists(conn: anytype, runner_id: []const u8) !bool {
    var q = PgQuery.from(try conn.query(sql.SELECT_RUNNER_EXISTS, .{runner_id}));
    defer q.deinit();
    return (try q.next()) != null;
}
