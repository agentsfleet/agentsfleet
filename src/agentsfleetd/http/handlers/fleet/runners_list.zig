//! GET /v1/fleets/runners — operator-plane read of the fleet.
//!
//! Gated by the `runner:read` scope (route_scopes.zig). Keyset-paginated over
//! the composite `(created_at, id)` key, newest first — the sole order; the
//! sortable Host column left with the table that used it. Each row carries a
//! DERIVED `liveness` (never the stored auth `status`, never the
//! `token_hash`): a runner minted but never seen reads `registered`; one
//! holding a live lease reads `busy` (the live-lease check runs before the
//! offline threshold, so a long execution that stops heartbeating is never
//! falsely offline); a fresh heartbeat reads `online`; stale beyond the lapse
//! threshold reads `offline`. Liveness is computed here, not stored — storing
//! it would drift (docs/architecture/runner_fleet.md "Runner state").

const std = @import("std");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const paging = @import("../../pagination.zig");
const keyset_cursor = @import("../../../fleet_runtime/keyset_cursor.zig");
const id_format = @import("../../../types/id_format.zig");
const sql = @import("sql.zig");
const protocol = @import("contract").protocol;
const constants = @import("common");

const logging = @import("log");

const MS_PER_SECOND = 1000;

const log = logging.scoped(.fleet_runners_list);

const Hx = hx_mod.Hx;

const MSG_RETIRED_PARAMS = "page, page_size and sort are retired on this list; page with starting_after and limit";
const MSG_BAD_LIMIT = "limit must be an integer between 1 and 100";
const MSG_BAD_CURSOR = "starting_after must be a cursor from a previous page";
const MSG_RUNNER_LIST_BUILD_FAILED = "Failed to build the runner list";

const QUERY_PAGE = "page";
const QUERY_PAGE_SIZE = "page_size";
const QUERY_SORT = "sort";

/// One fleet row as returned to the operator — no `token_hash`, no stored
/// `status`; `liveness` is derived, `labels` parsed from the stored JSONB.
const RunnerItem = struct {
    id: []const u8,
    host_id: []const u8,
    sandbox_tier: []const u8,
    admin_state: protocol.AdminState,
    liveness: protocol.RunnerLiveness,
    labels: []const []const u8,
    last_seen_at: i64,
    created_at: i64,
};

const ListQuery = struct {
    cursor: ?keyset_cursor.Cursor = null,
    limit: u32,
};

/// Derive runtime liveness from the stored `last_seen_at` + whether the runner
/// holds a live lease. Pure → unit-testable without a database. Order is
/// load-bearing: `busy` (live lease, actively renewing) is checked BEFORE the
/// offline threshold so a long-running execution is never falsely offline.
pub fn deriveLiveness(last_seen_at: i64, has_live_lease: bool, now_ms: i64) protocol.RunnerLiveness {
    if (last_seen_at == protocol.RUNNER_LAST_SEEN_NEVER) return .registered;
    if (has_live_lease) return .busy;
    if (now_ms - last_seen_at <= constants.RUNNER_OFFLINE_AFTER_MS) return .online;
    return .offline;
}

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

fn fetchPage(hx: Hx, conn: anytype, q: ListQuery, now_ms: i64) ?[]RunnerItem {
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

    return collectItems(hx.alloc, &rows_q, now_ms) catch |err| switch (err) {
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

/// Drain the row iterator into owned items. A row that fails to decode is
/// skipped (logged) — one bad row must not abort the page — but a mid-iteration
/// transport error propagates so the caller fails closed instead of returning a
/// partial page. `rows` is anything exposing `next() !?Row`; tests drive every
/// branch with a fake iterator. `alloc` is the caller-owned request arena, so
/// partial items on the error path are reclaimed when that arena is released.
fn collectItems(alloc: std.mem.Allocator, rows: anytype, now_ms: i64) ![]RunnerItem {
    var items: std.ArrayList(RunnerItem) = .empty;
    errdefer items.deinit(alloc);
    while (try rows.next()) |row| {
        const item = readItem(alloc, row, now_ms) catch |err| {
            log.warn("row_decode_skipped", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err) });
            continue;
        };
        try items.append(alloc, item);
    }
    return items.toOwnedSlice(alloc);
}

/// Build one item, duping borrowed row slices into the request arena (they
/// outlive `rows_q.deinit()`) and parsing the labels JSONB. `token_hash` and the
/// stored `status` are deliberately absent.
fn readItem(alloc: std.mem.Allocator, row: anytype, now_ms: i64) !RunnerItem {
    // Read the scalar columns first (fallible, no allocation), then dupe the
    // borrowed slices with an errdefer per owned slice — a decode error on a
    // later column frees the earlier dupes instead of leaking them on partial init.
    const raw_admin_state = try row.get([]u8, 3);
    const admin_state = std.meta.stringToEnum(protocol.AdminState, raw_admin_state) orelse return error.DbRowShape;
    const last_seen_at = try row.get(i64, 5);
    const created_at = try row.get(i64, 6);
    const has_live_lease = try row.get(bool, 7);
    const id = try alloc.dupe(u8, try row.get([]u8, 0));
    errdefer alloc.free(id);
    const host_id = try alloc.dupe(u8, try row.get([]u8, 1));
    errdefer alloc.free(host_id);
    const sandbox_tier = try alloc.dupe(u8, try row.get([]u8, 2));
    errdefer alloc.free(sandbox_tier);
    return .{
        .id = id,
        .host_id = host_id,
        .sandbox_tier = sandbox_tier,
        .admin_state = admin_state,
        .labels = parseLabels(alloc, try row.get([]u8, 4)),
        .last_seen_at = last_seen_at,
        .created_at = created_at,
        .liveness = deriveLiveness(last_seen_at, has_live_lease, now_ms),
    };
}

/// Parse the stored labels JSONB (a JSON array of strings) into owned slices.
/// A malformed value degrades to an empty set rather than failing the read.
/// Shared with the single-runner read (`runner_get.zig`) so both surfaces
/// decode labels identically.
pub fn parseLabels(alloc: std.mem.Allocator, text: []const u8) []const []const u8 {
    return std.json.parseFromSliceLeaky([]const []const u8, alloc, text, .{ .allocate = .alloc_always }) catch &.{};
}

const FakeRow = struct {
    const Self = @This();

    id: []const u8 = "r1",
    host_id: []const u8 = "h1",
    sandbox_tier: []const u8 = "landlock_full",
    admin_state: []const u8 = "active",
    labels_json: []const u8 = "[]",
    last_seen_at: i64 = 0,
    created_at: i64 = 0,
    has_live_lease: bool = false,
    fail_at: ?usize = null, // inject a decode error at this column index

    fn get(self: *const Self, comptime T: type, col: usize) !T {
        if (self.fail_at) |fc| {
            if (fc == col) return error.TestDecode;
        }
        if (T == []u8) return @constCast(switch (col) {
            0 => self.id,
            1 => self.host_id,
            2 => self.sandbox_tier,
            3 => self.admin_state,
            4 => self.labels_json,
            else => unreachable,
        });
        if (T == i64) return switch (col) {
            5 => self.last_seen_at,
            6 => self.created_at,
            else => unreachable,
        };
        if (T == bool) return switch (col) {
            7 => self.has_live_lease,
            else => unreachable,
        };
        unreachable;
    }
};

const FakeRows = struct {
    const Self = @This();

    rows: []const FakeRow,
    idx: usize = 0,
    fail_after: ?usize = null, // transport error once this many rows are yielded

    fn next(self: *Self) !?FakeRow {
        if (self.fail_after) |n| {
            if (self.idx == n) return error.TestTransport;
        }
        if (self.idx >= self.rows.len) return null;
        const r = self.rows[self.idx];
        self.idx += 1;
        return r;
    }
};

test "collectItems: a clean read returns every row in order" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var rows = FakeRows{ .rows = &.{ .{ .id = "a" }, .{ .id = "b" } } };
    const items = try collectItems(arena.allocator(), &rows, 1000);
    try std.testing.expectEqual(@as(usize, 2), items.len);
    try std.testing.expectEqualStrings("a", items[0].id);
    try std.testing.expectEqual(protocol.AdminState.active, items[0].admin_state);
    try std.testing.expectEqualStrings("b", items[1].id);
}

test "collectItems: a row that fails to decode is skipped; the rest survive" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var rows = FakeRows{ .rows = &.{ .{ .id = "a" }, .{ .id = "bad", .fail_at = 0 }, .{ .id = "c" } } };
    const items = try collectItems(arena.allocator(), &rows, 1000);
    try std.testing.expectEqual(@as(usize, 2), items.len);
    try std.testing.expectEqualStrings("a", items[0].id);
    try std.testing.expectEqualStrings("c", items[1].id);
}

test "collectItems: a mid-iteration transport error propagates (caller fails closed)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var rows = FakeRows{ .rows = &.{ .{ .id = "a" }, .{ .id = "b" } }, .fail_after = 1 };
    try std.testing.expectError(error.TestTransport, collectItems(arena.allocator(), &rows, MS_PER_SECOND));
}

test "readItem: a mid-decode column error frees the slices duped before it" {
    // Raw testing allocator (no arena): the leak detector fires if the errdefer
    // chain misses a dupe. fail_at=2 errors on sandbox_tier after id + host_id
    // are duped — both must be freed by readItem's errdefers.
    const fake = FakeRow{ .fail_at = 2 };
    try std.testing.expectError(error.TestDecode, readItem(std.testing.allocator, fake, MS_PER_SECOND));
}

test "parseLabels: a JSON array of strings parses to owned slices" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const labels = parseLabels(arena.allocator(), "[\"gpu\",\"prod\"]");
    try std.testing.expectEqual(@as(usize, 2), labels.len);
    try std.testing.expectEqualStrings("gpu", labels[0]);
}

test "parseLabels: malformed JSONB degrades to an empty set, not an error" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    try std.testing.expectEqual(@as(usize, 0), parseLabels(arena.allocator(), "{not valid").len);
}
