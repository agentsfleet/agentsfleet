//! GET /v1/workspaces/{ws}/fleets/{id}/events/{event_id} — one event, bodies
//! included.
//!
//! The expanded-row read. The list beside it renders a page and pays for every
//! column on every row; this one is asked for a single event and can afford the
//! trigger payload and the agent's full answer.
//!
//! 404 covers both "no such event" and "an event in another workspace" — the
//! store's statement carries the workspace predicate, so this handler receives
//! one null for both cases and could not distinguish them if it wanted to.

const std = @import("std");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const detail_store = @import("../../../state/fleet_event_detail_store.zig");

const log = logging.scoped(.http_fleet_event_detail);

const S_EVENT_NOT_FOUND = "Event not found";
/// `event_id` is TEXT on `core.fleet_events` — it arrives on the wire from the
/// producer rather than being minted here, so there is no shape to validate.
/// This bound only refuses an identifier long enough to be an attack on the
/// index rather than a lookup; a legitimate one is far shorter.
const EVENT_ID_MAX_LEN: usize = 256;

pub fn innerGetEvent(
    hx: hx_mod.Hx,
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(fleet_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "fleet_id must be a UUIDv7");
        return;
    }
    if (event_id.len == 0 or event_id.len > EVENT_ID_MAX_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "event_id is required");
        return;
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

    var row = (detail_store.getForFleet(conn, hx.alloc, workspace_id, fleet_id, event_id) catch |err| {
        log.err("event_detail_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err) });
        common.internalDbError(hx.res, hx.req_id);
        return;
    }) orelse {
        hx.fail(ec.ERR_EVENT_NOT_FOUND, S_EVENT_NOT_FOUND);
        return;
    };
    defer row.deinit(hx.alloc);

    hx.ok(.ok, row);
}

test "the not-found code is the event's own, not the fleet's" {
    // A missing event and a missing fleet are different facts, and an operator
    // reading the code should not have to guess which one the read refused on.
    try std.testing.expect(!std.mem.eql(u8, ec.ERR_EVENT_NOT_FOUND, ec.ERR_AGENTSFLEET_NOT_FOUND));
    try std.testing.expectEqual(std.http.Status.not_found, ec.lookup(ec.ERR_EVENT_NOT_FOUND).http_status);
}

test "the detail route does not shadow the live tail" {
    // `/events/stream` and `/events/{event_id}` are both six segments, and
    // `event_id` is free-form TEXT — nothing about its shape excludes the word
    // `stream`. Only router ORDER keeps them apart, and order is exactly what a
    // later edit reshuffles without noticing. Asserted here rather than over
    // HTTP: the stream route never closes its connection, so asking it a
    // question in a test suite hangs the suite.
    const router = @import("../../router.zig");
    const ws = "/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/fleets/0195b4ba-8d3a-7f13-8abc-2b3e1e0aa701/events/";

    const streamed = router.match(ws ++ "stream", .GET) orelse return error.StreamRouteUnreachable;
    try std.testing.expect(streamed == .workspace_fleet_events_stream);

    const detail = router.match(ws ++ "1785699668169-0", .GET) orelse return error.DetailRouteUnreachable;
    try std.testing.expect(detail == .workspace_fleet_event);
    try std.testing.expectEqualStrings("1785699668169-0", detail.workspace_fleet_event.event_id);
}
