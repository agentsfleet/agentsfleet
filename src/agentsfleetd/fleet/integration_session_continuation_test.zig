// Session-continuation integration over the runner control plane: the fleet's
// `core.fleet_sessions` checkpoint threads through a multi-stage lease, and a
// lease whose durable row was purged before renew is refused cleanly.
//
// Two journeys via the real HTTP router + runner_bearer middleware:
//
//   1. Continuation: a fleet carries a prior session checkpoint. A runner
//      leases the event, holds (renew), then reports — the report's checkpoint
//      becomes the session's new resume cursor, so `context_json` reflects the
//      final report state, not the seed. Session context survives the staged
//      lease > renew > report continuation.
//
//   2. Mid-lease unavailability: after a lease is issued, its durable
//      `runner_leases` row is purged (the fleet/session torn down out from under
//      the holder). A renew then loads no lease and is refused 404 UZ-RUN-006 —
//      no extend, no orphaned active lease left behind. (The purge removes the
//      durable `runner_leases` row directly — the row the renew path reads —
//      modeling the lease state vanishing while its fleet persists.)
//
// Mirrors control_plane_integration_test.zig's harness wiring + seed helpers.
// Requires LIVE_DB=1 + a reachable Redis; skipped when either is missing.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const serve_runner_lookup = @import("../cmd/serve_runner_lookup.zig");
const api_key = @import("../auth/api_key.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const harness_mod = @import("../http/test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const redis_fleet = @import("../queue/redis_fleet.zig");
const protocol = @import("contract").protocol;
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;
const LARGE_BALANCE_NANOS: i64 = 1000000000000;

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECK passes.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0de011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dea01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dec01";
const SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ded01";
const RUNNER_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "1" ** 64;

// A distinctive marker the seeded session checkpoint carries — the report must
// overwrite it, so its absence after report proves the cursor advanced.
const SEED_CONTEXT = "{\"last_event_id\":\"evt-prior\",\"last_response\":\"earlier-turn-marker\"}";

const CONFIG_NO_GATES =
    \\{"name":"session-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;
const SOURCE_MD =
    \\---
    \\name: session-bot
    \\---
    \\
    \\You are a session-continuation test fleet.
;

// SAFETY: populated by configureRegistry before the chain reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn startHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry });
}

// ── Seed helpers ────────────────────────────────────────────────────────────

fn seedRunner(conn: *pg.Conn) !void {
    const hash = api_key.sha256Hex(RUNNER_TOKEN);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'session-host', $2, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RUNNER_ID, hash[0..] });
}

fn seedActiveFleet(conn: *pg.Conn, context_json: []const u8) !void {
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "session-bot", CONFIG_NO_GATES, SOURCE_MD);
    try base.seedFleetSession(conn, SESSION_ID, FLEET_ID, context_json);
}

fn fundLargeBalance(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'session-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, LARGE_BALANCE_NANOS });
}

fn publishFreshEvent(h: *TestHarness) !void {
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, FLEET_ID);
    const id = try h.queue.xaddFleetEvent(.{
        .event_id = "",
        .fleet_id = FLEET_ID,
        .workspace_id = WORKSPACE_ID,
        .actor = "steer:test-user",
        .event_type = .chat,
        .request_json = "{\"message\":\"ping\"}",
        .created_at = clock.nowMillis(),
    });
    h.queue.alloc.free(id);
}

// ── HTTP + assertion helpers ──────────────────────────────────────────────────

const LeaseView = struct {
    present: bool,
    fencing_token: u64 = 0,
    lease_id: ?[]const u8 = null,
    event_id: ?[]const u8 = null,

    fn free(self: LeaseView) void {
        if (self.lease_id) |l| ALLOC.free(l);
        if (self.event_id) |e| ALLOC.free(e);
    }
};

fn parseLease(alloc: std.mem.Allocator, body: []const u8) !LeaseView {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    defer parsed.deinit();
    const lease = parsed.value.object.get("lease") orelse return .{ .present = false };
    if (lease == .null) return .{ .present = false };
    const obj = lease.object;
    const lease_id = try alloc.dupe(u8, obj.get("lease_id").?.string);
    errdefer alloc.free(lease_id);
    const event_id = try alloc.dupe(u8, obj.get("event").?.object.get("event_id").?.string);
    return .{
        .present = true,
        .fencing_token = @intCast(obj.get("fencing_token").?.integer),
        .lease_id = lease_id,
        .event_id = event_id,
    };
}

fn leaseOnce(h: *TestHarness) !LeaseView {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(RUNNER_TOKEN)).json("{}");
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    return parseLease(ALLOC, resp.body);
}

fn renewOnce(h: *TestHarness, lease_id: []const u8) !harness_mod.Response {
    const path = try std.fmt.allocPrint(ALLOC, "{s}/{s}/{s}", .{
        protocol.PATH_RUNNER_LEASES, lease_id, protocol.RUNNER_LEASE_RENEW_SUFFIX,
    });
    defer ALLOC.free(path);
    const req = try (try h.post(path).bearer(RUNNER_TOKEN)).json("{}");
    return req.send();
}

fn reportOnce(h: *TestHarness, lease_id: []const u8, event_id: []const u8, fencing_token: u64) !harness_mod.Response {
    const body = try std.fmt.allocPrint(ALLOC,
        \\{{"lease_id":"{s}","event_id":"{s}","fencing_token":{d},"outcome":"processed","response_text":"final-turn-marker","tokens":10,"telemetry":{{"time_to_first_token_ms":5,"wall_ms":100}},"checkpoint":{{"last_event_id":"{s}","last_response":"final-turn-marker"}}}}
    , .{ lease_id, event_id, fencing_token, event_id });
    defer ALLOC.free(body);
    const req = try (try h.post(protocol.PATH_RUNNER_REPORTS).bearer(RUNNER_TOKEN)).json(body);
    return req.send();
}

/// Read the session's resume cursor. alloc-dup'd; caller frees.
fn sessionContext(conn: *pg.Conn, alloc: std.mem.Allocator) ![]const u8 {
    var q = PgQuery.from(try conn.query("SELECT context_json::text FROM core.fleet_sessions WHERE fleet_id = $1::uuid", .{FLEET_ID}));
    defer q.deinit();
    const row = try q.next() orelse return error.SessionRowMissing;
    return alloc.dupe(u8, try row.get([]const u8, 0));
}

/// Count active leases still referencing the fleet — proves no orphan after a
/// purge.
fn activeLeaseCount(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT count(*)::bigint FROM fleet.runner_leases WHERE fleet_id = $1::uuid AND status = 'active'",
        .{FLEET_ID},
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.RowMissing;
    return row.get(i64, 0);
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn delStream(h: *TestHarness, comptime key: []const u8) void {
    var resp = h.queue.command(&.{ "DEL", key }) catch return;
    resp.deinit(h.queue.alloc);
}

fn cleanupAll(h: *TestHarness, conn: *pg.Conn) void {
    delStream(h, "fleet:" ++ FLEET_ID ++ ":events");
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    execIgnore(conn, "DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid", .{FLEET_ID});
    base.teardownPlatformProvider(conn, WORKSPACE_ID);
    base.teardownFleets(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
    base.teardownTenant(conn);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test "session context advances across a staged lease then renew then report" {
    const h = startHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProvider(ALLOC, conn, WORKSPACE_ID);
    try fundLargeBalance(conn);
    try seedRunner(conn);
    // Seed a PRIOR checkpoint so we can prove the report advances (not seeds) it.
    try seedActiveFleet(conn, SEED_CONTEXT);
    try publishFreshEvent(h);

    // The seed cursor is present before the run.
    const before = try sessionContext(conn, ALLOC);
    defer ALLOC.free(before);
    try std.testing.expect(std.mem.indexOf(u8, before, "earlier-turn-marker") != null);

    // lease > renew > report — the same session carried through every stage.
    const lv = try leaseOnce(h);
    defer lv.free();
    try std.testing.expect(lv.present);

    const renew_resp = try renewOnce(h, lv.lease_id.?);
    defer renew_resp.deinit();
    try renew_resp.expectStatus(.ok);

    const rep = try reportOnce(h, lv.lease_id.?, lv.event_id.?, lv.fencing_token);
    defer rep.deinit();
    try rep.expectStatus(.ok);

    // The report's checkpoint replaced the seed cursor — continuation persisted.
    const after = try sessionContext(conn, ALLOC);
    defer ALLOC.free(after);
    try std.testing.expect(std.mem.indexOf(u8, after, "final-turn-marker") != null);
    try std.testing.expect(std.mem.indexOf(u8, after, "earlier-turn-marker") == null);
}

test "renew is refused 404 when the lease was purged after issue, no orphan left" {
    const h = startHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProvider(ALLOC, conn, WORKSPACE_ID);
    try fundLargeBalance(conn);
    try seedRunner(conn);
    try seedActiveFleet(conn, "{}");
    try publishFreshEvent(h);

    const lv = try leaseOnce(h);
    defer lv.free();
    try std.testing.expect(lv.present);

    // The fleet/session is torn down mid-lease: purge the durable lease row the
    // renew path reads directly — not via a fleet delete, which would now cascade
    // the lease away through the fleet_id FK — so we remove exactly the lease
    // state the renew actually loads.
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE id = $1::uuid", .{lv.lease_id.?});

    // Renew now loads no lease → 404 UZ-RUN-006, no extend.
    const renew_resp = try renewOnce(h, lv.lease_id.?);
    defer renew_resp.deinit();
    try renew_resp.expectStatus(.not_found);
    try renew_resp.expectErrorCode("UZ-RUN-006");

    // No orphaned active lease remains for the fleet.
    try std.testing.expectEqual(@as(i64, 0), try activeLeaseCount(conn));
}
