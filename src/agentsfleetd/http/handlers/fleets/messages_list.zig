//! GET /v1/workspaces/{ws}/fleets/{id}/messages — the chat thread, bodies
//! included, one request.
//!
//! The chat view needs the newest N turns WITH `request_json` and
//! `response_text`; the events list deliberately omits bodies, so the view
//! used to fan out one detail request per turn. This read answers the whole
//! window in one statement (`fleet_event_detail_store.listThreadForFleet`),
//! keyset-paged newest-first with `starting_after`/`limit`.
//!
//! Pages are byte-budgeted, not byte-refused: rows join the page until the
//! encoded budget is spent, the FIRST row always ships (a single oversized
//! turn must not brick the thread), and `next_cursor` marks the cut — under
//! keyset semantics a short page with a cursor is a complete, truthful answer,
//! unlike the fixed-page reads that must refuse past their ceiling.
//!
//! A fleet id from another workspace yields an empty page — the workspace
//! predicate lives inside the statement, so existence is never disclosed,
//! mirroring the sibling events list.

const std = @import("std");
const httpz = @import("httpz");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const paging = @import("../../pagination.zig");
const response_size = @import("../../response_size.zig");
const detail_store = @import("../../../state/fleet_event_detail_store.zig");
const events_filter = @import("../../../state/fleet_events_filter.zig");

const log = logging.scoped(.http_fleet_messages);

const Hx = hx_mod.Hx;

const LIMIT_DEFAULT: u32 = 20;
/// Deliberately below the standard list caps — every row carries full bodies.
const LIMIT_MAX: u32 = 25;
/// Soft target for the encoded page. Rows stop joining once spent; the first
/// row is exempt so the newest turn always renders.
const THREAD_PAGE_BODY_BUDGET_BYTES: usize = 512 * 1024;

const MSG_INVALID_LIMIT = "limit must be between 1 and 25";
const MSG_INVALID_CURSOR = "invalid starting_after cursor";

pub fn innerListFleetMessages(
    hx: Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    fleet_id: []const u8,
) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(fleet_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "fleet_id must be a UUIDv7");
        return;
    }

    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "malformed query string");
        return;
    };
    const limit = parseLimit(qs) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, MSG_INVALID_LIMIT);
        return;
    };
    var after: ?events_filter.ParsedCursor = null;
    defer if (after) |a| hx.alloc.free(a.event_id);
    if (qs.get(paging.QUERY_STARTING_AFTER)) |raw| {
        after = events_filter.parseCursor(hx.alloc, raw) catch {
            hx.fail(ec.ERR_INVALID_REQUEST, MSG_INVALID_CURSOR);
            return;
        };
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    // Fetch one past the page so has-more is a fact, not a guess.
    const rows = detail_store.listThreadForFleet(
        conn,
        hx.alloc,
        workspace_id,
        fleet_id,
        if (after) |a| .{ .created_at = a.created_at, .event_id = a.event_id } else null,
        limit + 1,
    ) catch |err| {
        log.err("thread_list_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer detail_store.freeThreadRows(hx.alloc, rows);

    const included = includedUnderBudget(rows, limit) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    const items = rows[0..included];
    const has_more = rows.len > included;

    const next_cursor: ?[]u8 = if (has_more and included > 0) blk: {
        const last = items[included - 1];
        break :blk events_filter.makeCursor(hx.alloc, last.created_at, last.event_id) catch {
            common.internalDbError(hx.res, hx.req_id);
            return;
        };
    } else null;
    defer if (next_cursor) |nc| hx.alloc.free(nc);

    hx.ok(.ok, .{ .items = items, .total = null, .next_cursor = next_cursor });
}

/// How many leading rows fit the byte budget, capped at `limit`. The first
/// row is always included; measurement uses the same serialize options as the
/// response writer, so the budget tracks real encoded bytes.
fn includedUnderBudget(rows: []const detail_store.EventDetailRow, limit: u32) !usize {
    var included: usize = 0;
    var spent: usize = 0;
    for (rows) |row| {
        if (included >= limit) break;
        const row_bytes = try response_size.encoded(row, .{});
        if (included > 0 and spent + row_bytes > THREAD_PAGE_BODY_BUDGET_BYTES) break;
        spent += row_bytes;
        included += 1;
    }
    return included;
}

fn parseLimit(qs: anytype) !u32 {
    const raw = qs.get(paging.QUERY_LIMIT) orelse return LIMIT_DEFAULT;
    const n = std.fmt.parseInt(u32, raw, 10) catch return error.InvalidLimit;
    if (n == 0 or n > LIMIT_MAX) return error.InvalidLimit;
    return n;
}

test "includedUnderBudget always ships the first row and stops at the row cap" {
    const alloc = std.testing.allocator;
    // Three rows whose response_text alone dwarfs the budget: the first must
    // ship anyway; the second must not.
    const big = try alloc.alloc(u8, THREAD_PAGE_BODY_BUDGET_BYTES);
    defer alloc.free(big);
    @memset(big, 'a');
    var rows = [_]detail_store.EventDetailRow{ fixtureRow(big), fixtureRow(big), fixtureRow(big) };
    try std.testing.expectEqual(@as(usize, 1), try includedUnderBudget(&rows, 25));

    // Small rows are capped by the limit, not the budget.
    var small = [_]detail_store.EventDetailRow{ fixtureRow("hi"), fixtureRow("hi"), fixtureRow("hi") };
    try std.testing.expectEqual(@as(usize, 2), try includedUnderBudget(&small, 2));
    try std.testing.expectEqual(@as(usize, 3), try includedUnderBudget(&small, 25));
}

fn fixtureRow(response_text: []const u8) detail_store.EventDetailRow {
    return .{
        .fleet_id = @constCast("0195b4ba-8d3a-7f13-8abc-2b3e1e0aa701"),
        .event_id = @constCast("1700000000000-0"),
        .workspace_id = @constCast("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"),
        .actor = @constCast("steer:test"),
        .event_type = @constCast("chat"),
        .status = @constCast("processed"),
        .request_json = @constCast("{\"message\":\"hello\"}"),
        .response_text = @constCast(response_text),
        .tokens = null,
        .wall_ms = null,
        .failure_label = null,
        .failure_detail = null,
        .checkpoint_id = null,
        .resumes_event_id = null,
        .created_at = 1_700_000_000_000,
        .updated_at = 1_700_000_000_000,
        .cost_nanos = null,
    };
}
