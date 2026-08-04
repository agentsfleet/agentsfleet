//! The single-event read: bodies come back, and the workspace boundary holds.
//!
//! The boundary is the load-bearing half. This route is the only one that
//! serves an event body, and its identifier is caller-supplied TEXT rather than
//! a minted id — so a caller who learns an event identifier from anywhere could
//! ask any workspace for it. The answer must be a 404 that is byte-identical to
//! the one an unknown identifier gets, or the route becomes an existence oracle
//! for other tenants' traffic.
//!
//! Both fleets below are seeded under the SAME tenant, so the caller is
//! authorized at the tenant and the refusal can only come from the workspace
//! predicate inside the query. A 403 here would be a bug of a different kind:
//! it would confirm the event exists.
//!
//! Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;

const scope_fixtures = @import("../../test_scope_tokens.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const auth_mw = @import("../../../auth/middleware/mod.zig");
const id_format = @import("../../../types/id_format.zig");
const ec = @import("../../../errors/error_registry.zig");

const ALLOC = std.testing.allocator;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
/// A second workspace under the SAME tenant. The token is authorized there at
/// the tenant level, which is exactly what makes it a real boundary test.
const OTHER_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f33";
const TOKEN_VIEWER = scope_fixtures.VIEWER;

const REQUEST_BODY = "{\"message\":\"summarize the release notes\",\"source\":\"chat\"}";
const RESPONSE_BODY = "Shipped: the events list stopped carrying bodies.";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn seedWorkspaces(conn: *pg.Conn) !void {
    const now_ms = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'EventDetailTest', $2, $2) ON CONFLICT (id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    for ([_][]const u8{ TEST_WORKSPACE_ID, OTHER_WORKSPACE_ID }) |ws| {
        _ = try conn.exec(
            \\INSERT INTO workspaces (id, tenant_id, created_at)
            \\VALUES ($1::uuid, $2, $3) ON CONFLICT (id) DO NOTHING
        , .{ ws, TEST_TENANT_ID, now_ms });
    }
}

fn seedFleet(conn: *pg.Conn, workspace_id: []const u8, name_suffix: i64) ![]const u8 {
    const id = try id_format.generateFleetId(ALLOC);
    errdefer ALLOC.free(id);
    const name = try std.fmt.allocPrint(ALLOC, "event-detail-{d}", .{name_suffix});
    defer ALLOC.free(name);
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, tenant_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, (SELECT w.tenant_id FROM core.workspaces w WHERE w.id = $2::uuid),
        \\        $3, '# detail', '{}'::jsonb, 'active', $4, $4)
    , .{ id, workspace_id, name, clock.nowMillis() });
    return id;
}

/// Seed one event carrying both bodies. `event_id` is TEXT and producer-chosen,
/// so it is spelled here rather than minted.
fn seedEvent(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8, event_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, response_text, tokens, wall_ms, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, 'steer:tester', 'chat', 'processed',
        \\        $4::jsonb, $5, 128, 4200, $6, $6)
    , .{ fleet_id, event_id, workspace_id, REQUEST_BODY, RESPONSE_BODY, clock.nowMillis() });
}

fn purgeFleet(conn: *pg.Conn, fleet_id: []const u8) void {
    _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{fleet_id}) catch |err|
        std.log.warn("event-detail fixture purge ignored: {s}", .{@errorName(err)});
}

fn detailUrl(workspace_id: []const u8, fleet_id: []const u8, event_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(
        ALLOC,
        "/v1/workspaces/{s}/fleets/{s}/events/{s}",
        .{ workspace_id, fleet_id, event_id },
    );
}

test "integration: test_event_detail_returns_body_scoped_to_workspace" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedWorkspaces(conn);

    const stamp = clock.nowMillis();
    const mine = try seedFleet(conn, TEST_WORKSPACE_ID, stamp);
    defer ALLOC.free(mine);
    defer purgeFleet(conn, mine);
    const event_id = try std.fmt.allocPrint(ALLOC, "evt-detail-{d}", .{stamp});
    defer ALLOC.free(event_id);
    try seedEvent(conn, TEST_WORKSPACE_ID, mine, event_id);

    // The expanded read carries what the list does not.
    const url = try detailUrl(TEST_WORKSPACE_ID, mine, event_id);
    defer ALLOC.free(url);
    const r = try (try h.get(url).bearer(TOKEN_VIEWER)).send();
    defer r.deinit();
    try r.expectStatus(.ok);

    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, r.body, .{});
    defer parsed.deinit();
    const obj = parsed.value.object;
    try std.testing.expectEqualStrings(event_id, obj.get("event_id").?.string);
    try std.testing.expectEqualStrings(RESPONSE_BODY, obj.get("response_text").?.string);
    // The request body round-trips through jsonb, so compare on a field rather
    // than byte-for-byte — jsonb does not preserve key order or whitespace.
    try std.testing.expect(std.mem.indexOf(u8, obj.get("request_json").?.string, "summarize the release notes") != null);
    // Everything the list row already carried is here too, so an expanded row
    // never has to reconcile two different views of the same event.
    try std.testing.expectEqualStrings("processed", obj.get("status").?.string);
    try std.testing.expectEqualStrings("steer:tester", obj.get("actor").?.string);
    try std.testing.expectEqual(@as(i64, 128), obj.get("tokens").?.integer);
}

test "integration: test_event_detail_404s_unknown_and_cross_workspace_alike" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedWorkspaces(conn);

    const stamp = clock.nowMillis();
    const mine = try seedFleet(conn, TEST_WORKSPACE_ID, stamp);
    defer ALLOC.free(mine);
    defer purgeFleet(conn, mine);

    // An identifier that names no event, on a fleet the caller owns.
    const ghost_url = try detailUrl(TEST_WORKSPACE_ID, mine, "evt-does-not-exist");
    defer ALLOC.free(ghost_url);
    const ghost = try (try h.get(ghost_url).bearer(TOKEN_VIEWER)).send();
    defer ghost.deinit();
    try ghost.expectStatus(.not_found);
    try ghost.expectErrorCode(ec.ERR_EVENT_NOT_FOUND);

    // A REAL event, in another workspace of the same tenant, asked for through
    // the caller's own workspace. It exists; the answer must not say so.
    const theirs = try seedFleet(conn, OTHER_WORKSPACE_ID, stamp + 1);
    defer ALLOC.free(theirs);
    defer purgeFleet(conn, theirs);
    const secret_id = try std.fmt.allocPrint(ALLOC, "evt-secret-{d}", .{stamp});
    defer ALLOC.free(secret_id);
    try seedEvent(conn, OTHER_WORKSPACE_ID, theirs, secret_id);

    const cross_url = try detailUrl(TEST_WORKSPACE_ID, theirs, secret_id);
    defer ALLOC.free(cross_url);
    const cross = try (try h.get(cross_url).bearer(TOKEN_VIEWER)).send();
    defer cross.deinit();
    try cross.expectStatus(.not_found);
    try cross.expectErrorCode(ec.ERR_EVENT_NOT_FOUND);

    // Indistinguishable is the claim, so assert it directly: same status, same
    // code. A future 403 on the cross-workspace arm would confirm existence.
    try std.testing.expectEqual(ghost.status, cross.status);
    // And the body never leaks the stored answer on the refused path.
    try std.testing.expect(std.mem.indexOf(u8, cross.body, RESPONSE_BODY) == null);
}
