// HTTP integration tests for per-event cost on the events row (M131 §2):
//   GET /v1/workspaces/{ws}/fleets/{id}/events  →  each item gains cost_nanos
//
// cost_nanos is the SUM of the event's telemetry rows' credit_deducted_nanos
// (billing writes up to two per event: receive + stage). The assertions that
// matter: the sum is over BOTH legs (never one), and an event with no
// telemetry carries null (never a fabricated zero).
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const id_format = @import("../../../types/id_format.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
// Distinct fleet id namespace so this suite's rows never collide with a sibling.
const COST_FLEET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0cccc1";
const TOKEN = scope_fixtures.TENANT_ADMIN;

const RECEIVE = "receive";
const STAGE = "stage";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn seedBase(conn: *pg.Conn, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'EventsCostTest', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'cost-fleet', 'seed', '{}'::jsonb, 'active', $3, $3)
        \\ON CONFLICT (id) DO NOTHING
    , .{ COST_FLEET, TEST_WORKSPACE_ID, now_ms });
    // A clean slate for this fleet's events so a re-run's row counts are stable.
    _ = conn.exec("DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid", .{COST_FLEET}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.fleet_execution_telemetry WHERE fleet_id = $1", .{COST_FLEET}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
}

fn insertEvent(conn: *pg.Conn, event_id: []const u8, ts: i64) !void {
    var uid_buf: [36]u8 = undefined;
    const uid = try id_format.formatUuidV7(&uid_buf);
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (uid, fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4::uuid, 'cron:x', 'cron', 'processed',
        \\        '{"m":"t"}'::jsonb, $5, $5)
    , .{ uid, COST_FLEET, event_id, TEST_WORKSPACE_ID, ts });
}

fn insertTelemetry(conn: *pg.Conn, event_id: []const u8, charge_type: []const u8, nanos: i64, ts: i64) !void {
    var uid_buf: [36]u8 = undefined;
    const uid = try id_format.formatUuidV7(&uid_buf);
    // `id` is the row's TEXT unique key — (event_id, charge_type) is unique by
    // construction (the telemetry table's own uniqueness axis), so this is stable.
    var id_buf: [64]u8 = undefined;
    const id = try std.fmt.bufPrint(&id_buf, "{s}-{s}", .{ event_id, charge_type });
    _ = try conn.exec(
        \\INSERT INTO core.fleet_execution_telemetry
        \\  (uid, id, tenant_id, workspace_id, fleet_id, event_id, charge_type,
        \\   posture, model, credit_deducted_nanos, recorded_at)
        \\VALUES ($1::uuid, $2, $3::uuid, $4, $5, $6, $7, 'platform', 'claude', $8, $9)
    , .{ uid, id, TEST_TENANT_ID, TEST_WORKSPACE_ID, COST_FLEET, event_id, charge_type, nanos, ts });
}

fn fetchEvents(h: *TestHarness, alloc: std.mem.Allocator) !std.json.Parsed(std.json.Value) {
    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets/{s}/events", .{ TEST_WORKSPACE_ID, COST_FLEET });
    defer alloc.free(url);
    const r = try (try h.get(url).bearer(TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    return std.json.parseFromSlice(std.json.Value, alloc, r.body, .{});
}

fn costOf(items: std.json.Array, event_id: []const u8) ?std.json.Value {
    for (items.items) |item| {
        if (std.mem.eql(u8, item.object.get("event_id").?.string, event_id)) {
            return item.object.get("cost_nanos").?;
        }
    }
    return null;
}

test "integration: event cost is the sum of both telemetry legs, null when none" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const now_ms = clock.nowMillis();
    try seedBase(conn, now_ms);

    // Event A: receive=300 + stage=700 → cost 1000 (the sum, not one leg).
    try insertEvent(conn, "evt-cost-a", now_ms);
    try insertTelemetry(conn, "evt-cost-a", RECEIVE, 300, now_ms);
    try insertTelemetry(conn, "evt-cost-a", STAGE, 700, now_ms);
    // Event B: no telemetry at all → cost null, still returned.
    try insertEvent(conn, "evt-cost-b", now_ms + 1);

    const parsed = try fetchEvents(h, alloc);
    defer parsed.deinit();
    const items = parsed.value.object.get("items").?.array;

    const a = costOf(items, "evt-cost-a") orelse return error.EventAMissing;
    // pin test: literal is the contract (300 receive + 700 stage = 1000)
    try std.testing.expectEqual(@as(i64, 1000), a.integer); // summed both legs, not 300 or 700

    const b = costOf(items, "evt-cost-b") orelse return error.EventBMissing;
    try std.testing.expect(b == .null); // no telemetry → null, never a fabricated zero

    _ = conn.exec("DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid", .{COST_FLEET}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.fleet_execution_telemetry WHERE fleet_id = $1", .{COST_FLEET}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
}
