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
const affinity = @import("affinity.zig");
const vault = @import("../state/vault.zig");
const credential_key = @import("../fleet_runtime/credential_key.zig");
const crypto_primitives = @import("../secrets/crypto_primitives.zig");
const grant_lookup = @import("../state/integration_grant_lookup.zig");

const PROVIDER_GITHUB = shared.PROVIDER_GITHUB;

const ALLOC = std.testing.allocator;
const LARGE_BALANCE_NANOS: i64 = 1000000000000;

// UUIDv7 literals (version nibble 7, variant 8) so the schema's id CHECK passes.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6011";
const RUNNER_A_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6a01";
const RUNNER_B_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6b01";
const AGENTSFLEET_1_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6c01";
const AGENTSFLEET_2_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6c02";
const SESSION_1_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6d01";
const SESSION_2_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6d02";
const AFFINITY_1_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6e01";
const AFFINITY_2_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6e02";
const LEASE_OLD_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6f01";

const RUNNER_A_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "a" ** 64;
const RUNNER_B_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "b" ** 64;

const CONFIG_NO_GATES =
    \\{"name":"runner-cp-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;
const SOURCE_MD =
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

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{ .configureRegistry = configureRegistry });
}

// ── Seed helpers ────────────────────────────────────────────────────────────

fn seedRunner(conn: *pg.Conn, runner_id: []const u8, host_id: []const u8, token: []const u8) !void {
    const hash = api_key.sha256Hex(token);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ runner_id, host_id, hash[0..] });
}

fn seedActiveFleet(conn: *pg.Conn, fleet_id: []const u8, name: []const u8, session_id: []const u8) !void {
    try base.seedFleet(conn, fleet_id, WORKSPACE_ID, name, CONFIG_NO_GATES, SOURCE_MD);
    try base.seedFleetSession(conn, session_id, fleet_id, "{}");
}

fn seedAffinity(conn: *pg.Conn, affinity_id: []const u8, fleet_id: []const u8, last_runner_id: []const u8, fencing_seq: i64, leased_until: i64) !void {
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

fn seedActiveLease(conn: *pg.Conn, lease_id: []const u8, runner_id: []const u8, fleet_id: []const u8, fencing_token: i64) !void {
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

fn fundLargeBalance(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'runner-cp-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, LARGE_BALANCE_NANOS });
}

fn publishFreshEvent(h: *TestHarness, fleet_id: []const u8) !void {
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

const LeaseView = struct {
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

fn leaseAs(h: *TestHarness, token: []const u8) !LeaseView {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(token)).json("{}");
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    return parseLease(ALLOC, resp.body);
}

/// Lease as `token` and assert the issued lease's policy carries a non-empty
/// provider and the exact `expect_api_key`. Self-contained (no LeaseView dup) so
/// it leaves the shared parseLease path untouched.
fn expectLeasePolicyKey(h: *TestHarness, token: []const u8, expect_api_key: []const u8) !void {
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
/// the control plane resolved into tenant_providers reaches the budget the
/// runner receives, and drives the auto tool_window tiering (capabilities.md §4).
fn expectLeasePolicyContext(h: *TestHarness, token: []const u8, expect_cap: i64, expect_tool_window: i64, expect_model: []const u8) !void {
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

fn expectLeaseInstructions(h: *TestHarness, token: []const u8, expect_substr: []const u8) !void {
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

fn reportLease(h: *TestHarness, token: []const u8, lease_id: []const u8, fencing_token: u64) !harness_mod.Response {
    const body = try std.fmt.allocPrint(ALLOC,
        \\{{"lease_id":"{s}","event_id":"evt-seed-1","fencing_token":{d},"outcome":"processed","response_text":"done","tokens":10,"telemetry":{{"time_to_first_token_ms":5,"wall_ms":100}},"checkpoint":{{"last_event_id":"evt-seed-1","last_response":"done"}}}}
    , .{ lease_id, fencing_token });
    defer ALLOC.free(body);
    const req = try (try h.post(protocol.PATH_RUNNER_REPORTS).bearer(token)).json(body);
    return req.send();
}

fn leaseStatusIs(conn: *pg.Conn, lease_id: []const u8, expected: []const u8) !bool {
    var q = PgQuery.from(try conn.query("SELECT status FROM fleet.runner_leases WHERE id = $1::uuid", .{lease_id}));
    defer q.deinit();
    const row = try q.next() orelse return error.LeaseRowMissing;
    return std.mem.eql(u8, try row.get([]const u8, 0), expected);
}

fn leasedUntilOf(conn: *pg.Conn, fleet_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query("SELECT leased_until FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{fleet_id}));
    defer q.deinit();
    const row = try q.next() orelse return error.AffinityRowMissing;
    return row.get(i64, 0);
}

fn activeLeaseRunnerIs(conn: *pg.Conn, fleet_id: []const u8, runner_id: []const u8) !bool {
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
fn cleanupAll(h: *TestHarness, conn: *pg.Conn) void {
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
    // Sticky hint: fleet 2 prefers runner A (expired claim → still claimable,
    // sorts to the front of the candidate scan).
    try seedAffinity(conn, AFFINITY_2_ID, AGENTSFLEET_2_ID, RUNNER_A_ID, 0, 0);

    try publishFreshEvent(h, AGENTSFLEET_1_ID);
    try publishFreshEvent(h, AGENTSFLEET_2_ID);

    // Lease 1 → the sticky-preferred fleet 2.
    const first = try leaseAs(h, RUNNER_A_TOKEN);
    defer if (first.fleet_id) |z| ALLOC.free(z);
    try std.testing.expect(first.present);
    try std.testing.expectEqualStrings(AGENTSFLEET_2_ID, first.fleet_id.?);

    // Lease 2 → the other active fleet (sticky one is now claimed).
    const second = try leaseAs(h, RUNNER_A_TOKEN);
    defer if (second.fleet_id) |z| ALLOC.free(z);
    try std.testing.expect(second.present);
    try std.testing.expectEqualStrings(AGENTSFLEET_1_ID, second.fleet_id.?);

    // Lease 3 → no work; both fleets are claimed.
    const third = try leaseAs(h, RUNNER_A_TOKEN);
    defer if (third.fleet_id) |z| ALLOC.free(z);
    try std.testing.expect(!third.present);
}

test "integration: runner control plane — report with a stale fencing token is rejected, writes nothing" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn, RUNNER_A_ID, "runner-cp-a", RUNNER_A_TOKEN);
    try seedActiveFleet(conn, AGENTSFLEET_1_ID, "cp-fleet-1", SESSION_1_ID);
    try seedActiveLease(conn, LEASE_OLD_ID, RUNNER_A_ID, AGENTSFLEET_1_ID, 1);
    // The fleet's live fencing seq has advanced past this lease's token, as a
    // reclaim would leave it.
    try seedAffinity(conn, AFFINITY_1_ID, AGENTSFLEET_1_ID, RUNNER_A_ID, 2, clock.nowMillis() + 60_000);

    const resp = try reportLease(h, RUNNER_A_TOKEN, LEASE_OLD_ID, 1);
    defer resp.deinit();
    try resp.expectErrorCode("UZ-RUN-005");

    // State unchanged: the lease stays active (no finalize / settle ran).
    try std.testing.expect(try leaseStatusIs(conn, LEASE_OLD_ID, "active"));
}

test "integration: runner control plane — an expired lease is reclaimed and re-fenced with a higher token" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn, RUNNER_A_ID, "runner-cp-a", RUNNER_A_TOKEN); // dead holder
    try seedRunner(conn, RUNNER_B_ID, "runner-cp-b", RUNNER_B_TOKEN); // reclaimer
    try seedActiveFleet(conn, AGENTSFLEET_1_ID, "cp-fleet-1", SESSION_1_ID);
    // Dead holder A: an expired affinity (claimable) + an active lease that
    // carries the durable event envelope to re-lease.
    try seedAffinity(conn, AFFINITY_1_ID, AGENTSFLEET_1_ID, RUNNER_A_ID, 1, 0);
    try seedActiveLease(conn, LEASE_OLD_ID, RUNNER_A_ID, AGENTSFLEET_1_ID, 1);

    // B leases → reclaims A's event under a strictly higher token.
    const lv = try leaseAs(h, RUNNER_B_TOKEN);
    defer if (lv.fleet_id) |z| ALLOC.free(z);
    try std.testing.expect(lv.present);
    try std.testing.expectEqualStrings(AGENTSFLEET_1_ID, lv.fleet_id.?);
    try std.testing.expect(lv.fencing_token > 1);

    // A's old lease is retired.
    try std.testing.expect(try leaseStatusIs(conn, LEASE_OLD_ID, "expired"));

    // A's late report on the stale lease is fenced out.
    const rep = try reportLease(h, RUNNER_A_TOKEN, LEASE_OLD_ID, 1);
    defer rep.deinit();
    try rep.expectErrorCode("UZ-RUN-005");
}

test "integration: runner control plane — a fresh lease carries the resolved provider key on the policy" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    const KNOWN_KEY = "fw_lease_path_known_key";
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProviderWithKey(ALLOC, conn, WORKSPACE_ID, KNOWN_KEY);
    try fundLargeBalance(conn);
    try seedRunner(conn, RUNNER_A_ID, "runner-cp-a", RUNNER_A_TOKEN);
    try seedActiveFleet(conn, AGENTSFLEET_1_ID, "cp-fleet-1", SESSION_1_ID);
    try publishFreshEvent(h, AGENTSFLEET_1_ID);

    // The billed key (resolveActiveProvider) is the key the runner receives.
    try expectLeasePolicyKey(h, RUNNER_A_TOKEN, KNOWN_KEY);
}

test "integration: runner control plane — a reclaimed lease re-resolves and carries the provider key" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    const KNOWN_KEY = "fw_reclaim_path_known_key";
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProviderWithKey(ALLOC, conn, WORKSPACE_ID, KNOWN_KEY);
    try fundLargeBalance(conn);
    try seedRunner(conn, RUNNER_A_ID, "runner-cp-a", RUNNER_A_TOKEN); // dead holder
    try seedRunner(conn, RUNNER_B_ID, "runner-cp-b", RUNNER_B_TOKEN); // reclaimer
    try seedActiveFleet(conn, AGENTSFLEET_1_ID, "cp-fleet-1", SESSION_1_ID);
    // Dead holder A: expired affinity (claimable) + active lease carrying the envelope.
    try seedAffinity(conn, AFFINITY_1_ID, AGENTSFLEET_1_ID, RUNNER_A_ID, 1, 0);
    try seedActiveLease(conn, LEASE_OLD_ID, RUNNER_A_ID, AGENTSFLEET_1_ID, 1);

    // Reclaim reuses prior billing, but the key was never persisted — issueLease
    // re-resolves it, so the reclaimed lease still authenticates (the named fix).
    try expectLeasePolicyKey(h, RUNNER_B_TOKEN, KNOWN_KEY);
}

test "integration: runner control plane — a fresh lease overlays the resolved context cap+model onto sentinel frontmatter" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    // The model + 1M cap the lease overlays come from the active platform_llm_keys
    // row (M100: resolvePlatformDefault sources model+cap live from that row, not
    // the tenant_providers snapshot or a compile-time constant). CONFIG_NO_GATES
    // carries no x-agentsfleet.context block, so the fleet's frontmatter cap/model
    // are the sentinels (0 / "") that the lease-time overlay fills.
    const OVERLAY_MODEL = "accounts/fireworks/models/kimi-k2.6";
    const OVERLAY_CAP_TOKENS = 1_000_000; // ≥ large tier → tool_window 30 (capabilities.md §4)
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProviderWithKey(ALLOC, conn, WORKSPACE_ID, "fw_overlay_path_key");
    // Pin a 1M cap on the live platform_llm_keys row (the fixture seeds the
    // mid-tier default cap) so the overlaid cap lands in a different tool_window
    // tier than the mid default — proving the tiering, not just the passthrough.
    // The row's model already equals OVERLAY_MODEL via the fixture.
    _ = try conn.exec(
        "UPDATE core.platform_llm_keys SET context_cap_tokens = $1 WHERE active = true",
        .{@as(i32, OVERLAY_CAP_TOKENS)},
    );
    try fundLargeBalance(conn);
    try seedRunner(conn, RUNNER_A_ID, "runner-cp-a", RUNNER_A_TOKEN);
    try seedActiveFleet(conn, AGENTSFLEET_1_ID, "cp-fleet-1", SESSION_1_ID);
    try publishFreshEvent(h, AGENTSFLEET_1_ID);

    // The sentinel frontmatter inherits the tenant cap+model, and the overlaid
    // 1M cap drives the auto tool_window to the large tier (30). Pre-fix this
    // lease shipped context_cap_tokens=0 / model="" / tool_window=20 (the mid
    // default), silently disabling L3 chunking — the drift this overlay closes.
    try expectLeasePolicyContext(h, RUNNER_A_TOKEN, OVERLAY_CAP_TOKENS, 30, OVERLAY_MODEL);
}

test "integration: runner control plane — a fresh lease carries the installed SKILL.md instructions" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProviderWithKey(ALLOC, conn, WORKSPACE_ID, "fw_instr_fresh_key");
    try fundLargeBalance(conn);
    try seedRunner(conn, RUNNER_A_ID, "runner-cp-a", RUNNER_A_TOKEN);
    try seedActiveFleet(conn, AGENTSFLEET_1_ID, "cp-fleet-1", SESSION_1_ID);
    try publishFreshEvent(h, AGENTSFLEET_1_ID);

    // The SKILL.md body (extracted from SOURCE_MD by FleetSession) rides the lease,
    // so the runner delivers the installed behaviour to NullClaw.
    try expectLeaseInstructions(h, RUNNER_A_TOKEN, "You are a control-plane test fleet.");
}

test "integration: runner control plane — a reclaimed lease keeps the installed SKILL.md instructions" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProviderWithKey(ALLOC, conn, WORKSPACE_ID, "fw_instr_reclaim_key");
    try fundLargeBalance(conn);
    try seedRunner(conn, RUNNER_A_ID, "runner-cp-a", RUNNER_A_TOKEN); // dead holder
    try seedRunner(conn, RUNNER_B_ID, "runner-cp-b", RUNNER_B_TOKEN); // reclaimer
    try seedActiveFleet(conn, AGENTSFLEET_1_ID, "cp-fleet-1", SESSION_1_ID);
    try seedAffinity(conn, AFFINITY_1_ID, AGENTSFLEET_1_ID, RUNNER_A_ID, 1, 0);
    try seedActiveLease(conn, LEASE_OLD_ID, RUNNER_A_ID, AGENTSFLEET_1_ID, 1);

    // Reclaim resolves the session through the same FleetSession path, so the
    // installed instructions still ride the re-issued lease.
    try expectLeaseInstructions(h, RUNNER_B_TOKEN, "You are a control-plane test fleet.");
}

test "integration: runner control plane — sticky routing is a hint, not ownership" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn, RUNNER_A_ID, "runner-cp-a", RUNNER_A_TOKEN); // sticky-preferred, unavailable
    try seedRunner(conn, RUNNER_B_ID, "runner-cp-b", RUNNER_B_TOKEN); // any eligible runner
    try seedActiveFleet(conn, AGENTSFLEET_1_ID, "cp-fleet-1", SESSION_1_ID);
    // Sticky preference is A, but A's claim has expired → B must still get it.
    try seedAffinity(conn, AFFINITY_1_ID, AGENTSFLEET_1_ID, RUNNER_A_ID, 1, 0);
    try seedActiveLease(conn, LEASE_OLD_ID, RUNNER_A_ID, AGENTSFLEET_1_ID, 1);

    const lv = try leaseAs(h, RUNNER_B_TOKEN);
    defer if (lv.fleet_id) |z| ALLOC.free(z);
    try std.testing.expect(lv.present);
    try std.testing.expectEqualStrings(AGENTSFLEET_1_ID, lv.fleet_id.?);

    // The new active lease belongs to B, not the sticky-preferred A.
    try std.testing.expect(try activeLeaseRunnerIs(conn, AGENTSFLEET_1_ID, RUNNER_B_ID));
}

test "integration: runner control plane — release is token-guarded: a superseded holder cannot free the live slot" {
    const h = try startHarness(ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn, RUNNER_A_ID, "runner-cp-a", RUNNER_A_TOKEN);
    try seedActiveFleet(conn, AGENTSFLEET_1_ID, "cp-fleet-1", SESSION_1_ID);
    // The live holder owns the slot at fencing_seq=2, claim valid into the future.
    const live_until = clock.nowMillis() + 60_000;
    try seedAffinity(conn, AFFINITY_1_ID, AGENTSFLEET_1_ID, RUNNER_A_ID, 2, live_until);

    // A superseded holder (token 1 < seq 2, as a reclaim would leave it) releases
    // → no-op: the slot stays held, leased_until unchanged.
    try affinity.release(conn, AGENTSFLEET_1_ID, 1);
    try std.testing.expectEqual(live_until, try leasedUntilOf(conn, AGENTSFLEET_1_ID));

    // The live holder (token == seq) releases → slot freed (leased_until → ~now).
    try affinity.release(conn, AGENTSFLEET_1_ID, 2);
    try std.testing.expect(try leasedUntilOf(conn, AGENTSFLEET_1_ID) < live_until);
}

// ── Grant-gated mintable classification at lease-issue ─────────

const CONFIG_GITHUB_CRED =
    \\{"name":"runner-cp-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"credentials":["github"],"budget":{"daily_dollars":5.0}}}
;
const CONFIG_STATIC_CRED =
    \\{"name":"runner-cp-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"credentials":["cpstatic"],"budget":{"daily_dollars":5.0}}}
;
const GRANT_CP_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6f02";
const STATIC_SENTINEL = "cp_static_sentinel";

fn seedFleetWithConfig(conn: *pg.Conn, fleet_id: []const u8, name: []const u8, session_id: []const u8, config: []const u8) !void {
    try base.seedFleet(conn, fleet_id, WORKSPACE_ID, name, config, SOURCE_MD);
    try base.seedFleetSession(conn, session_id, fleet_id, "{}");
}

fn seedVaultJson(conn: *pg.Conn, name: []const u8, json: []const u8) !void {
    const key_name = try credential_key.allocKeyName(ALLOC, name);
    defer ALLOC.free(key_name);
    try vault.storeJsonPlaintext(ALLOC, conn, WORKSPACE_ID, key_name, json);
}

fn setGithubGrant(conn: *pg.Conn, fleet_id: []const u8, status: grant_lookup.GrantStatus) !void {
    _ = try conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (uid, grant_id, fleet_id, service, status, requested_at, requested_reason)
        \\VALUES ($1::uuid, $1, $2::uuid, $3, $4, 0, 'cp lease-gate test')
        \\ON CONFLICT (fleet_id, service) DO UPDATE SET status = EXCLUDED.status
    , .{ GRANT_CP_ID, fleet_id, PROVIDER_GITHUB, status.toSlice() });
}

/// Lease as `token` and return the duped raw body for policy-level assertions.
fn leaseBodyAs(h: *TestHarness, token: []const u8) ![]u8 {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(token)).json("{}");
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    return ALLOC.dupe(u8, resp.body);
}

test "integration: test_lease_gates_mintable_on_grant" {
    // Grant-gate dimension 3.1 — a connected-but-ungranted mintable credential is
    // omitted from BOTH policy surfaces (`mintable` and `secrets_map`); the same
    // config with an approved grant emits the mintable. Two fleets share the
    // workspace's fleet:github handle; only the grant row differs.
    crypto_primitives.setTestKek();
    const h = try startHarness(ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProviderWithKey(ALLOC, conn, WORKSPACE_ID, "fw_gate_key");
    try fundLargeBalance(conn);
    try seedRunner(conn, RUNNER_A_ID, "runner-cp-a", RUNNER_A_TOKEN);
    try seedFleetWithConfig(conn, AGENTSFLEET_1_ID, "cp-gate-ungranted", SESSION_1_ID, CONFIG_GITHUB_CRED);
    try seedFleetWithConfig(conn, AGENTSFLEET_2_ID, "cp-gate-granted", SESSION_2_ID, CONFIG_GITHUB_CRED);
    try seedVaultJson(conn, PROVIDER_GITHUB, "{\"integration\":\"github\",\"installation_id\":\"42\"}");
    try setGithubGrant(conn, AGENTSFLEET_2_ID, .approved); // fleet 1 stays ungranted
    try publishFreshEvent(h, AGENTSFLEET_1_ID);
    try publishFreshEvent(h, AGENTSFLEET_2_ID);

    // Two leases (assignment order is the scheduler's); assert per fleet id.
    var checked_ungranted = false;
    var checked_granted = false;
    for (0..2) |_| {
        const body = try leaseBodyAs(h, RUNNER_A_TOKEN);
        defer ALLOC.free(body);
        const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, body, .{});
        defer parsed.deinit();
        const lease = parsed.value.object.get("lease").?.object;
        const fleet_id = lease.get("event").?.object.get("fleet_id").?.string;
        const policy = lease.get("policy").?.object;
        const mintable = policy.get("mintable").?.array;
        const secrets_map = policy.get("secrets_map").?;
        if (std.mem.eql(u8, fleet_id, AGENTSFLEET_1_ID)) {
            // Ungranted: no mintable emitted AND no handle leaked into secrets_map.
            try std.testing.expectEqual(@as(usize, 0), mintable.items.len);
            if (secrets_map == .object) try std.testing.expect(secrets_map.object.get(PROVIDER_GITHUB) == null);
            try std.testing.expect(std.mem.indexOf(u8, body, "installation_id") == null);
            checked_ungranted = true;
        } else {
            // Granted: the mintable rides the policy, id-only.
            try std.testing.expectEqual(@as(usize, 1), mintable.items.len);
            try std.testing.expectEqualStrings(PROVIDER_GITHUB, mintable.items[0].object.get("integration").?.string);
            checked_granted = true;
        }
    }
    try std.testing.expect(checked_ungranted);
    try std.testing.expect(checked_granted);
}

test "integration: test_static_secrets_unaffected_by_grant_gate" {
    // Grant-gate dimension 3.2 — a static custom secret (no `integration` field)
    // resolves into secrets_map exactly as before, with zero grant rows present.
    crypto_primitives.setTestKek();
    const h = try startHarness(ALLOC);
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProviderWithKey(ALLOC, conn, WORKSPACE_ID, "fw_gate_key2");
    try fundLargeBalance(conn);
    try seedRunner(conn, RUNNER_A_ID, "runner-cp-a", RUNNER_A_TOKEN);
    try seedFleetWithConfig(conn, AGENTSFLEET_1_ID, "cp-gate-static", SESSION_1_ID, CONFIG_STATIC_CRED);
    try seedVaultJson(conn, "cpstatic", "{\"api_token\":\"" ++ STATIC_SENTINEL ++ "\"}");
    try publishFreshEvent(h, AGENTSFLEET_1_ID);

    const body = try leaseBodyAs(h, RUNNER_A_TOKEN);
    defer ALLOC.free(body);
    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, body, .{});
    defer parsed.deinit();
    const lease = parsed.value.object.get("lease").?.object;
    const policy = lease.get("policy").?.object;
    try std.testing.expectEqual(@as(usize, 0), policy.get("mintable").?.array.items.len);
    const cpstatic = policy.get("secrets_map").?.object.get("cpstatic").?.object;
    try std.testing.expectEqualStrings(STATIC_SENTINEL, cpstatic.get("api_token").?.string);
}
