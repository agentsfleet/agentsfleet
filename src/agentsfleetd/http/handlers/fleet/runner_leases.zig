//! GET /v1/fleets/runners/{runner_id}/leases — operator-plane lease history.
//!
//! Each lease arrives joined to its Fleet event, so outcome and failure cause
//! land in one round trip and the outcome is settled server-side into one
//! closed tag — two surfaces reading this list cannot drift on what expired
//! means. Keyset over the composite `(created_at, id)` key; `starting_after`
//! is a lease id this runner holds, resolved to its sort position before the
//! seek (the Stripe shape from `docs/REST_API_DESIGN_GUIDELINES.md` §3).

const std = @import("std");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const paging = @import("../../pagination.zig");
const sql = @import("sql.zig");
const assign = @import("../../../fleet/assign.zig");
const event_rows = @import("../../../fleet/event_rows.zig");
const protocol = @import("contract").protocol;
const id_format = @import("../../../types/id_format.zig");
const logging = @import("log");

const log = logging.scoped(.fleet_runner_leases);

const Hx = hx_mod.Hx;

const MSG_BAD_LIMIT = "limit must be an integer between 1 and 100";
const MSG_BAD_CURSOR = "starting_after must be a lease id held by this runner, and must match workspace_id and fleet when those filters are set";
const MSG_BAD_WORKSPACE = "workspace_id must be a workspace id";
const MSG_BAD_FLEET = "fleet must be a fleet id or name, at most 200 characters";
const QUERY_WORKSPACE_ID = "workspace_id";
const QUERY_FLEET = "fleet";
/// Bounds the fleet filter so an unbounded query string cannot reach the
/// comparison. `core.fleets.name` is far shorter than this in practice. Public
/// so the lease-read integration suite asserts the boundary against this value
/// rather than a copy that could drift from it.
pub const MAX_FLEET_FILTER_LEN = 200;

/// The one closed outcome vocabulary for a lease, derived here and nowhere
/// else. An expired lease reads expired regardless of how its event later
/// settled under the runner that reclaimed it, and a missing or non-terminal
/// event reads unknown — never a fabricated success.
pub const LeaseOutcome = enum { running, succeeded, failed, expired, unknown };

/// Wire shape per lease — a flat record. `request_json` and the raw lease
/// status have no field here, so neither can leave the server.
const LeaseItem = struct {
    id: []const u8,
    fleet_id: []const u8,
    fleet_name: ?[]const u8,
    workspace_id: []const u8,
    event_id: []const u8,
    event_type: []const u8,
    actor: []const u8,
    outcome: LeaseOutcome,
    failure_label: ?[]const u8,
    failure_detail: ?[]const u8,
    kind: assign.Kind,
    fencing_token: i64,
    provider: []const u8,
    model: []const u8,
    posture: []const u8,
    metered_input_tokens: i64,
    metered_cached_tokens: i64,
    metered_output_tokens: i64,
    wall_ms: ?i64,
    lease_expires_at: i64,
    created_at: i64,
};

pub fn innerListRunnerLeases(hx: Hx, req: *httpz.Request, runner_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, runner_id, "runner_id")) return;

    const qs = req.query() catch null;
    const limit = paging.parseLimit(if (qs) |q| q.get(paging.QUERY_LIMIT) else null) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, MSG_BAD_LIMIT);
        return;
    };
    const starting_after = if (qs) |q| q.get(paging.QUERY_STARTING_AFTER) else null;
    if (starting_after) |cursor| {
        if (!id_format.isUuidV7(cursor)) {
            hx.fail(ec.ERR_INVALID_REQUEST, MSG_BAD_CURSOR);
            return;
        }
    }
    // Optional ownership filters, independent and combinable. A malformed id is
    // refused; an unknown-but-well-formed one simply matches nothing (an empty
    // page, not an error).
    const workspace_id = if (qs) |q| q.get(QUERY_WORKSPACE_ID) else null;
    if (workspace_id) |ws| {
        if (!id_format.isUuidV7(ws)) {
            hx.fail(ec.ERR_INVALID_REQUEST, MSG_BAD_WORKSPACE);
            return;
        }
    }
    // Matched against a fleet id OR an exact fleet name, so the operator filters
    // by the name the table already shows rather than transcribing a UUID.
    const fleet = if (qs) |q| q.get(QUERY_FLEET) else null;
    if (fleet) |value| {
        if (value.len == 0 or value.len > MAX_FLEET_FILTER_LEN) {
            hx.fail(ec.ERR_INVALID_REQUEST, MSG_BAD_FLEET);
            return;
        }
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const total = fetchLeaseTotal(conn, runner_id, workspace_id, fleet) catch |err| {
        return failRead(hx, err);
    } orelse {
        hx.fail(ec.ERR_RUNNER_NOT_FOUND, "Runner not found");
        return;
    };

    var boundary_created_at: ?i64 = null;
    if (starting_after) |cursor| {
        boundary_created_at = resolveCursor(conn, cursor, runner_id, workspace_id, fleet) catch |err| {
            return failRead(hx, err);
        } orelse {
            hx.fail(ec.ERR_INVALID_REQUEST, MSG_BAD_CURSOR);
            return;
        };
    }

    const items = fetchLeasePage(conn, hx.alloc, runner_id, workspace_id, fleet, starting_after, boundary_created_at, limit) catch |err| {
        return failRead(hx, err);
    };

    const next_cursor: ?[]const u8 = if (items.len == limit and items.len > 0) items[items.len - 1].id else null;
    hx.ok(.ok, .{ .items = items, .total = total, .next_cursor = next_cursor });
}

fn failRead(hx: Hx, err: anyerror) void {
    log.err("runner_leases_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .req_id = hx.req_id });
    common.internalDbError(hx.res, hx.req_id);
}

/// Existence probe and page-stable total in one statement: null means the
/// runner id does not resolve. The workspace and fleet filters scope the total
/// to the same set the page returns.
fn fetchLeaseTotal(conn: anytype, runner_id: []const u8, workspace_id: ?[]const u8, fleet: ?[]const u8) !?i64 {
    var q = PgQuery.from(try conn.query(sql.SELECT_RUNNER_LEASE_TOTAL, .{ runner_id, workspace_id, fleet }));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    return try row.get(i64, 0);
}

/// Resolve `starting_after` to the boundary sort key; null means the lease id
/// is not one this runner holds *on the stream being paged* — the workspace and
/// fleet filters scope the cursor exactly as they scope the page, so a cursor
/// from outside the filtered stream is refused instead of seeking this page past
/// a boundary that was never on it.
fn resolveCursor(conn: anytype, lease_id: []const u8, runner_id: []const u8, workspace_id: ?[]const u8, fleet: ?[]const u8) !?i64 {
    var q = PgQuery.from(try conn.query(sql.SELECT_RUNNER_LEASE_CURSOR, .{ lease_id, runner_id, workspace_id, fleet }));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    return try row.get(i64, 0);
}

fn fetchLeasePage(
    conn: anytype,
    alloc: std.mem.Allocator,
    runner_id: []const u8,
    workspace_id: ?[]const u8,
    fleet: ?[]const u8,
    starting_after: ?[]const u8,
    boundary_created_at: ?i64,
    limit: u32,
) ![]LeaseItem {
    const limit_i64: i64 = @intCast(limit);
    var q = if (boundary_created_at) |created_at|
        PgQuery.from(try conn.query(sql.SELECT_RUNNER_LEASE_PAGE_AFTER, .{ runner_id, workspace_id, created_at, starting_after.?, limit_i64, fleet }))
    else
        PgQuery.from(try conn.query(sql.SELECT_RUNNER_LEASE_PAGE_FIRST, .{ runner_id, workspace_id, limit_i64, fleet }));
    defer q.deinit();
    return collectItems(alloc, &q);
}

/// Drain the page into owned items. A row that fails to decode is skipped and
/// logged — one bad row must not abort the page — but a mid-iteration
/// transport error propagates so the caller fails closed. `alloc` is the
/// request arena, so partial items on the error path die with the request.
fn collectItems(alloc: std.mem.Allocator, q: *PgQuery) ![]LeaseItem {
    var items: std.ArrayList(LeaseItem) = .empty;
    errdefer items.deinit(alloc);
    while (try q.next()) |row| {
        const item = readItem(alloc, row) catch |err| {
            log.warn("lease_row_decode_skipped", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err) });
            continue;
        };
        try items.append(alloc, item);
    }
    return items.toOwnedSlice(alloc);
}

fn dupeOptional(alloc: std.mem.Allocator, raw: ?[]const u8) !?[]const u8 {
    const value = raw orelse return null;
    return try alloc.dupe(u8, value);
}

fn readItem(alloc: std.mem.Allocator, row: anytype) !LeaseItem {
    // Scalars first (fallible, no allocation), then one errdefer per owned
    // slice — a decode error on a later column frees the earlier dupes.
    const lease_expires_at = try row.get(i64, 8);
    const created_at = try row.get(i64, 9);
    const fencing_token = try row.get(i64, 10);
    const metered_input_tokens = try row.get(i64, 14);
    const metered_cached_tokens = try row.get(i64, 15);
    const metered_output_tokens = try row.get(i64, 16);
    const wall_ms = try row.get(?i64, 20);
    const is_reclaim = try row.get(bool, 21);

    const lease_status = try row.get([]u8, 7);
    const event_status = try row.get(?[]u8, 17);
    const outcome = outcomeFor(lease_status, event_status);

    const id = try alloc.dupe(u8, try row.get([]u8, 0));
    errdefer alloc.free(id);
    const fleet_id = try alloc.dupe(u8, try row.get([]u8, 1));
    errdefer alloc.free(fleet_id);
    const fleet_name = try dupeOptional(alloc, try row.get(?[]u8, 2));
    errdefer if (fleet_name) |v| alloc.free(v);
    const workspace_id = try alloc.dupe(u8, try row.get([]u8, 3));
    errdefer alloc.free(workspace_id);
    const event_id = try alloc.dupe(u8, try row.get([]u8, 4));
    errdefer alloc.free(event_id);
    const event_type = try alloc.dupe(u8, try row.get([]u8, 5));
    errdefer alloc.free(event_type);
    const actor = try alloc.dupe(u8, try row.get([]u8, 6));
    errdefer alloc.free(actor);
    const provider = try alloc.dupe(u8, try row.get([]u8, 11));
    errdefer alloc.free(provider);
    const model = try alloc.dupe(u8, try row.get([]u8, 12));
    errdefer alloc.free(model);
    const posture = try alloc.dupe(u8, try row.get([]u8, 13));
    errdefer alloc.free(posture);
    const failure_label = try dupeOptional(alloc, try row.get(?[]u8, 18));
    errdefer if (failure_label) |v| alloc.free(v);
    const failure_detail = try dupeOptional(alloc, try row.get(?[]u8, 19));
    errdefer if (failure_detail) |v| alloc.free(v);

    return .{
        .id = id,
        .fleet_id = fleet_id,
        .fleet_name = fleet_name,
        .workspace_id = workspace_id,
        .event_id = event_id,
        .event_type = event_type,
        .actor = actor,
        .outcome = outcome,
        .failure_label = failure_label,
        .failure_detail = failure_detail,
        .kind = if (is_reclaim) .reclaim else .fresh,
        .fencing_token = fencing_token,
        .provider = provider,
        .model = model,
        .posture = posture,
        .metered_input_tokens = metered_input_tokens,
        .metered_cached_tokens = metered_cached_tokens,
        .metered_output_tokens = metered_output_tokens,
        .wall_ms = wall_ms,
        .lease_expires_at = lease_expires_at,
        .created_at = created_at,
    };
}

/// Pure outcome derivation. The lease's own status is consulted first, so a
/// reclaimed lease's successor outcome is never credited to the expired
/// holder; a stale `active` row reads running until reclaim marks it.
fn outcomeFor(lease_status: []const u8, event_status: ?[]const u8) LeaseOutcome {
    if (std.mem.eql(u8, lease_status, protocol.RUNNER_LEASE_STATUS_EXPIRED)) return .expired;
    if (std.mem.eql(u8, lease_status, protocol.RUNNER_LEASE_STATUS_ACTIVE)) return .running;
    if (std.mem.eql(u8, lease_status, protocol.RUNNER_LEASE_STATUS_REPORTED)) {
        const settled = event_status orelse return .unknown;
        if (std.mem.eql(u8, settled, event_rows.STATUS_PROCESSED)) return .succeeded;
        if (std.mem.eql(u8, settled, event_rows.STATUS_FLEET_ERROR)) return .failed;
        return .unknown;
    }
    return .unknown;
}

test "test_runner_leases_outcome_mapping" {
    // Every (lease status, event status) pair maps to exactly one tag.
    try std.testing.expectEqual(LeaseOutcome.succeeded, outcomeFor(protocol.RUNNER_LEASE_STATUS_REPORTED, event_rows.STATUS_PROCESSED));
    try std.testing.expectEqual(LeaseOutcome.failed, outcomeFor(protocol.RUNNER_LEASE_STATUS_REPORTED, event_rows.STATUS_FLEET_ERROR));
    try std.testing.expectEqual(LeaseOutcome.running, outcomeFor(protocol.RUNNER_LEASE_STATUS_ACTIVE, null));
    // A stale active lease still reads running — reclaim, not the clock, is
    // what moves it to expired.
    try std.testing.expectEqual(LeaseOutcome.running, outcomeFor(protocol.RUNNER_LEASE_STATUS_ACTIVE, event_rows.STATUS_PROCESSED));
    // The expired holder never inherits the successor's settled outcome.
    try std.testing.expectEqual(LeaseOutcome.expired, outcomeFor(protocol.RUNNER_LEASE_STATUS_EXPIRED, event_rows.STATUS_PROCESSED));
    try std.testing.expectEqual(LeaseOutcome.expired, outcomeFor(protocol.RUNNER_LEASE_STATUS_EXPIRED, null));
    // A reported lease with no event row, or a non-terminal event, is unknown.
    try std.testing.expectEqual(LeaseOutcome.unknown, outcomeFor(protocol.RUNNER_LEASE_STATUS_REPORTED, null));
    try std.testing.expectEqual(LeaseOutcome.unknown, outcomeFor(protocol.RUNNER_LEASE_STATUS_REPORTED, event_rows.STATUS_RECEIVED));
    try std.testing.expectEqual(LeaseOutcome.unknown, outcomeFor("mystery", null));
}

/// A row whose every column decodes, so the only failure the proof below can
/// induce is the allocator's. Column types follow what `readItem` asks for.
const ProofRow = struct {
    const TEXT: []const u8 = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";

    fn get(_: @This(), comptime T: type, _: usize) !T {
        return switch (T) {
            i64 => 1,
            bool => false,
            ?i64 => @as(?i64, 1),
            []u8 => @constCast(TEXT),
            ?[]u8 => @as(?[]u8, @constCast(TEXT)),
            else => @compileError("unhandled column type " ++ @typeName(T)),
        };
    }
};

fn freeProofItem(alloc: std.mem.Allocator, item: LeaseItem) void {
    alloc.free(item.id);
    alloc.free(item.fleet_id);
    if (item.fleet_name) |v| alloc.free(v);
    alloc.free(item.workspace_id);
    alloc.free(item.event_id);
    alloc.free(item.event_type);
    alloc.free(item.actor);
    alloc.free(item.provider);
    alloc.free(item.model);
    alloc.free(item.posture);
    if (item.failure_label) |v| alloc.free(v);
    if (item.failure_detail) |v| alloc.free(v);
}

fn readItemUnderAllocator(alloc: std.mem.Allocator) !void {
    freeProofItem(alloc, try readItem(alloc, ProofRow{}));
}

test "test_lease_row_read_unwinds_without_leaking" {
    // `readItem` frees twelve owned slices through an `errdefer` rung each, and
    // production hands it a request arena — so a missing rung leaks nothing
    // there and shows up only under the pooled allocator a long-running daemon
    // actually uses. Failing every site in turn is what proves each rung frees
    // what it claims to; reading the ladder and agreeing it looks right is not.
    try std.testing.checkAllAllocationFailures(std.testing.allocator, readItemUnderAllocator, .{});
}
