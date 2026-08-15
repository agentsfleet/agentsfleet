//! Method and identifier refusals for the schedule routes.
//!
//! Both gates run ahead of authorization and the pool on every verb, which is
//! what makes them provable without a datastore — and worth proving, because
//! the identifier check names the field it rejected. A caller sending three
//! identifiers gets told which one is wrong; collapsing that to one generic
//! message is a silent regression that no status code would reveal.

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;

const api = @import("api.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");

const Hx = hx_mod.Hx;

const REQ_ID = "req-schedules-1";
const WORKSPACE_ID = "01932b7c-0000-7000-8000-0000000000e1";
const FLEET_ID = "01932b7c-0000-7000-8000-0000000000e2";
const SCHEDULE_ID = "01932b7c-0000-7000-8000-0000000000e3";
const BAD_ID = "not-a-uuidv7";
const K_ERROR_CODE = "error_code";
const K_DETAIL = "detail";
const STATUS_METHOD_NOT_ALLOWED: u16 = 405;

/// Every path under test returns at the method gate or the identifier gate,
/// both of which precede `hx.ctx` being read at all.
fn buildHx(res: *httpz.Response) Hx {
    return Hx{
        .alloc = testing.allocator,
        // SAFETY: authorization runs after both gates, past every return here.
        .principal = undefined,
        .req_id = REQ_ID,
        // SAFETY: as above.
        .ctx = undefined,
        .res = res,
    };
}

fn expectInvalidId(ht: *httpz.testing.Testing, label: []const u8) !void {
    var want: [96]u8 = undefined;
    const detail = try std.fmt.bufPrint(&want, "{s} must be a valid UUIDv7", .{label});
    const json = try ht.getJson();
    try testing.expectEqualStrings(ec.ERR_INVALID_REQUEST, json.object.get(K_ERROR_CODE).?.string);
    try testing.expectEqualStrings(detail, json.object.get(K_DETAIL).?.string);
}

// ── method gates ─────────────────────────────────────────────────────────

test "should refuse a verb the schedule collection does not serve" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.req.method = .PUT;

    api.innerScheduleCollection(buildHx(ht.res), ht.req, WORKSPACE_ID, FLEET_ID);

    try testing.expectEqual(STATUS_METHOD_NOT_ALLOWED, ht.res.status);
}

test "should refuse a verb the schedule item does not serve" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.req.method = .PUT;

    api.innerScheduleItem(buildHx(ht.res), ht.req, WORKSPACE_ID, FLEET_ID, SCHEDULE_ID);

    try testing.expectEqual(STATUS_METHOD_NOT_ALLOWED, ht.res.status);
}

test "should refuse anything but a POST to the sync route" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.req.method = .GET;

    // Sync provokes a provider call; a GET reaching it would make a mutation
    // reachable from a link or a prefetch.
    api.innerScheduleSync(buildHx(ht.res), ht.req, WORKSPACE_ID, FLEET_ID, SCHEDULE_ID);

    try testing.expectEqual(STATUS_METHOD_NOT_ALLOWED, ht.res.status);
}

// ── identifier gates, per field ──────────────────────────────────────────

test "should name the workspace id when it is the malformed one" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.req.method = .GET;

    api.innerScheduleCollection(buildHx(ht.res), ht.req, BAD_ID, FLEET_ID);

    try expectInvalidId(&ht, "workspace_id");
}

test "should name the fleet id when the workspace id is fine" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.req.method = .GET;

    api.innerScheduleCollection(buildHx(ht.res), ht.req, WORKSPACE_ID, BAD_ID);

    try expectInvalidId(&ht, "fleet_id");
}

test "should name the schedule id when only it is malformed" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.req.method = .GET;

    api.innerScheduleItem(buildHx(ht.res), ht.req, WORKSPACE_ID, FLEET_ID, BAD_ID);

    try expectInvalidId(&ht, "schedule_id");
}

test "should report the first malformed identifier, not the last" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.req.method = .GET;

    // All three are wrong. The order is left-to-right so the message is stable
    // for a caller fixing them one at a time.
    api.innerScheduleItem(buildHx(ht.res), ht.req, BAD_ID, BAD_ID, BAD_ID);

    try expectInvalidId(&ht, "workspace_id");
}

test "should validate identifiers on the delete verb too" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.req.method = .DELETE;

    // A delete that skipped validation would reach authorization with an
    // unvalidated identifier in hand.
    api.innerScheduleItem(buildHx(ht.res), ht.req, WORKSPACE_ID, FLEET_ID, BAD_ID);

    try expectInvalidId(&ht, "schedule_id");
}

test "should validate identifiers on the sync route before any provider work" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.req.method = .POST;

    api.innerScheduleSync(buildHx(ht.res), ht.req, WORKSPACE_ID, BAD_ID, SCHEDULE_ID);

    try expectInvalidId(&ht, "fleet_id");
}

// NOT TESTED HERE — `writeOutcome` and `serviceError`. Both take a value the
// cron service produces, and reaching one means running the service against
// Postgres and the QStash provider, so their arms belong to the integration
// suite rather than a fixture that fakes an outcome the service never emitted.
