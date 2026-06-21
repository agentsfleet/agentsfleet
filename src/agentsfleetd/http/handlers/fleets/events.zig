//! GET /v1/workspaces/{ws}/fleets/{id}/events — operator-visible
//! event history for a single fleet. Cursor-paginated newest-first
//! over `core.fleet_events`. Optional filters: actor glob, since=
//! duration or RFC 3339 timestamp.
//!
//! `since=` and `cursor=` are mutually exclusive — supplying both is a
//! 400 with `since_and_cursor_mutually_exclusive`.

const std = @import("std");
const clock = @import("common").clock;
const httpz = @import("httpz");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const events_store = @import("../../../state/fleet_events_store.zig");

const log = logging.scoped(.http_fleet_events);

const LIMIT_DEFAULT: u32 = 50;
const LIMIT_MAX: u32 = 200;

pub fn innerListEvents(
    hx: hx_mod.Hx,
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

    const params = parseFilterParams(hx, qs) catch return;
    const filter = buildFilter(hx, params) catch return;
    defer if (filter.actor_like) |al| hx.alloc.free(al);

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const rows = events_store.listForFleet(conn, hx.alloc, workspace_id, fleet_id, filter) catch |err| {
        if (err == error.InvalidCursor) {
            hx.fail(ec.ERR_INVALID_REQUEST, "invalid cursor");
            return;
        }
        log.err("list_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err) });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    writeListResponse(hx, rows, filter.limit);
}

const FilterParams = struct {
    limit: u32,
    cursor: ?[]const u8,
    actor: ?[]const u8,
    actor_prefix: ?[]const u8,
    since_raw: ?[]const u8,
};

fn parseFilterParams(hx: hx_mod.Hx, qs: anytype) error{Failed}!FilterParams {
    const limit = parseLimit(qs) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "limit must be between 1 and 200");
        return error.Failed;
    };
    const cursor = qs.get("cursor");
    const actor = qs.get("actor");
    const actor_prefix = qs.get("actor_prefix");
    const since_raw = qs.get("since");

    if (cursor != null and since_raw != null) {
        hx.fail(ec.ERR_INVALID_REQUEST, "since_and_cursor_mutually_exclusive");
        return error.Failed;
    }
    if (actor != null and actor_prefix != null) {
        hx.fail(ec.ERR_INVALID_REQUEST, "actor_and_actor_prefix_mutually_exclusive");
        return error.Failed;
    }

    return .{ .limit = limit, .cursor = cursor, .actor = actor, .actor_prefix = actor_prefix, .since_raw = since_raw };
}

fn buildFilter(hx: hx_mod.Hx, params: FilterParams) error{Failed}!events_store.Filter {
    var since_ms: ?i64 = null;
    if (params.since_raw) |raw| {
        since_ms = events_store.parseSince(raw, clock.nowMillis()) catch {
            hx.fail(ec.ERR_INVALID_REQUEST, "invalid_since_format: use Go-style duration (15s, 30m, 2h, 7d) or RFC 3339 (YYYY-MM-DDTHH:MM:SSZ)");
            return error.Failed;
        };
    }

    var actor_like: ?[]u8 = null;
    if (params.actor) |a| {
        actor_like = events_store.globToLike(hx.alloc, a) catch {
            common.internalDbError(hx.res, hx.req_id);
            return error.Failed;
        };
    } else if (params.actor_prefix) |p| {
        // `actor_prefix=webhook:` ≡ `actor=webhook:*` — reuse globToLike's
        // %/_-escaping by appending `*` (the glob wildcard char) before
        // translation. Mutual exclusion enforced in parseFilterParams.
        actor_like = prefixToLike(hx.alloc, p) catch {
            common.internalDbError(hx.res, hx.req_id);
            return error.Failed;
        };
    }

    return events_store.Filter{
        .limit = params.limit,
        .cursor = params.cursor,
        .actor_like = actor_like,
        .since_ms = since_ms,
    };
}

test "prefixToLike: appends % wildcard" {
    const out = try prefixToLike(std.testing.allocator, "webhook:");
    defer std.testing.allocator.free(out);
    try std.testing.expectEqualStrings("webhook:%", out);
}

test "prefixToLike: escapes % and _ but not *" {
    const out = try prefixToLike(std.testing.allocator, "we_b%h*");
    defer std.testing.allocator.free(out);
    try std.testing.expectEqualStrings("we\\_b\\%h*%", out);
}

test "prefixToLike: empty prefix returns just %" {
    const out = try prefixToLike(std.testing.allocator, "");
    defer std.testing.allocator.free(out);
    try std.testing.expectEqualStrings("%", out);
}

/// Translate `<prefix>` to a SQL LIKE pattern by escaping `%` and `_` (so
/// they are not wildcards) and appending `%`. Distinct from globToLike's
/// `*`-to-`%` translation — under prefix mode the literal `*` matches a
/// literal `*`, never a wildcard.
fn prefixToLike(alloc: std.mem.Allocator, prefix: []const u8) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    for (prefix) |b| switch (b) {
        '%', '_' => {
            try out.append(alloc, '\\');
            try out.append(alloc, b);
        },
        else => try out.append(alloc, b),
    };
    try out.append(alloc, '%');
    return out.toOwnedSlice(alloc);
}

fn writeListResponse(hx: hx_mod.Hx, rows: []events_store.EventRow, limit: u32) void {
    const next_cursor: ?[]u8 = if (rows.len == limit and rows.len > 0) blk: {
        const last = rows[rows.len - 1];
        break :blk events_store.makeCursor(hx.alloc, last.created_at, last.event_id) catch {
            common.internalDbError(hx.res, hx.req_id);
            return;
        };
    } else null;

    hx.ok(.ok, .{ .items = rows, .next_cursor = next_cursor });
}

fn parseLimit(qs: anytype) !u32 {
    const raw = qs.get("limit") orelse return LIMIT_DEFAULT;
    const n = std.fmt.parseInt(u32, raw, 10) catch return error.InvalidLimit;
    if (n == 0 or n > LIMIT_MAX) return error.InvalidLimit;
    return n;
}
