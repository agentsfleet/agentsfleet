// Integration tests for the assigned-runner-policy arc (M148 §1–§3) — the
// control plane assigns policy, delivers it with identity, accepts the host's
// capability report, and reconciles the two into a lease-gating degraded
// verdict. One suite walks the whole direction-of-authority inversion over the
// live test DB: register → heartbeat → PATCH → degraded → recovery.
//
// Spec test names carried here (graded at VERIFY):
//   1.1 test_enrollment_returns_assigned_policy
//   1.2 test_heartbeat_carries_current_assigned_policy
//   1.3 test_assigned_policy_persists_on_the_runner_row
//   3.2 test_first_heartbeat_carries_the_capability_report
//   3.3 test_unachievable_assignment_marks_runner_degraded
//   3.5 test_degraded_clears_when_capability_returns
// (2.1 test_runner_applies_assigned_tier_not_environment is runner-side — the
// conflicting *process environment* lives in the daemon's build graph, not in
// this binary; see src/runner/daemon/loop_test.zig.)
//
// The lease-gate proof (3.3) is a contrast, not a vacuous null: work IS
// published and leasable, the degraded runner is issued nothing, and after its
// capability recovers THE SAME runner receives THE SAME work — so the null can
// only have been the degraded gate. Requires TEST_DATABASE_URL (harness skips
// otherwise); the lease tests additionally require Redis and skip without it.
// Per the harness contract, cleanup helpers acquire their own connection.

const std = @import("std");
const pg = @import("pg");

const auth_mw = @import("../../../auth/middleware/mod.zig");
const api_key = @import("../../../auth/api_key.zig");
const ec = @import("../../../errors/error_registry.zig");
const serve_runner_lookup = @import("../../../cmd/serve_runner_lookup.zig");
const base = @import("../../../db/test_fixtures.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const redis_fleet = @import("../../../queue/redis_fleet.zig");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const reconcile = @import("heartbeat_reconcile.zig");
const clock = @import("common").clock;
const protocol = @import("contract").protocol;

const ALLOC = std.testing.allocator;

const TEST_JWKS = scope_fixtures.JWKS;
const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const PLATFORM_ADMIN_TOKEN = scope_fixtures.PLATFORM_ADMIN;

// Distinct UUIDv7 literals — no collision with sibling runner suites.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fa011";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fac01";

// One host name per test so residue from an aborted run never cross-fires.
const HOST_ENROLL = "policy-assign-host-enroll";
const HOST_LEGACY = "policy-assign-host-legacy";
const HOST_LENIENT = "policy-assign-host-lenient";
const HOST_HEARTBEAT = "policy-assign-host-heartbeat";
const HOST_RESTART = "policy-assign-host-restart";
const HOST_REPORT = "policy-assign-host-report";
const HOST_DEGRADED = "policy-assign-host-degraded";
const HOST_RECOVERY = "policy-assign-host-recovery";

const LARGE_BALANCE_NANOS: i64 = 1_000_000_000_000;
const BILLING_GRANT_SOURCE = "assigned-policy-test";

const CONFIG_NO_GATES =
    \\{"name":"assigned-policy-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;
const SOURCE_MD =
    \\---
    \\name: assigned-policy-bot
    \\---
    \\
    \\You are an assigned-policy test fleet.
;

// ── Wire bodies ─────────────────────────────────────────────────────────────
// Register bodies are per-host comptime constants; the assigned_policy
// envelope is the M148 register shape (the host declares nothing).

fn registerBody(comptime host: []const u8, comptime tier: []const u8, comptime network: []const u8, comptime registry: []const u8, comptime workers: []const u8) []const u8 {
    return "{\"host_id\":\"" ++ host ++ "\",\"assigned_policy\":{\"sandbox_tier\":\"" ++ tier ++
        "\",\"network_policy\":\"" ++ network ++ "\",\"registry_allowlist\":" ++ registry ++
        ",\"worker_count\":" ++ workers ++ "},\"labels\":[]}";
}

const TIER_LANDLOCK = "landlock_full";
const TIER_DEV = "dev_none";
const TIER_NESTED = "container_nested";
const NETWORK_ALLOW_ALL = "allow_all";
const NETWORK_DENY_ALL = "deny_all_egress";
const REGISTRY_INTERNAL = "registry.internal.example";
// Over the shared MAX_WORKER_COUNT bound — the echo must come back clamped.
const OVERSIZE_WORKERS = "999";

const BODY_ENROLL = registerBody(HOST_ENROLL, TIER_LANDLOCK, NETWORK_DENY_ALL, "[\"" ++ REGISTRY_INTERNAL ++ "\"]", OVERSIZE_WORKERS);
const BODY_HEARTBEAT = registerBody(HOST_HEARTBEAT, TIER_DEV, NETWORK_ALLOW_ALL, "[]", "1");
const BODY_RESTART = registerBody(HOST_RESTART, TIER_LANDLOCK, NETWORK_ALLOW_ALL, "[]", "1");
const BODY_REPORT = registerBody(HOST_REPORT, TIER_DEV, NETWORK_ALLOW_ALL, "[]", "1");
const BODY_DEGRADED = registerBody(HOST_DEGRADED, TIER_LANDLOCK, NETWORK_ALLOW_ALL, "[]", "1");
const BODY_RECOVERY = registerBody(HOST_RECOVERY, TIER_LANDLOCK, NETWORK_ALLOW_ALL, "[]", "1");
const BODY_LENIENT = registerBody(HOST_LENIENT, TIER_DEV, NETWORK_ALLOW_ALL, "[]", "1");

// The pre-migration row: a fixed id (so the PATCH can address it) and a fixed
// bearer, inserted with ONLY the pre-M148 columns — migration 042's defaults
// fill the rest and network_policy stays NULL.
const LEGACY_RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fae01";
const LEGACY_TOKEN = protocol.RUNNER_TOKEN_PREFIX ++ "l" ** 60;

// One-of violations for the PATCH body contract: both halves, and neither.
const PATCH_BOTH =
    \\{"action":"cordon","assigned_policy":{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":1}}
;
const PATCH_NEITHER = "{\"labels\":[]}";
// A scheme-carrying entry the dashboard's grammar refuses — the server must too.
const PATCH_BAD_REGISTRY =
    \\{"assigned_policy":{"sandbox_tier":"container_nested","network_policy":"allow_all","registry_allowlist":["http://bad url"],"worker_count":1}}
;

const PATCH_TO_NESTED =
    \\{"assigned_policy":{"sandbox_tier":"container_nested","network_policy":"deny_all_egress","registry_allowlist":["pypi.org"],"worker_count":2}}
;

// Capability reports: full-minus-landlock degrades a landlock_full assignment;
// the full report satisfies it (egress_enforcement stays false — no assignment
// here wants `allow_list_egress`).
const HB_REPORT_NO_LANDLOCK =
    \\{"capability_report":{"landlock":false,"seccomp":true,"cgroup_controllers":["cpu","memory","pids"],"bubblewrap":true,"egress_enforcement":false}}
;
const HB_REPORT_FULL =
    \\{"capability_report":{"landlock":true,"seccomp":true,"cgroup_controllers":["cpu","memory","pids"],"bubblewrap":true,"egress_enforcement":false}}
;
const HB_EMPTY = "{}";
// Shape-invalid: landlock must be a bool. The lenient parse must read this as
// "no report this beat", never fail the liveness beat or clobber the stored one.
const HB_REPORT_MALFORMED =
    \\{"capability_report":{"landlock":"maybe"}}
;

// Response fragments asserted by substring — std.json serializes struct fields
// in declaration order, so key:value pairs are stable.
const FRAG_TIER_LANDLOCK = "\"sandbox_tier\":\"" ++ TIER_LANDLOCK ++ "\"";
const FRAG_TIER_DEV = "\"sandbox_tier\":\"" ++ TIER_DEV ++ "\"";
const FRAG_TIER_NESTED = "\"sandbox_tier\":\"" ++ TIER_NESTED ++ "\"";
const FRAG_NETWORK_DENY = "\"network_policy\":\"" ++ NETWORK_DENY_ALL ++ "\"";
const FRAG_DEGRADED_TRUE = "\"degraded\":true";
const FRAG_DEGRADED_FALSE = "\"degraded\":false";
const FRAG_REASON_NULL = "\"degraded_reason\":null";
const FRAG_LEASE_NULL = "\"lease\":null";
// The detail read's achievable report opens with the landlock flag — false in
// the degraded arc's stored report.
const FRAG_ACHIEVABLE_NO_LANDLOCK = "\"landlock\":false";
const FRAG_WORKERS_TWO = "\"worker_count\":2";
const FRAG_PATCHED_REGISTRY = "pypi.org";

// SAFETY: populated by configureRegistry before runnerBearer reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn startHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

// ── Helpers ─────────────────────────────────────────────────────────────────

const Registered = struct { runner_id: []const u8, runner_token: []const u8 };

fn register(h: *TestHarness, body: []const u8) !Registered {
    const resp = try (try (try h.post(protocol.PATH_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).json(body)).send();
    defer resp.deinit();
    try resp.expectStatus(.created);
    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, resp.body, .{});
    defer parsed.deinit();
    const obj = parsed.value.object;
    const id = try ALLOC.dupe(u8, obj.get("runner_id").?.string);
    errdefer ALLOC.free(id);
    const token = try ALLOC.dupe(u8, obj.get("runner_token").?.string);
    return .{ .runner_id = id, .runner_token = token };
}

fn freeRegistered(r: Registered) void {
    ALLOC.free(r.runner_id);
    ALLOC.free(r.runner_token);
}

fn heartbeat(h: *TestHarness, token: []const u8, body: []const u8) !harness_mod.Response {
    return (try (try h.post(protocol.PATH_RUNNER_HEARTBEATS).bearer(token)).json(body)).send();
}

fn leaseOnce(h: *TestHarness, token: []const u8) !harness_mod.Response {
    return (try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(token)).json(HB_EMPTY)).send();
}

/// The stored verdict + capability columns, duped for the caller.
const VerdictRow = struct {
    degraded: bool,
    reason: ?[]u8,
    has_report: bool,
    reported_at: i64,

    fn deinit(self: VerdictRow) void {
        if (self.reason) |r| ALLOC.free(r);
    }
};

fn verdictRow(conn: *pg.Conn, runner_id: []const u8) !VerdictRow {
    var q = PgQuery.from(try conn.query(
        \\SELECT degraded, degraded_reason, capability_report IS NOT NULL,
        \\       COALESCE(capability_reported_at, 0)
        \\FROM fleet.runners WHERE id = $1::uuid
    , .{runner_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const reason_raw = try row.get(?[]const u8, 1);
    return .{
        .degraded = try row.get(bool, 0),
        .reason = if (reason_raw) |r| try ALLOC.dupe(u8, r) else null,
        .has_report = try row.get(bool, 2),
        .reported_at = try row.get(i64, 3),
    };
}

fn policyAssignedEventCount(conn: *pg.Conn, runner_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT COUNT(*)::bigint FROM fleet.runner_events
        \\WHERE runner_id = $1::uuid AND event_type = $2
    , .{ runner_id, @tagName(protocol.RunnerEventType.runner_policy_assigned) }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return row.get(i64, 0);
}

fn patchPath(runner_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(ALLOC, "{s}/{s}", .{ protocol.PATH_FLEET_RUNNERS, runner_id });
}

fn execIgnore(conn: *pg.Conn, sql_text: []const u8, args: anytype) void {
    _ = conn.exec(sql_text, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn seedFleetWork(conn: *pg.Conn) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProvider(ALLOC, conn, WORKSPACE_ID);
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_wallet (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, LARGE_BALANCE_NANOS, BILLING_GRANT_SOURCE });
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "assigned-policy-fleet", CONFIG_NO_GATES, SOURCE_MD);
    try base.seedFleetSession(conn, FLEET_ID, HB_EMPTY);
}

fn publishFreshEvent(h: *TestHarness) !void {
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, FLEET_ID);
    const id = try redis_fleet.xaddFleetEvent(&h.queue, .{
        .event_id = "",
        .fleet_id = FLEET_ID,
        .workspace_id = WORKSPACE_ID,
        .actor = "steer:assigned-policy",
        .event_type = .chat,
        .request_json = "{\"message\":\"ping\"}",
        .created_at = clock.nowMillis(),
    });
    h.queue.alloc.free(id);
}

/// Suite-wide teardown, safe on partial residue. Runner rows are addressed by
/// the per-test host names, so an aborted sibling run never cross-fires.
fn teardown(conn: *pg.Conn) void {
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE workspace_id = $1::uuid", .{WORKSPACE_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM core.fleet_events WHERE workspace_id = $1::uuid", .{WORKSPACE_ID});
    execIgnore(conn,
        \\DELETE FROM fleet.runners WHERE host_id IN ($1, $2, $3, $4, $5, $6, $7, $8)
    , .{ HOST_ENROLL, HOST_HEARTBEAT, HOST_RESTART, HOST_REPORT, HOST_DEGRADED, HOST_RECOVERY, HOST_LEGACY, HOST_LENIENT });
    base.teardownPlatformProvider(conn, WORKSPACE_ID);
    base.teardownFleets(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

/// Teardown under a freshly-acquired connection (deferred inline cleanup would
/// hold a pool connection across `pool.deinit()`).
fn cleanupAll(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    teardown(conn);
}

fn purgeRedis(h: *TestHarness) void {
    redis_fleet.purgeFleetRedisState(&h.queue, FLEET_ID) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

// ── §1 — policy travels with identity ───────────────────────────────────────

test "integration: test_enrollment_returns_assigned_policy" {
    const h = try startHarness();
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn); // residue from an aborted prior run
    }
    defer cleanupAll(h);

    // Enrol with landlock_full assigned; the echo is the assignment AS STORED —
    // same tier, same network, same registry, and the worker count clamped into
    // the shared bound (999 → MAX_WORKER_COUNT), so the operator sees exactly
    // what the host will apply.
    const resp = try (try (try h.post(protocol.PATH_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).json(BODY_ENROLL)).send();
    defer resp.deinit();
    try resp.expectStatus(.created);
    try std.testing.expect(resp.bodyContains(FRAG_TIER_LANDLOCK));
    try std.testing.expect(resp.bodyContains(FRAG_NETWORK_DENY));
    try std.testing.expect(resp.bodyContains(REGISTRY_INTERNAL));
    const clamped = try std.fmt.allocPrint(ALLOC, "\"worker_count\":{d}", .{protocol.MAX_WORKER_COUNT});
    defer ALLOC.free(clamped);
    try std.testing.expect(resp.bodyContains(clamped));

    // And the row holds the assignment — columns, not a per-request derivation.
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT sandbox_tier, network_policy, worker_count FROM fleet.runners WHERE host_id = $1
    , .{HOST_ENROLL}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings(TIER_LANDLOCK, try row.get([]const u8, 0));
    try std.testing.expectEqualStrings(NETWORK_DENY_ALL, try row.get(?[]const u8, 1) orelse return error.TestUnexpectedResult);
    try std.testing.expectEqual(@as(i32, @intCast(protocol.MAX_WORKER_COUNT)), try row.get(i32, 2));
}

test "integration: test_heartbeat_carries_current_assigned_policy" {
    const h = try startHarness();
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn);
    }
    defer cleanupAll(h);

    const reg = try register(h, BODY_HEARTBEAT);
    defer freeRegistered(reg);

    // Beat 1 delivers the enrollment-time assignment.
    {
        const resp = try heartbeat(h, reg.runner_token, HB_EMPTY);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_TIER_DEV));
        try std.testing.expect(resp.bodyContains(FRAG_DEGRADED_FALSE));
    }

    // The operator retiers the runner over the real PATCH wire (no host visit).
    {
        const path = try patchPath(reg.runner_id);
        defer ALLOC.free(path);
        const resp = try (try (try h.patch(path).bearer(PLATFORM_ADMIN_TOKEN)).json(PATCH_TO_NESTED)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_TIER_NESTED));
    }

    // Beat 2 — same runner process, no restart — carries the NEW assignment,
    // and the re-assignment left its audit event on the row's history.
    {
        const resp = try heartbeat(h, reg.runner_token, HB_EMPTY);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_TIER_NESTED));
        try std.testing.expect(resp.bodyContains(FRAG_WORKERS_TWO));
        try std.testing.expect(resp.bodyContains(FRAG_PATCHED_REGISTRY));
    }
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try std.testing.expectEqual(@as(i64, 1), try policyAssignedEventCount(conn, reg.runner_id));
    }
}

test "integration: test_assigned_policy_persists_on_the_runner_row" {
    // Two harness instantiations = a literal control-plane restart between the
    // write and the read: nothing in memory survives, only the row.
    var token: ?[]const u8 = null;
    defer if (token) |t| ALLOC.free(t);
    {
        const h = try startHarness();
        defer h.deinit();
        {
            const conn = try h.acquireConn();
            defer h.releaseConn(conn);
            teardown(conn);
        }
        const reg = try register(h, BODY_RESTART);
        token = try ALLOC.dupe(u8, reg.runner_token);
        freeRegistered(reg);
    }
    {
        const h = try startHarness();
        defer h.deinit();
        defer cleanupAll(h);
        const resp = try heartbeat(h, token.?, HB_EMPTY);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_TIER_LANDLOCK));
    }
}

// ── §3 — capability up, reconciled, lease-gated ─────────────────────────────

test "integration: test_first_heartbeat_carries_the_capability_report" {
    const h = try startHarness();
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn);
    }
    defer cleanupAll(h);

    const reg = try register(h, BODY_REPORT);
    defer freeRegistered(reg);

    // The first beat's report lands on the row with its arrival stamp.
    {
        const resp = try heartbeat(h, reg.runner_token, HB_REPORT_FULL);
        defer resp.deinit();
        try resp.expectStatus(.ok);
    }
    var first: VerdictRow = undefined;
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        first = try verdictRow(conn, reg.runner_id);
    }
    defer first.deinit();
    try std.testing.expect(first.has_report);
    try std.testing.expect(first.reported_at > 0);

    // A later report-free beat re-reconciles the STORED report and, in steady
    // state, rewrites nothing — the arrival stamp does not move.
    {
        const resp = try heartbeat(h, reg.runner_token, HB_EMPTY);
        defer resp.deinit();
        try resp.expectStatus(.ok);
    }
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        const second = try verdictRow(conn, reg.runner_id);
        defer second.deinit();
        try std.testing.expect(second.has_report);
        try std.testing.expectEqual(first.reported_at, second.reported_at);
    }
}

test "integration: test_unachievable_assignment_marks_runner_degraded" {
    const h = try startHarness();
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;
    base.setTestEncryptionKey();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn);
        try seedFleetWork(conn);
    }
    defer cleanupAll(h);
    defer purgeRedis(h);

    const reg = try register(h, BODY_DEGRADED);
    defer freeRegistered(reg);

    // The report lacks Landlock; the assignment is landlock_full. The reply
    // AND the row carry the verdict, and the reason names the mechanism.
    {
        const resp = try heartbeat(h, reg.runner_token, HB_REPORT_NO_LANDLOCK);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_DEGRADED_TRUE));
        try std.testing.expect(resp.bodyContains(reconcile.REASON_LANDLOCK_UNAVAILABLE));
    }
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        const row = try verdictRow(conn, reg.runner_id);
        defer row.deinit();
        try std.testing.expect(row.degraded);
        try std.testing.expectEqualStrings(reconcile.REASON_LANDLOCK_UNAVAILABLE, row.reason orelse return error.TestUnexpectedResult);
    }

    // The operator surfaces carry the verdict: the fleet LIST row shows the
    // assigned tier, the degraded flag, and the reason; the DETAIL read adds
    // the achievable report (§4 server face of Dimensions 4.1/4.2).
    {
        const resp = try (try h.get(protocol.PATH_FLEET_RUNNERS).bearer(PLATFORM_ADMIN_TOKEN)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(HOST_DEGRADED));
        try std.testing.expect(resp.bodyContains(FRAG_DEGRADED_TRUE));
        try std.testing.expect(resp.bodyContains(reconcile.REASON_LANDLOCK_UNAVAILABLE));
    }
    {
        const path = try patchPath(reg.runner_id);
        defer ALLOC.free(path);
        const resp = try (try h.get(path).bearer(PLATFORM_ADMIN_TOKEN)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_TIER_LANDLOCK));
        try std.testing.expect(resp.bodyContains(FRAG_DEGRADED_TRUE));
        try std.testing.expect(resp.bodyContains(FRAG_ACHIEVABLE_NO_LANDLOCK));
    }

    // Work IS available, and the degraded runner is issued none of it.
    try publishFreshEvent(h);
    {
        const resp = try leaseOnce(h, reg.runner_token);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_LEASE_NULL));
    }

    // Capability returns → verdict clears → THE SAME runner receives THE SAME
    // work. This is what proves the null above was the degraded gate and not
    // an empty queue.
    {
        const resp = try heartbeat(h, reg.runner_token, HB_REPORT_FULL);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_DEGRADED_FALSE));
    }
    {
        const resp = try leaseOnce(h, reg.runner_token);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(!resp.bodyContains(FRAG_LEASE_NULL));
    }
}

test "integration: a pre-migration row reads degraded until the dashboard assigns a policy" {
    // The rollout case Indy declined to unbrick via env: a runner enrolled
    // before the policy columns existed has NULL network_policy. Migration 042
    // deliberately does NOT backfill it (Indy: "yes done by hand") — the row
    // starts at the column default, the operator's manual patch (the statement
    // documented in 042_runner_assigned_policy.sql, proven here) marks it
    // degraded, the dashboard PATCH is the fix path, and each verdict change
    // is visible on the very next beat.
    const h = try startHarness();
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn);
        const hash = api_key.sha256Hex(LEGACY_TOKEN);
        _ = try conn.exec(
            \\INSERT INTO fleet.runners
            \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
            \\   last_seen_at, created_at, updated_at)
            \\VALUES ($1::uuid, $2, $3, $4, $5, '[]'::jsonb, NULL, 0, 0, 0)
            \\ON CONFLICT (id) DO NOTHING
        , .{ LEGACY_RUNNER_ID, HOST_LEGACY, hash[0..], TIER_LANDLOCK, protocol.ADMIN_STATE_ACTIVE });

        // No backfill: the freshly migrated row reads NOT degraded — pins the
        // deliberate absence so a silently re-added backfill (or a flipped
        // column default) fails here, not in a rollout.
        {
            const before = try verdictRow(conn, LEGACY_RUNNER_ID);
            defer before.deinit();
            try std.testing.expect(!before.degraded);
        }

        // The operator's manual patch — the exact statement 042 documents,
        // with the reason bound to the canonical constant so the documented
        // remedy can never drift from what reconciliation writes.
        _ = try conn.exec(
            \\UPDATE fleet.runners SET degraded = TRUE, degraded_reason = $1
            \\WHERE network_policy IS NULL
        , .{reconcile.REASON_NO_ASSIGNED_POLICY});
        {
            const after = try verdictRow(conn, LEGACY_RUNNER_ID);
            defer after.deinit();
            try std.testing.expect(after.degraded);
            try std.testing.expectEqualStrings(reconcile.REASON_NO_ASSIGNED_POLICY, after.reason orelse return error.TestUnexpectedResult);
        }
    }
    defer cleanupAll(h);

    // The legacy row heartbeats: no assignment to deliver, degraded with the
    // no-assigned-policy reason — never a silently defaulted policy.
    {
        const resp = try heartbeat(h, LEGACY_TOKEN, HB_EMPTY);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_DEGRADED_TRUE));
        try std.testing.expect(resp.bodyContains(reconcile.REASON_NO_ASSIGNED_POLICY));
        try std.testing.expect(resp.bodyContains("\"assigned_policy\":null"));
    }

    const path = try patchPath(LEGACY_RUNNER_ID);
    defer ALLOC.free(path);

    // The PATCH body is EXACTLY one of action / assigned_policy: both and
    // neither each answer 400 with the invalid-request code.
    inline for (.{ PATCH_BOTH, PATCH_NEITHER }) |bad| {
        const resp = try (try (try h.patch(path).bearer(PLATFORM_ADMIN_TOKEN)).json(bad)).send();
        defer resp.deinit();
        try resp.expectStatus(.bad_request);
        try std.testing.expect(resp.bodyContains(ec.ERR_INVALID_REQUEST));
    }

    // The operator assigns a policy from the dashboard PATCH. The verdict is
    // re-reconciled IN the PATCH request — the reason moves to
    // no-capability-report on the row immediately, before any beat — and the
    // next beat delivers the new assignment.
    {
        const resp = try (try (try h.patch(path).bearer(PLATFORM_ADMIN_TOKEN)).json(PATCH_TO_NESTED)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
    }
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        const row = try verdictRow(conn, LEGACY_RUNNER_ID);
        defer row.deinit();
        try std.testing.expect(row.degraded);
        try std.testing.expectEqualStrings(reconcile.REASON_NO_CAPABILITY_REPORT, row.reason orelse return error.TestUnexpectedResult);
    }

    // The PATCH is idempotent: re-sending identical values answers 200 and
    // emits NO second audit event (the IS DISTINCT FROM guard writes nothing).
    {
        const resp = try (try (try h.patch(path).bearer(PLATFORM_ADMIN_TOKEN)).json(PATCH_TO_NESTED)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
    }
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try std.testing.expectEqual(@as(i64, 1), try policyAssignedEventCount(conn, LEGACY_RUNNER_ID));
    }

    // Registry entries are validated server-side — the raw API cannot store
    // what the dialog would reject.
    {
        const resp = try (try (try h.patch(path).bearer(PLATFORM_ADMIN_TOKEN)).json(PATCH_BAD_REGISTRY)).send();
        defer resp.deinit();
        try resp.expectStatus(.bad_request);
        try std.testing.expect(resp.bodyContains(ec.ERR_INVALID_REQUEST));
    }

    {
        const resp = try heartbeat(h, LEGACY_TOKEN, HB_EMPTY);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_TIER_NESTED));
        try std.testing.expect(resp.bodyContains(FRAG_DEGRADED_TRUE));
        try std.testing.expect(resp.bodyContains(reconcile.REASON_NO_CAPABILITY_REPORT));
    }

    // A satisfying report completes the recovery: the once-bricked legacy row
    // is a healthy assigned runner, all from the dashboard + heartbeats.
    {
        const resp = try heartbeat(h, LEGACY_TOKEN, HB_REPORT_FULL);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_DEGRADED_FALSE));
    }
}

test "integration: a malformed capability report never fails the liveness beat" {
    // The heartbeat's lenient-parse branch: garbage in the report slot reads
    // as "no report this beat" — the beat succeeds, policy delivery is
    // unaffected, and the STORED report (and its arrival stamp) is untouched.
    const h = try startHarness();
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn);
    }
    defer cleanupAll(h);

    const reg = try register(h, BODY_LENIENT);
    defer freeRegistered(reg);

    {
        const resp = try heartbeat(h, reg.runner_token, HB_REPORT_FULL);
        defer resp.deinit();
        try resp.expectStatus(.ok);
    }
    var first: VerdictRow = undefined;
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        first = try verdictRow(conn, reg.runner_id);
    }
    defer first.deinit();
    try std.testing.expect(first.has_report);

    {
        const resp = try heartbeat(h, reg.runner_token, HB_REPORT_MALFORMED);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_TIER_DEV));
        try std.testing.expect(resp.bodyContains(FRAG_DEGRADED_FALSE));
    }
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        const second = try verdictRow(conn, reg.runner_id);
        defer second.deinit();
        try std.testing.expect(second.has_report);
        try std.testing.expectEqual(first.reported_at, second.reported_at);
    }
}

test "integration: test_degraded_clears_when_capability_returns" {
    const h = try startHarness();
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn);
    }
    defer cleanupAll(h);

    const reg = try register(h, BODY_RECOVERY);
    defer freeRegistered(reg);

    {
        const resp = try heartbeat(h, reg.runner_token, HB_REPORT_NO_LANDLOCK);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_DEGRADED_TRUE));
    }

    // The very next beat with a satisfying report clears the verdict — reply
    // and row agree, and the reason is gone, not stale.
    {
        const resp = try heartbeat(h, reg.runner_token, HB_REPORT_FULL);
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(FRAG_DEGRADED_FALSE));
        try std.testing.expect(resp.bodyContains(FRAG_REASON_NULL));
    }
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        const row = try verdictRow(conn, reg.runner_id);
        defer row.deinit();
        try std.testing.expect(!row.degraded);
        try std.testing.expect(row.reason == null);
    }
}
