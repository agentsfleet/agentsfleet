//! GET /v1/fleets/runners — operator-plane read of the fleet.
//!
//! Gated by the `runner:read` scope (route_scopes.zig). Paginated, read-only.
//! Each row carries a DERIVED `liveness` (never the stored auth `status`, never
//! the `token_hash`): a runner minted but never seen reads `registered`; one
//! holding a live lease reads `busy` (the live-lease check runs before the
//! offline threshold, so a long execution that stops heartbeating is never
//! falsely offline); a fresh heartbeat reads `online`; stale beyond the lapse
//! threshold reads `offline`. Liveness is computed here, not stored — storing it
//! would drift (docs/architecture/runner_fleet.md "Runner state").

const std = @import("std");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const pagination = @import("../pagination.zig");
const sql = @import("sql.zig");
const protocol = @import("contract").protocol;
const constants = @import("common");

const logging = @import("log");

const MS_PER_SECOND = 1000;

const log = logging.scoped(.fleet_runners_list);

const Hx = hx_mod.Hx;

const S_CREATED_AT_DESC = "r.created_at DESC, r.id DESC";

const MSG_RUNNER_LIST_BUILD_FAILED = "Failed to build the runner list";

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

const PageRows = struct {
    items: []RunnerItem,
    total: i64,
};

const ListQuery = struct {
    page: i32 = 1,
    page_size: i32 = pagination.DEFAULT_PAGE_SIZE,
    order_sql: []const u8 = S_CREATED_AT_DESC,
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

fn sortClauseFor(raw: []const u8) ?[]const u8 {
    if (std.mem.eql(u8, raw, "-created_at")) return S_CREATED_AT_DESC;
    if (std.mem.eql(u8, raw, "created_at")) return "r.created_at ASC, r.id ASC";
    if (std.mem.eql(u8, raw, "host_id")) return "r.host_id ASC, r.id ASC";
    if (std.mem.eql(u8, raw, "-host_id")) return "r.host_id DESC, r.id DESC";
    return null;
}

fn parseListQuery(req: *httpz.Request) ?ListQuery {
    const qs = req.query() catch return null;
    const pp = pagination.parsePageParams(qs) orelse return null;
    var out: ListQuery = .{ .page = pp.page, .page_size = pp.page_size };
    if (qs.get("sort")) |s| out.order_sql = sortClauseFor(s) orelse return null;
    return out;
}

pub fn innerListFleetRunners(hx: Hx, req: *httpz.Request) void {
    const q = parseListQuery(req) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "page must be a positive integer; page_size must be between 1 and 100; sort must be one of created_at|-created_at|host_id|-host_id");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const now_ms = constants.clock.nowMillis();
    const page = fetchPage(hx, conn, q, now_ms) orelse return;

    hx.ok(.ok, .{
        .items = page.items,
        .total = page.total,
        .page = q.page,
        .page_size = q.page_size,
    });
}

fn fetchPage(hx: Hx, conn: anytype, q: ListQuery, now_ms: i64) ?PageRows {
    const offset: i64 = @as(i64, q.page - 1) * @as(i64, q.page_size);
    const limit: i64 = q.page_size;
    // order_sql is from sortClauseFor's fixed allowlist, never user input.
    // Statement text and the reasoning behind its shape live in `sql.zig`.
    const list_sql = std.fmt.allocPrint(hx.alloc, sql.SELECT_RUNNER_PAGE_FMT, .{ q.order_sql, q.order_sql }) catch {
        common.internalOperationError(hx.res, "Query build failed", hx.req_id);
        return null;
    };
    var rows_q = PgQuery.from(conn.query(list_sql, .{ protocol.RUNNER_LEASE_STATUS_ACTIVE, now_ms, limit, offset }) catch {
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
/// partial page that disagrees with the COUNT. `rows` is anything exposing
/// `next() !?Row`; tests drive every branch with a fake iterator. `alloc` is the
/// caller-owned request arena, so partial items on the error path are reclaimed
/// when that arena is released.
fn collectItems(alloc: std.mem.Allocator, rows: anytype, now_ms: i64) !PageRows {
    var items: std.ArrayList(RunnerItem) = .empty;
    errdefer items.deinit(alloc);
    var total: i64 = 0;
    while (try rows.next()) |row| {
        const row_total = try row.get(i64, 8);
        if (total == 0) total = row_total;
        if (try row.get(bool, 9)) {
            total = row_total;
            continue;
        }
        const item = readItem(alloc, row, now_ms) catch |err| {
            log.warn("row_decode_skipped", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err) });
            continue;
        };
        try items.append(alloc, item);
    }
    return .{ .items = try items.toOwnedSlice(alloc), .total = total };
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
fn parseLabels(alloc: std.mem.Allocator, text: []const u8) []const []const u8 {
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
    total: i64 = 1,
    count_only: bool = false,
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
            8 => self.total,
            else => unreachable,
        };
        if (T == bool) return switch (col) {
            7 => self.has_live_lease,
            9 => self.count_only,
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
    const page = try collectItems(arena.allocator(), &rows, 1000);
    const items = page.items;
    try std.testing.expectEqual(@as(usize, 2), items.len);
    try std.testing.expectEqual(@as(i64, 1), page.total);
    try std.testing.expectEqualStrings("a", items[0].id);
    try std.testing.expectEqual(protocol.AdminState.active, items[0].admin_state);
    try std.testing.expectEqualStrings("b", items[1].id);
}

test "collectItems: a row that fails to decode is skipped; the rest survive" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var rows = FakeRows{ .rows = &.{ .{ .id = "a" }, .{ .id = "bad", .fail_at = 0 }, .{ .id = "c" } } };
    const page = try collectItems(arena.allocator(), &rows, 1000);
    const items = page.items;
    try std.testing.expectEqual(@as(usize, 2), items.len);
    try std.testing.expectEqualStrings("a", items[0].id);
    try std.testing.expectEqualStrings("c", items[1].id);
}

test "collectItems: count-only sentinel preserves total for empty pages" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var rows = FakeRows{ .rows = &.{.{ .total = 42, .count_only = true }} };
    const page = try collectItems(arena.allocator(), &rows, 1000);
    try std.testing.expectEqual(@as(usize, 0), page.items.len);
    try std.testing.expectEqual(@as(i64, 42), page.total);
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
