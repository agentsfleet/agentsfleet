// Integration tests for the agentsfleetd-side runner control plane: lease
// assignment across active fleets, fencing-token verification at report,
// expiry-reclaim with a token bump, and sticky-routing-as-a-hint.
//
// Drives POST /v1/runners/me/leases and POST /v1/runners/me/reports through the
// in-process TestHarness against the live test DB + Redis. The harness's default
// runner lookup stubs to null (401); we wire the real DB-backed lookup and seed
// fleet.runners rows whose token_hash matches the presented agt_r token.
//
// Requires LIVE_DB=1 + a reachable Redis. Skipped when either is missing.

const std = @import("std");
const shared = @import("common");
const clock = shared.clock;
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

pub const ALLOC = std.testing.allocator;
const LARGE_BALANCE_NANOS: i64 = 1000000000000;

// UUIDv7 literals (version nibble 7, variant 8) so the schema's id CHECK passes.
pub const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6011";
pub const RUNNER_A_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6a01";
pub const RUNNER_B_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6b01";
pub const AGENTSFLEET_1_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6c01";
pub const AGENTSFLEET_2_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6c02";
pub const SESSION_1_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6d01";
pub const SESSION_2_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6d02";
pub const AFFINITY_1_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6e01";
pub const AFFINITY_2_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6e02";
pub const LEASE_OLD_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6f01";

pub const RUNNER_A_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "a" ** 64;
pub const RUNNER_B_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "b" ** 64;

pub const CONFIG_NO_GATES =
    \\{"name":"runner-cp-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;
pub const SOURCE_MD =
    \\---
    \\name: runner-cp-bot
    \\---
    \\
    \\You are a control-plane test fleet.
;

// The real DB-backed runner lookup. Parked at module scope so the value outlives
// the middleware chain; tests run sequentially in one process, so reassigning
// across harness starts is safe (each reassignment follows the prior deinit).
// SAFETY: populated by configureRegistry before the chain reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

pub fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{ .configureRegistry = configureRegistry });
}

// ── Seed helpers ────────────────────────────────────────────────────────────

pub fn seedRunner(conn: *pg.Conn, runner_id: []const u8, host_id: []const u8, token: []const u8) !void {
    const hash = api_key.sha256Hex(token);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ runner_id, host_id, hash[0..] });
}

pub fn seedActiveFleet(conn: *pg.Conn, fleet_id: []const u8, name: []const u8, session_id: []const u8) !void {
    try base.seedFleet(conn, fleet_id, WORKSPACE_ID, name, CONFIG_NO_GATES, SOURCE_MD);
    try base.seedFleetSession(conn, session_id, fleet_id, "{}");
}

pub fn seedAffinity(conn: *pg.Conn, affinity_id: []const u8, fleet_id: []const u8, last_runner_id: []const u8, fencing_seq: i64, leased_until: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, fleet_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (fleet_id) DO UPDATE
        \\  SET last_runner_id = EXCLUDED.last_runner_id,
        \\      fencing_seq = EXCLUDED.fencing_seq,
        \\      leased_until = EXCLUDED.leased_until
    , .{ affinity_id, fleet_id, last_runner_id, fencing_seq, leased_until });
}

pub fn seedActiveLease(conn: *pg.Conn, lease_id: []const u8, runner_id: []const u8, fleet_id: []const u8, fencing_token: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, 'evt-seed-1',
        \\        'steer:test', 'chat', '{"message":"hi"}', 0, 'platform',
        \\        'test-provider', 'test-model', 0, 0, 0, 0, $6, $7, 'active', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ lease_id, runner_id, fleet_id, WORKSPACE_ID, base.TEST_TENANT_ID, fencing_token, clock.nowMillis() + 60_000 });
}

pub fn fundLargeBalance(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'runner-cp-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, LARGE_BALANCE_NANOS });
}

pub fn publishFreshEvent(h: *TestHarness, fleet_id: []const u8) !void {
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, fleet_id);
    const id = try h.queue.xaddFleetEvent(.{
        .event_id = "",
        .fleet_id = fleet_id,
        .workspace_id = WORKSPACE_ID,
        .actor = "steer:test-user",
        .event_type = .chat,
        .request_json = "{\"message\":\"ping\"}",
        .created_at = clock.nowMillis(),
    });
    h.queue.alloc.free(id);
}

// ── HTTP + assertion helpers ──────────────────────────────────────────────────

pub const LeaseView = struct {
    present: bool,
    fencing_token: u64 = 0,
    /// alloc-dup'd; the caller frees. Null when no lease was issued.
    fleet_id: ?[]const u8 = null,
};

fn parseLease(alloc: std.mem.Allocator, body: []const u8) !LeaseView {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    defer parsed.deinit();
    const lease = parsed.value.object.get("lease") orelse return .{ .present = false };
    if (lease == .null) return .{ .present = false };
    const obj = lease.object;
    const zid = obj.get("event").?.object.get("fleet_id").?.string;
    return .{
        .present = true,
        .fencing_token = @intCast(obj.get("fencing_token").?.integer),
        .fleet_id = try alloc.dupe(u8, zid),
    };
}

pub fn leaseAs(h: *TestHarness, token: []const u8) !LeaseView {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(token)).json("{}");
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    return parseLease(ALLOC, resp.body);
}

/// Lease as `token` and assert the issued lease's policy carries a non-empty
/// provider and the exact `expect_api_key`. Self-contained (no LeaseView dup) so
/// it leaves the shared parseLease path untouched.
pub fn expectLeasePolicyKey(h: *TestHarness, token: []const u8, expect_api_key: []const u8) !void {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(token)).json("{}");
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, resp.body, .{});
    defer parsed.deinit();
    const lease = parsed.value.object.get("lease").?;
    try std.testing.expect(lease != .null);
    const policy = lease.object.get("policy").?.object;
    try std.testing.expect(policy.get("provider").?.string.len > 0);
    try std.testing.expectEqualStrings(expect_api_key, policy.get("api_key").?.string);
}

/// Lease as `token` and assert the issued lease's policy.context carries the
/// overlaid cap+model and the cap-derived auto tool_window. Proves the
/// lease-time tenant-provider overlay (see user_flow.md) end-to-end: the cap
/// the control plane resolved into tenant_model_selection reaches the budget the
/// runner receives, and drives the auto tool_window tiering (capabilities.md §4).
pub fn expectLeasePolicyContext(h: *TestHarness, token: []const u8, expect_cap: i64, expect_tool_window: i64, expect_model: []const u8) !void {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(token)).json("{}");
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, resp.body, .{});
    defer parsed.deinit();
    const lease = parsed.value.object.get("lease").?;
    try std.testing.expect(lease != .null);
    const ctx = lease.object.get("policy").?.object.get("context").?.object;
    try std.testing.expectEqual(expect_cap, ctx.get("context_cap_tokens").?.integer);
    try std.testing.expectEqual(expect_tool_window, ctx.get("tool_window").?.integer);
    try std.testing.expectEqualStrings(expect_model, ctx.get("model").?.string);
}

pub fn expectLeaseInstructions(h: *TestHarness, token: []const u8, expect_substr: []const u8) !void {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(token)).json("{}");
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, resp.body, .{});
    defer parsed.deinit();
    const lease = parsed.value.object.get("lease").?;
    try std.testing.expect(lease != .null);
    const instructions = lease.object.get("instructions").?.string;
    try std.testing.expect(std.mem.indexOf(u8, instructions, expect_substr) != null);
}

pub fn reportLease(h: *TestHarness, token: []const u8, lease_id: []const u8, fencing_token: u64) !harness_mod.Response {
    const body = try std.fmt.allocPrint(ALLOC,
        \\{{"lease_id":"{s}","event_id":"evt-seed-1","fencing_token":{d},"outcome":"processed","response_text":"done","tokens":10,"telemetry":{{"time_to_first_token_ms":5,"wall_ms":100}},"checkpoint":{{"last_event_id":"evt-seed-1","last_response":"done"}}}}
    , .{ lease_id, fencing_token });
    defer ALLOC.free(body);
    const req = try (try h.post(protocol.PATH_RUNNER_REPORTS).bearer(token)).json(body);
    return req.send();
}

pub fn leaseStatusIs(conn: *pg.Conn, lease_id: []const u8, expected: []const u8) !bool {
    var q = PgQuery.from(try conn.query("SELECT status FROM fleet.runner_leases WHERE id = $1::uuid", .{lease_id}));
    defer q.deinit();
    const row = try q.next() orelse return error.LeaseRowMissing;
    return std.mem.eql(u8, try row.get([]const u8, 0), expected);
}

pub fn leasedUntilOf(conn: *pg.Conn, fleet_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query("SELECT leased_until FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{fleet_id}));
    defer q.deinit();
    const row = try q.next() orelse return error.AffinityRowMissing;
    return row.get(i64, 0);
}

pub fn activeLeaseRunnerIs(conn: *pg.Conn, fleet_id: []const u8, runner_id: []const u8) !bool {
    var q = PgQuery.from(try conn.query(
        \\SELECT runner_id::text FROM fleet.runner_leases
        \\WHERE fleet_id = $1::uuid AND status = 'active'
        \\ORDER BY fencing_token DESC LIMIT 1
    , .{fleet_id}));
    defer q.deinit();
    const row = try q.next() orelse return error.NoActiveLease;
    return std.mem.eql(u8, try row.get([]const u8, 0), runner_id);
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn delStream(h: *TestHarness, comptime key: []const u8) void {
    var resp = h.queue.command(&.{ "DEL", key }) catch return;
    resp.deinit(h.queue.alloc);
}

/// Idempotent teardown of every fixture any test in this file seeds. Deletes are
/// no-ops when absent, so one routine serves all tests.
pub fn cleanupAll(h: *TestHarness, conn: *pg.Conn) void {
    delStream(h, "fleet:" ++ AGENTSFLEET_1_ID ++ ":events");
    delStream(h, "fleet:" ++ AGENTSFLEET_2_ID ++ ":events");
    execIgnore(conn, "DELETE FROM core.integration_grants WHERE fleet_id IN ($1::uuid, $2::uuid)", .{ AGENTSFLEET_1_ID, AGENTSFLEET_2_ID });
    execIgnore(conn, "DELETE FROM vault.secrets WHERE workspace_id = $1", .{WORKSPACE_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE runner_id IN ($1::uuid, $2::uuid)", .{ RUNNER_A_ID, RUNNER_B_ID });
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id IN ($1::uuid, $2::uuid)", .{ AGENTSFLEET_1_ID, AGENTSFLEET_2_ID });
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id IN ($1::uuid, $2::uuid)", .{ RUNNER_A_ID, RUNNER_B_ID });
    execIgnore(conn, "DELETE FROM core.fleet_events WHERE fleet_id IN ($1::uuid, $2::uuid)", .{ AGENTSFLEET_1_ID, AGENTSFLEET_2_ID });
    base.teardownPlatformProvider(conn, WORKSPACE_ID);
    base.teardownFleets(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
    base.teardownTenant(conn);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test "integration: runner control plane — lease assigns across active fleets, sticky-preferred first" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProvider(ALLOC, conn, WORKSPACE_ID);
    try fundLargeBalance(conn);
    try seedRunner(conn, RUNNER_A_ID, "runner-cp-a", RUNNER_A_TOKEN);
    try seedActiveFleet(conn, AGENTSFLEET_1_ID, "cp-fleet-1", SESSION_1_ID);
    try seedActiveFleet(conn, AGENTSFLEET_2_ID, "cp-fleet-2", SESSION_2_ID);
    try seedAffinity(conn, AFFINITY_2_ID, AGENTSFLEET_2_ID, RUNNER_A_ID, 0, 0);

    try publishFreshEvent(h, AGENTSFLEET_1_ID);
    try publishFreshEvent(h, AGENTSFLEET_2_ID);

    const first = try leaseAs(h, RUNNER_A_TOKEN);
    defer if (first.fleet_id) |z| ALLOC.free(z);
    try std.testing.expect(first.present);
    try std.testing.expectEqualStrings(AGENTSFLEET_2_ID, first.fleet_id.?);

    const second = try leaseAs(h, RUNNER_A_TOKEN);
    defer if (second.fleet_id) |z| ALLOC.free(z);
    try std.testing.expect(second.present);
    try std.testing.expectEqualStrings(AGENTSFLEET_1_ID, second.fleet_id.?);

    const third = try leaseAs(h, RUNNER_A_TOKEN);
    defer if (third.fleet_id) |z| ALLOC.free(z);
    try std.testing.expect(!third.present);
}
