// Integration tests for the zombied-side runner control plane: lease
// assignment across active zombies, fencing-token verification at report,
// expiry-reclaim with a token bump, and sticky-routing-as-a-hint.
//
// Drives POST /v1/runners/me/leases and POST /v1/runners/me/reports through the
// in-process TestHarness against the live test DB + Redis. The harness's default
// runner lookup stubs to null (401); we wire the real DB-backed lookup and seed
// fleet.runners rows whose token_hash matches the presented zrn_ token.
//
// Requires LIVE_DB=1 + a reachable Redis. Skipped when either is missing.

const std = @import("std");
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const serve_runner_lookup = @import("../cmd/serve_runner_lookup.zig");
const api_key = @import("../auth/api_key.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const harness_mod = @import("../http/test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const redis_zombie = @import("../queue/redis_zombie.zig");
const protocol = @import("contract").protocol;
const base = @import("../db/test_fixtures.zig");
const affinity = @import("affinity.zig");

const ALLOC = std.testing.allocator;

// UUIDv7 literals (version nibble 7, variant 8) so the schema's id CHECK passes.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6011";
const RUNNER_A_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6a01";
const RUNNER_B_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6b01";
const ZOMBIE_1_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6c01";
const ZOMBIE_2_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6c02";
const SESSION_1_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6d01";
const SESSION_2_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6d02";
const AFFINITY_1_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6e01";
const AFFINITY_2_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6e02";
const LEASE_OLD_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d6f01";

const RUNNER_A_TOKEN = "zrn_" ++ "a" ** 64;
const RUNNER_B_TOKEN = "zrn_" ++ "b" ** 64;

const CONFIG_NO_GATES =
    \\{"name":"runner-cp-bot","x-usezombie":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;
const SOURCE_MD =
    \\---
    \\name: runner-cp-bot
    \\---
    \\
    \\You are a control-plane test agent.
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
        \\  (id, host_id, token_hash, sandbox_tier, status, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ runner_id, host_id, hash[0..] });
}

fn seedActiveZombie(conn: *pg.Conn, zombie_id: []const u8, name: []const u8, session_id: []const u8) !void {
    try base.seedZombie(conn, zombie_id, WORKSPACE_ID, name, CONFIG_NO_GATES, SOURCE_MD);
    try base.seedZombieSession(conn, session_id, zombie_id, "{}");
}

fn seedAffinity(conn: *pg.Conn, affinity_id: []const u8, zombie_id: []const u8, last_runner_id: []const u8, fencing_seq: i64, leased_until: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, zombie_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (zombie_id) DO UPDATE
        \\  SET last_runner_id = EXCLUDED.last_runner_id,
        \\      fencing_seq = EXCLUDED.fencing_seq,
        \\      leased_until = EXCLUDED.leased_until
    , .{ affinity_id, zombie_id, last_runner_id, fencing_seq, leased_until });
}

fn seedActiveLease(conn: *pg.Conn, lease_id: []const u8, runner_id: []const u8, zombie_id: []const u8, fencing_token: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, zombie_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, 'evt-seed-1',
        \\        'steer:test', 'chat', '{"message":"hi"}', 0, 'platform',
        \\        'test-provider', 'test-model', 0, 0, 0, 0, $6, $7, 'active', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ lease_id, runner_id, zombie_id, WORKSPACE_ID, base.TEST_TENANT_ID, fencing_token, std.time.milliTimestamp() + 60_000 });
}

fn fundLargeBalance(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, 1000000000000, 'runner-cp-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{base.TEST_TENANT_ID});
}

fn publishFreshEvent(h: *TestHarness, zombie_id: []const u8) !void {
    try redis_zombie.ensureZombieConsumerGroup(&h.queue, zombie_id);
    const id = try h.queue.xaddZombieEvent(.{
        .event_id = "",
        .zombie_id = zombie_id,
        .workspace_id = WORKSPACE_ID,
        .actor = "steer:test-user",
        .event_type = .chat,
        .request_json = "{\"message\":\"ping\"}",
        .created_at = std.time.milliTimestamp(),
    });
    h.queue.alloc.free(id);
}

// ── HTTP + assertion helpers ──────────────────────────────────────────────────

const LeaseView = struct {
    present: bool,
    fencing_token: u64 = 0,
    /// alloc-dup'd; the caller frees. Null when no lease was issued.
    zombie_id: ?[]const u8 = null,
};

fn parseLease(alloc: std.mem.Allocator, body: []const u8) !LeaseView {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    defer parsed.deinit();
    const lease = parsed.value.object.get("lease") orelse return .{ .present = false };
    if (lease == .null) return .{ .present = false };
    const obj = lease.object;
    const zid = obj.get("event").?.object.get("zombie_id").?.string;
    return .{
        .present = true,
        .fencing_token = @intCast(obj.get("fencing_token").?.integer),
        .zombie_id = try alloc.dupe(u8, zid),
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

fn leasedUntilOf(conn: *pg.Conn, zombie_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query("SELECT leased_until FROM fleet.runner_affinity WHERE zombie_id = $1::uuid", .{zombie_id}));
    defer q.deinit();
    const row = try q.next() orelse return error.AffinityRowMissing;
    return row.get(i64, 0);
}

fn activeLeaseRunnerIs(conn: *pg.Conn, zombie_id: []const u8, runner_id: []const u8) !bool {
    var q = PgQuery.from(try conn.query(
        \\SELECT runner_id::text FROM fleet.runner_leases
        \\WHERE zombie_id = $1::uuid AND status = 'active'
        \\ORDER BY fencing_token DESC LIMIT 1
    , .{zombie_id}));
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
    delStream(h, "zombie:" ++ ZOMBIE_1_ID ++ ":events");
    delStream(h, "zombie:" ++ ZOMBIE_2_ID ++ ":events");
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE runner_id IN ($1::uuid, $2::uuid)", .{ RUNNER_A_ID, RUNNER_B_ID });
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE zombie_id IN ($1::uuid, $2::uuid)", .{ ZOMBIE_1_ID, ZOMBIE_2_ID });
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id IN ($1::uuid, $2::uuid)", .{ RUNNER_A_ID, RUNNER_B_ID });
    execIgnore(conn, "DELETE FROM core.zombie_events WHERE zombie_id IN ($1::uuid, $2::uuid)", .{ ZOMBIE_1_ID, ZOMBIE_2_ID });
    base.teardownPlatformProvider(conn, WORKSPACE_ID);
    base.teardownZombies(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
    base.teardownTenant(conn);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test "integration: runner control plane — lease assigns across active zombies, sticky-preferred first" {
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
    try seedActiveZombie(conn, ZOMBIE_1_ID, "cp-zombie-1", SESSION_1_ID);
    try seedActiveZombie(conn, ZOMBIE_2_ID, "cp-zombie-2", SESSION_2_ID);
    // Sticky hint: zombie 2 prefers runner A (expired claim → still claimable,
    // sorts to the front of the candidate scan).
    try seedAffinity(conn, AFFINITY_2_ID, ZOMBIE_2_ID, RUNNER_A_ID, 0, 0);

    try publishFreshEvent(h, ZOMBIE_1_ID);
    try publishFreshEvent(h, ZOMBIE_2_ID);

    // Lease 1 → the sticky-preferred zombie 2.
    const first = try leaseAs(h, RUNNER_A_TOKEN);
    defer if (first.zombie_id) |z| ALLOC.free(z);
    try std.testing.expect(first.present);
    try std.testing.expectEqualStrings(ZOMBIE_2_ID, first.zombie_id.?);

    // Lease 2 → the other active zombie (sticky one is now claimed).
    const second = try leaseAs(h, RUNNER_A_TOKEN);
    defer if (second.zombie_id) |z| ALLOC.free(z);
    try std.testing.expect(second.present);
    try std.testing.expectEqualStrings(ZOMBIE_1_ID, second.zombie_id.?);

    // Lease 3 → no work; both zombies are claimed.
    const third = try leaseAs(h, RUNNER_A_TOKEN);
    defer if (third.zombie_id) |z| ALLOC.free(z);
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
    try seedActiveLease(conn, LEASE_OLD_ID, RUNNER_A_ID, ZOMBIE_1_ID, 1);
    // The zombie's live fencing seq has advanced past this lease's token, as a
    // reclaim would leave it.
    try seedAffinity(conn, AFFINITY_1_ID, ZOMBIE_1_ID, RUNNER_A_ID, 2, std.time.milliTimestamp() + 60_000);

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
    try seedActiveZombie(conn, ZOMBIE_1_ID, "cp-zombie-1", SESSION_1_ID);
    // Dead holder A: an expired affinity (claimable) + an active lease that
    // carries the durable event envelope to re-lease.
    try seedAffinity(conn, AFFINITY_1_ID, ZOMBIE_1_ID, RUNNER_A_ID, 1, 0);
    try seedActiveLease(conn, LEASE_OLD_ID, RUNNER_A_ID, ZOMBIE_1_ID, 1);

    // B leases → reclaims A's event under a strictly higher token.
    const lv = try leaseAs(h, RUNNER_B_TOKEN);
    defer if (lv.zombie_id) |z| ALLOC.free(z);
    try std.testing.expect(lv.present);
    try std.testing.expectEqualStrings(ZOMBIE_1_ID, lv.zombie_id.?);
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
    try seedActiveZombie(conn, ZOMBIE_1_ID, "cp-zombie-1", SESSION_1_ID);
    try publishFreshEvent(h, ZOMBIE_1_ID);

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
    try seedActiveZombie(conn, ZOMBIE_1_ID, "cp-zombie-1", SESSION_1_ID);
    // Dead holder A: expired affinity (claimable) + active lease carrying the envelope.
    try seedAffinity(conn, AFFINITY_1_ID, ZOMBIE_1_ID, RUNNER_A_ID, 1, 0);
    try seedActiveLease(conn, LEASE_OLD_ID, RUNNER_A_ID, ZOMBIE_1_ID, 1);

    // Reclaim reuses prior billing, but the key was never persisted — issueLease
    // re-resolves it, so the reclaimed lease still authenticates (the named fix).
    try expectLeasePolicyKey(h, RUNNER_B_TOKEN, KNOWN_KEY);
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
    try seedActiveZombie(conn, ZOMBIE_1_ID, "cp-zombie-1", SESSION_1_ID);
    // Sticky preference is A, but A's claim has expired → B must still get it.
    try seedAffinity(conn, AFFINITY_1_ID, ZOMBIE_1_ID, RUNNER_A_ID, 1, 0);
    try seedActiveLease(conn, LEASE_OLD_ID, RUNNER_A_ID, ZOMBIE_1_ID, 1);

    const lv = try leaseAs(h, RUNNER_B_TOKEN);
    defer if (lv.zombie_id) |z| ALLOC.free(z);
    try std.testing.expect(lv.present);
    try std.testing.expectEqualStrings(ZOMBIE_1_ID, lv.zombie_id.?);

    // The new active lease belongs to B, not the sticky-preferred A.
    try std.testing.expect(try activeLeaseRunnerIs(conn, ZOMBIE_1_ID, RUNNER_B_ID));
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
    // The live holder owns the slot at fencing_seq=2, claim valid into the future.
    const live_until = std.time.milliTimestamp() + 60_000;
    try seedAffinity(conn, AFFINITY_1_ID, ZOMBIE_1_ID, RUNNER_A_ID, 2, live_until);

    // A superseded holder (token 1 < seq 2, as a reclaim would leave it) releases
    // → no-op: the slot stays held, leased_until unchanged.
    try affinity.release(conn, ZOMBIE_1_ID, 1);
    try std.testing.expectEqual(live_until, try leasedUntilOf(conn, ZOMBIE_1_ID));

    // The live holder (token == seq) releases → slot freed (leased_until → ~now).
    try affinity.release(conn, ZOMBIE_1_ID, 2);
    try std.testing.expect(try leasedUntilOf(conn, ZOMBIE_1_ID) < live_until);
}
