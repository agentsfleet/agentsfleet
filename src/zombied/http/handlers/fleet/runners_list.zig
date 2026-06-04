//! GET /v1/fleet/runners — platform-admin operator-plane read of the fleet.
//!
//! Authed by `platformAdmin` (same gate as enrollment). Paginated, read-only.
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
const protocol = @import("contract").protocol;
const constants = @import("common");

const logging = @import("log");
const log = logging.scoped(.fleet_runners_list);

const Hx = hx_mod.Hx;

const S_CREATED_AT_DESC = "r.created_at DESC, r.id DESC";

const DEFAULT_PAGE_SIZE: i32 = 25;
const MAX_PAGE_SIZE: i32 = 100;

const MSG_OUT_OF_MEMORY = "Out of memory";

/// One fleet row as returned to the operator — no `token_hash`, no stored
/// `status`; `liveness` is derived, `labels` parsed from the stored JSONB.
const RunnerItem = struct {
    id: []const u8,
    host_id: []const u8,
    sandbox_tier: []const u8,
    liveness: protocol.RunnerLiveness,
    labels: []const []const u8,
    last_seen_at: i64,
    created_at: i64,
};

const ListQuery = struct {
    page: i32 = 1,
    page_size: i32 = DEFAULT_PAGE_SIZE,
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
    const qs = req.query() catch return .{};
    var out: ListQuery = .{};
    if (qs.get("page")) |v| out.page = std.fmt.parseInt(i32, v, 10) catch 1;
    if (qs.get("page_size")) |v| out.page_size = std.fmt.parseInt(i32, v, 10) catch DEFAULT_PAGE_SIZE;
    if (out.page < 1) out.page = 1;
    if (out.page_size < 1 or out.page_size > MAX_PAGE_SIZE) return null;
    if (qs.get("sort")) |s| out.order_sql = sortClauseFor(s) orelse return null;
    return out;
}

pub fn innerListFleetRunners(hx: Hx, req: *httpz.Request) void {
    const q = parseListQuery(req) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "page_size must be between 1 and 100; sort must be one of created_at|-created_at|host_id|-host_id");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    const items = fetchPage(hx, conn, q, now_ms) orelse return;
    const total = fetchTotal(hx, conn) orelse return;

    hx.ok(.ok, .{
        .items = items,
        .total = total,
        .page = q.page,
        .page_size = q.page_size,
    });
}

fn fetchPage(hx: Hx, conn: anytype, q: ListQuery, now_ms: i64) ?[]RunnerItem {
    const offset: i64 = @as(i64, q.page - 1) * @as(i64, q.page_size);
    const limit: i64 = q.page_size;
    // order_sql is from sortClauseFor's fixed allowlist, never user input.
    const list_sql = std.fmt.allocPrint(hx.alloc,
        "SELECT r.id::text, r.host_id, r.sandbox_tier, r.labels::text, r.last_seen_at, r.created_at, " ++
        "EXISTS (SELECT 1 FROM fleet.runner_leases l WHERE l.runner_id = r.id " ++
        "AND l.status = $1 AND l.lease_expires_at > $2) " ++
        "FROM fleet.runners r ORDER BY {s} LIMIT $3 OFFSET $4", .{q.order_sql}) catch {
        common.internalOperationError(hx.res, "Query build failed", hx.req_id);
        return null;
    };
    var rows_q = PgQuery.from(conn.query(list_sql, .{ protocol.RUNNER_LEASE_STATUS_ACTIVE, now_ms, limit, offset }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    });
    defer rows_q.deinit();

    var items: std.ArrayListUnmanaged(RunnerItem) = .{};
    // A mid-iteration row-fetch transport error is a different failure class than
    // a single undecodable row: it must surface as a 500, never a silent partial
    // page (which would disagree with fetchTotal's independent COUNT).
    while (rows_q.next() catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    }) |row| {
        const item = readItem(hx.alloc, row, now_ms) catch |err| {
            log.warn("row_decode_skipped", .{ .err = @errorName(err) });
            continue;
        };
        items.append(hx.alloc, item) catch {
            common.internalOperationError(hx.res, MSG_OUT_OF_MEMORY, hx.req_id);
            return null;
        };
    }
    return items.toOwnedSlice(hx.alloc) catch {
        common.internalOperationError(hx.res, MSG_OUT_OF_MEMORY, hx.req_id);
        return null;
    };
}

/// Build one item, duping borrowed row slices into the request arena (they
/// outlive `rows_q.deinit()`) and parsing the labels JSONB. `token_hash` and the
/// stored `status` are deliberately absent.
fn readItem(alloc: std.mem.Allocator, row: anytype, now_ms: i64) !RunnerItem {
    const last_seen_at = try row.get(i64, 4);
    return .{
        .id = try alloc.dupe(u8, try row.get([]u8, 0)),
        .host_id = try alloc.dupe(u8, try row.get([]u8, 1)),
        .sandbox_tier = try alloc.dupe(u8, try row.get([]u8, 2)),
        .labels = parseLabels(alloc, try row.get([]u8, 3)),
        .last_seen_at = last_seen_at,
        .created_at = try row.get(i64, 5),
        .liveness = deriveLiveness(last_seen_at, try row.get(bool, 6), now_ms),
    };
}

/// Parse the stored labels JSONB (a JSON array of strings) into owned slices.
/// A malformed value degrades to an empty set rather than failing the read.
fn parseLabels(alloc: std.mem.Allocator, text: []const u8) []const []const u8 {
    return std.json.parseFromSliceLeaky([]const []const u8, alloc, text, .{ .allocate = .alloc_always }) catch &.{};
}

fn fetchTotal(hx: Hx, conn: anytype) ?i64 {
    var q = PgQuery.from(conn.query(
        \\SELECT COUNT(*)::bigint FROM fleet.runners
    , .{}) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    });
    defer q.deinit();
    // COUNT(*) always yields exactly one row; any error or absent row is a real
    // DB failure → 500, not a fabricated total derived from the page length.
    const row = (q.next() catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    }) orelse {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    };
    return row.get(i64, 0) catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    };
}

// ── Tests (pure liveness derivation; no DB) ──────────────────────────────────

test "deriveLiveness: never-seen sentinel is registered regardless of lease" {
    const now: i64 = 1_000_000;
    try std.testing.expectEqual(protocol.RunnerLiveness.registered, deriveLiveness(protocol.RUNNER_LAST_SEEN_NEVER, false, now));
    // A never-seen runner can't actually hold a lease, but registered wins the
    // ordering either way — proves the sentinel is checked first.
    try std.testing.expectEqual(protocol.RunnerLiveness.registered, deriveLiveness(protocol.RUNNER_LAST_SEEN_NEVER, true, now));
}

test "deriveLiveness: a live lease is busy even when last_seen is stale" {
    const now: i64 = 10_000_000;
    const stale = now - constants.RUNNER_OFFLINE_AFTER_MS - 1; // would be offline without the lease
    try std.testing.expectEqual(protocol.RunnerLiveness.busy, deriveLiveness(stale, true, now));
}

test "deriveLiveness: fresh heartbeat without a lease is online; stale is offline" {
    const now: i64 = 10_000_000;
    const fresh = now - 1;
    const at_threshold = now - constants.RUNNER_OFFLINE_AFTER_MS; // inclusive → still online
    const stale = now - constants.RUNNER_OFFLINE_AFTER_MS - 1;
    try std.testing.expectEqual(protocol.RunnerLiveness.online, deriveLiveness(fresh, false, now));
    try std.testing.expectEqual(protocol.RunnerLiveness.online, deriveLiveness(at_threshold, false, now));
    try std.testing.expectEqual(protocol.RunnerLiveness.offline, deriveLiveness(stale, false, now));
}
