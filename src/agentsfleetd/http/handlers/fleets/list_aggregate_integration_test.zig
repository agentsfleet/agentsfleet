// HTTP integration test for the denormalized list counters (M131 §8).
//
// `events_processed` / `budget_used_nanos` used to be aggregated over the child
// tables on every read (~1.8s at scale); §8 denormalized them onto core.fleets,
// maintained by the migration-030 triggers. This test pins the OBSERVABLE
// contract: after seeding events/telemetry directly, the triggers have kept the
// fleet's counters in step with the child rows, and a fleet with zero children
// reports 0 (NOT NULL DEFAULT 0), never null. Because the triggers fire on the
// raw INSERT seeds, no counter is set by hand here — the DB maintains them.
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
// A dedicated workspace so the list result is exactly this suite's fleets —
// list is workspace-scoped, and a shared workspace would mix in siblings' rows.
const AGG_WORKSPACE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f33";
const TOKEN = scope_fixtures.TENANT_ADMIN;

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
        \\VALUES ($1, 'ListAggTest', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
    , .{ AGG_WORKSPACE, TEST_TENANT_ID, now_ms });
    // Clean slate — this workspace belongs to this suite alone.
    _ = conn.exec("DELETE FROM core.fleet_execution_telemetry WHERE workspace_id = $1", .{AGG_WORKSPACE}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.fleet_events WHERE workspace_id = $1::uuid", .{AGG_WORKSPACE}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.fleets WHERE workspace_id = $1::uuid", .{AGG_WORKSPACE}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
}

fn seedFleet(alloc: std.mem.Allocator, conn: *pg.Conn, name: []const u8, now_ms: i64) ![]const u8 {
    const id = try id_format.generateFleetId(alloc);
    errdefer alloc.free(id);
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'seed', '{}'::jsonb, 'active', $4, $4)
    , .{ id, AGG_WORKSPACE, name, now_ms });
    return id;
}

fn addEvent(conn: *pg.Conn, fleet_id: []const u8, event_id: []const u8, ts: i64) !void {
    var uid_buf: [36]u8 = undefined;
    const uid = try id_format.formatUuidV7(&uid_buf);
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (uid, fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4::uuid, 'cron:x', 'cron', 'processed', '{}'::jsonb, $5, $5)
    , .{ uid, fleet_id, event_id, AGG_WORKSPACE, ts });
}

fn addTelemetry(conn: *pg.Conn, fleet_id: []const u8, event_id: []const u8, charge: []const u8, nanos: i64, ts: i64) !void {
    var uid_buf: [36]u8 = undefined;
    const uid = try id_format.formatUuidV7(&uid_buf);
    var id_buf: [80]u8 = undefined;
    const id = try std.fmt.bufPrint(&id_buf, "{s}-{s}", .{ event_id, charge });
    _ = try conn.exec(
        \\INSERT INTO core.fleet_execution_telemetry
        \\  (uid, id, tenant_id, workspace_id, fleet_id, event_id, charge_type,
        \\   posture, model, credit_deducted_nanos, recorded_at)
        \\VALUES ($1::uuid, $2, $3::uuid, $4, $5, $6, $7, 'platform', 'claude', $8, $9)
    , .{ uid, id, TEST_TENANT_ID, AGG_WORKSPACE, fleet_id, event_id, charge, nanos, ts });
}

fn findFleet(items: std.json.Array, id: []const u8) ?std.json.ObjectMap {
    for (items.items) |item| {
        if (std.mem.eql(u8, item.object.get("id").?.string, id)) return item.object;
    }
    return null;
}

test "integration: list counters match children; a bare fleet reads 0 not null; renewal delta tracked" {
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

    // Fleet BUSY: 3 events, telemetry summing to 1500 nanos.
    const busy = try seedFleet(alloc, conn, "agg-busy", now_ms);
    defer alloc.free(busy);
    try addEvent(conn, busy, "agg-e1", now_ms);
    try addEvent(conn, busy, "agg-e2", now_ms + 1);
    try addEvent(conn, busy, "agg-e3", now_ms + 2);
    try addTelemetry(conn, busy, "agg-e1", "receive", 500, now_ms);
    // pin test: literal is the contract (500 + 1000 + 250 renewal = 1750, asserted below)
    try addTelemetry(conn, busy, "agg-e1", "stage", 1000, now_ms);
    // Renewal accumulation: the stage row's credit grows post-execution (the
    // production upsert does `credit = credit + EXCLUDED`). The budget trigger
    // must add the +250 delta, not miss it — so the total becomes 1750, not 1500.
    _ = try conn.exec(
        \\UPDATE core.fleet_execution_telemetry SET credit_deducted_nanos = credit_deducted_nanos + 250
        \\WHERE fleet_id = $1 AND event_id = 'agg-e1' AND charge_type = 'stage'
    , .{busy});

    // Fleet BARE: no events, no telemetry — the LEFT JOIN miss must COALESCE to 0.
    const bare = try seedFleet(alloc, conn, "agg-bare", now_ms + 5);
    defer alloc.free(bare);

    const url = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/fleets?limit=100", .{AGG_WORKSPACE});
    defer alloc.free(url);
    const r = try (try h.get(url).bearer(TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, r.body, .{});
    defer parsed.deinit();
    const items = parsed.value.object.get("items").?.array;

    const busy_obj = findFleet(items, busy) orelse return error.BusyMissing;
    try std.testing.expectEqual(@as(i64, 3), busy_obj.get("events_processed").?.integer);
    // 500 (receive) + 1000 (stage) + 250 (renewal delta on the stage row) = 1750.
    try std.testing.expectEqual(@as(i64, 1750), busy_obj.get("budget_used_nanos").?.integer);

    const bare_obj = findFleet(items, bare) orelse return error.BareMissing;
    // The load-bearing assertion: an aggregate miss is 0, never null — a brand-new
    // fleet's counters must not vanish on the wall.
    try std.testing.expectEqual(@as(i64, 0), bare_obj.get("events_processed").?.integer);
    try std.testing.expectEqual(@as(i64, 0), bare_obj.get("budget_used_nanos").?.integer);

    _ = conn.exec("DELETE FROM core.fleet_execution_telemetry WHERE workspace_id = $1", .{AGG_WORKSPACE}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.fleet_events WHERE workspace_id = $1::uuid", .{AGG_WORKSPACE}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
    _ = conn.exec("DELETE FROM core.fleets WHERE workspace_id = $1::uuid", .{AGG_WORKSPACE}) catch |e| std.log.warn("cleanup ignored: {s}", .{@errorName(e)});
}
