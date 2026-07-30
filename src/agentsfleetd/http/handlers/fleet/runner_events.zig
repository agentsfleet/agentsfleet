//! GET /v1/fleets/runners/{id}/events — platform-admin runner history.

const std = @import("std");
const sql = @import("sql.zig");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const paging = @import("../../pagination.zig");
const keyset_cursor = @import("../../../fleet_runtime/keyset_cursor.zig");
const id_format = @import("../../../types/id_format.zig");
const protocol = @import("contract").protocol;
const runner_events = @import("../../../fleet/runner_events.zig");

const Hx = hx_mod.Hx;
const QUERY_EVENT_TYPE = "event_type";
const QUERY_SINCE = "since";
const QUERY_UNTIL = "until";
const QUERY_PAGE = "page";
const QUERY_PAGE_SIZE = "page_size";
const S_BAD_QUERY = "limit must be between 1 and 100; starting_after must be a cursor from a previous page; event_type must be a comma-separated set of runner event types; since/until must be millis";
const S_RETIRED_PARAMS = "page and page_size are retired on this list; page with starting_after and limit";

/// The enum's own cardinality bounds the token count, so a hostile comma chain
/// cannot inflate the allocation. Repeats inside that bound are accepted rather
/// than refused: the filter matches with `= ANY(...)`, where a duplicate tag
/// selects exactly the same rows.
const MAX_EVENT_TYPE_TOKENS = std.meta.fields(protocol.RunnerEventType).len;

pub fn innerListFleetRunnerEvents(hx: Hx, req: *httpz.Request, runner_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, runner_id, "runner_id")) return;
    if (hasRetiredPageParams(req)) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_RETIRED_PARAMS);
        return;
    }
    const q = parseListQuery(hx.alloc, req) orelse {
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

    const page = runner_events.listForRunner(conn, hx.alloc, runner_id, q.filter, q.cursor, @intCast(q.limit)) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    const next_cursor: ?[]const u8 = if (page.items.len == q.limit and page.items.len > 0) blk: {
        const last = page.items[page.items.len - 1];
        break :blk keyset_cursor.format(hx.alloc, .{ .created_at_ms = last.occurred_at, .id = last.id }) catch {
            common.internalOperationError(hx.res, "Failed to build the events page", hx.req_id);
            return;
        };
    } else null;
    hx.ok(.ok, protocol.RunnerEventsResponse{ .items = page.items, .total = page.total, .next_cursor = next_cursor });
}

const ListQuery = struct {
    cursor: ?runner_events.EventCursor = null,
    limit: u32,
    filter: runner_events.Filter = .{},
};

fn hasRetiredPageParams(req: *httpz.Request) bool {
    const qs = req.query() catch return false;
    return qs.get(QUERY_PAGE) != null or qs.get(QUERY_PAGE_SIZE) != null;
}

fn parseListQuery(alloc: std.mem.Allocator, req: *httpz.Request) ?ListQuery {
    const qs = req.query() catch return null;
    const limit = paging.parseLimit(qs.get(paging.QUERY_LIMIT)) catch return null;
    var out = ListQuery{ .limit = limit };
    if (qs.get(paging.QUERY_STARTING_AFTER)) |raw| {
        const parsed = keyset_cursor.parse(raw) catch return null;
        // The id half seeks a ::uuid bind; refusing a non-UUID here keeps a
        // crafted cursor at 400 instead of a Postgres cast error's 500.
        if (!id_format.isUuidV7(parsed.id)) return null;
        out.cursor = .{ .occurred_at = parsed.created_at_ms, .id = parsed.id };
    }
    if (qs.get(QUERY_EVENT_TYPE)) |raw| out.filter.event_types = parseEventTypeSet(alloc, raw) orelse return null;
    if (qs.get(QUERY_SINCE)) |raw| out.filter.since = std.fmt.parseInt(i64, raw, 10) catch return null;
    if (qs.get(QUERY_UNTIL)) |raw| out.filter.until = std.fmt.parseInt(i64, raw, 10) catch return null;
    if (out.filter.since) |since| {
        if (out.filter.until) |until| {
            if (until < since) return null;
        }
    }
    return out;
}

/// Parse the comma-separated `event_type` set (the guidelines' multi-value
/// equality grammar). An empty value, an empty token, an unrecognised tag, or
/// a chain longer than the enum itself all refuse the whole request — a
/// partial filter would read as "no such events". Returned slice lives in the
/// request arena.
fn parseEventTypeSet(alloc: std.mem.Allocator, raw: []const u8) ?[]const protocol.RunnerEventType {
    if (raw.len == 0) return null;
    var out: std.ArrayList(protocol.RunnerEventType) = .empty;
    defer out.deinit(alloc);
    var it = std.mem.splitScalar(u8, raw, ',');
    while (it.next()) |token| {
        if (token.len == 0) return null;
        if (out.items.len >= MAX_EVENT_TYPE_TOKENS) return null;
        const tag = std.meta.stringToEnum(protocol.RunnerEventType, token) orelse return null;
        out.append(alloc, tag) catch return null;
    }
    return out.toOwnedSlice(alloc) catch null;
}

test "event type set parses single, multi, and refuses bad shapes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const single = parseEventTypeSet(alloc, @tagName(protocol.RunnerEventType.runner_online)).?;
    try std.testing.expectEqual(@as(usize, 1), single.len);
    try std.testing.expectEqual(protocol.RunnerEventType.runner_online, single[0]);

    const pair = parseEventTypeSet(alloc, "runner_online,runner_offline").?;
    try std.testing.expectEqual(@as(usize, 2), pair.len);

    try std.testing.expect(parseEventTypeSet(alloc, "") == null);
    try std.testing.expect(parseEventTypeSet(alloc, "runner_online,") == null);
    try std.testing.expect(parseEventTypeSet(alloc, "runner_online,not_a_type") == null);
    try std.testing.expect(parseEventTypeSet(alloc, ",runner_online") == null);
}

fn runnerExists(conn: anytype, runner_id: []const u8) !bool {
    var q = PgQuery.from(try conn.query(sql.SELECT_RUNNER_EXISTS, .{runner_id}));
    defer q.deinit();
    return (try q.next()) != null;
}
