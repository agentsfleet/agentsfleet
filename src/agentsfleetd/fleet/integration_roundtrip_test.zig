// End-to-end round-trip over the runner control plane via the real HTTP router
// + runner_bearer middleware against the live test DB + Redis. Two journeys:
//
//   1. Single-runner happy path: a runner polls (lease), holds during execution,
//      extends (renew), then reports completion. The lease returns a
//      fencing_token; renew advances the kill deadline; report flips the lease
//      to 'reported'. The token threads through all three calls unchanged.
//
//   2. Multi-runner reclaim chain — the fencing ordering law: runner A's claim
//      expires, runner B re-leases the same event under a strictly higher token,
//      and A's late report on the stale token is rejected UZ-RUN-005 while B's
//      report (current token) succeeds. A's old token < B's new token is the
//      monotonic guarantee the whole exactly-once story rests on.
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
const metrics_runner = @import("../observability/metrics_runner.zig");
const telemetry = @import("../observability/telemetry.zig");

const ALLOC = std.testing.allocator;
const LARGE_BALANCE_NANOS: i64 = 1000000000000;

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECK passes.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dd011";
const RUNNER_A_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dda01";
const RUNNER_B_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddb01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddc01";
const FENCED_ERROR_CODE = "UZ-RUN-005";

const RUNNER_A_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "f" ** 64;
const RUNNER_B_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "0" ** 64;

const CONFIG_NO_GATES =
    \\{"name":"roundtrip-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;
const SOURCE_MD =
    \\---
    \\name: roundtrip-bot
    \\---
    \\
    \\You are a round-trip test fleet.
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

fn seedActiveFleet(conn: *pg.Conn) !void {
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "roundtrip-bot", CONFIG_NO_GATES, SOURCE_MD);
    try base.seedFleetSession(conn, FLEET_ID, "{}");
}

fn fundLargeBalance(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_wallet (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'roundtrip-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, LARGE_BALANCE_NANOS });
}

fn publishFreshEvent(h: *TestHarness) !void {
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, FLEET_ID);
    const id = try redis_fleet.xaddFleetEvent(&h.queue, .{
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
    /// alloc-dup'd; the caller frees. Null when no lease was issued.
    lease_id: ?[]const u8 = null,
    /// alloc-dup'd; the caller frees. The leased event's id — the report echoes it.
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

fn leaseAs(h: *TestHarness, token: []const u8) !LeaseView {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(token)).json(protocol.LEASE_REQUEST_CURRENT_JSON);
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    return parseLease(ALLOC, resp.body);
}

fn renewAs(h: *TestHarness, token: []const u8, lease_id: []const u8) !harness_mod.Response {
    const path = try std.fmt.allocPrint(ALLOC, "{s}/{s}/{s}", .{
        protocol.PATH_RUNNER_LEASES, lease_id, protocol.RUNNER_LEASE_RENEW_SUFFIX,
    });
    defer ALLOC.free(path);
    const req = try (try h.post(path).bearer(token)).json("{}");
    return req.send();
}

fn reportAs(h: *TestHarness, token: []const u8, lease_id: []const u8, event_id: []const u8, fencing_token: u64) !harness_mod.Response {
    const body = try std.fmt.allocPrint(ALLOC,
        \\{{"lease_id":"{s}","event_id":"{s}","fencing_token":{d},"outcome":"processed","response_text":"done","tokens":10,"telemetry":{{"time_to_first_token_ms":5,"wall_ms":100}},"checkpoint":{{"last_event_id":"{s}","last_response":"done"}}}}
    , .{ lease_id, event_id, fencing_token, event_id });
    defer ALLOC.free(body);
    const req = try (try h.post(protocol.PATH_RUNNER_REPORTS).bearer(token)).json(body);
    return req.send();
}

fn reportFailureAs(h: *TestHarness, token: []const u8, lease_id: []const u8, event_id: []const u8, fencing_token: u64, reason: []const u8) !harness_mod.Response {
    const body = try std.fmt.allocPrint(ALLOC,
        \\{{"lease_id":"{s}","event_id":"{s}","fencing_token":{d},"outcome":"fleet_error","failure_reason":"{s}","response_text":"killed","tokens":0,"telemetry":{{"time_to_first_token_ms":0,"wall_ms":50}},"checkpoint":{{"last_event_id":"{s}","last_response":""}}}}
    , .{ lease_id, event_id, fencing_token, reason, event_id });
    defer ALLOC.free(body);
    const req = try (try h.post(protocol.PATH_RUNNER_REPORTS).bearer(token)).json(body);
    return req.send();
}

/// True iff the event's persisted `failure_label` equals `expected`. Compared
/// in-function so the row-backed slice never outlives the query.
fn failureLabelMatches(conn: *pg.Conn, event_id: []const u8, expected: []const u8) !bool {
    var q = PgQuery.from(try conn.query(
        "SELECT failure_label FROM core.fleet_events WHERE fleet_id = $1::uuid AND event_id = $2",
        .{ FLEET_ID, event_id },
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.EventRowMissing;
    const label = try row.get(?[]const u8, 0) orelse return false;
    return std.mem.eql(u8, label, expected);
}

fn leaseStatusIs(conn: *pg.Conn, lease_id: []const u8, expected: []const u8) !bool {
    var q = PgQuery.from(try conn.query("SELECT status FROM fleet.runner_leases WHERE id = $1::uuid", .{lease_id}));
    defer q.deinit();
    const row = try q.next() orelse return error.LeaseRowMissing;
    return std.mem.eql(u8, try row.get([]const u8, 0), expected);
}

fn leaseExpiresAtOf(conn: *pg.Conn, lease_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query("SELECT lease_expires_at FROM fleet.runner_leases WHERE id = $1::uuid", .{lease_id}));
    defer q.deinit();
    const row = try q.next() orelse return error.LeaseRowMissing;
    return row.get(i64, 0);
}

fn completionCaptureCount() u32 {
    return telemetry.TestBackend.globalCount(.fleet_completed);
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

/// Drop the fleet's stream AND its readiness mark — `fleet:ready` is one shared
/// key and `peek` is bounded + randomized, so a leftover mark for a deleted
/// fleet can crowd a sibling suite's fleet out of the sample.
fn forgetFleet(h: *TestHarness, fleet_id: []const u8) void {
    redis_fleet.purgeFleetRedisState(&h.queue, fleet_id) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn cleanupAll(h: *TestHarness, conn: *pg.Conn) void {
    forgetFleet(h, FLEET_ID);
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id IN ($1::uuid, $2::uuid)", .{ RUNNER_A_ID, RUNNER_B_ID });
    execIgnore(conn, "DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid", .{FLEET_ID});
    base.teardownPlatformProvider(conn, WORKSPACE_ID);
    base.teardownFleets(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
    base.teardownTenant(conn);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test "single runner completes a full lease then renew then report round-trip" {
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
    try seedRunner(conn, RUNNER_A_ID, "roundtrip-a", RUNNER_A_TOKEN);
    try seedActiveFleet(conn);
    try publishFreshEvent(h);

    // 1. Lease — the runner polls and is assigned the fleet's event.
    const lv = try leaseAs(h, RUNNER_A_TOKEN);
    defer lv.free();
    try std.testing.expect(lv.present);
    const lease_id = lv.lease_id.?;
    const before = try leaseExpiresAtOf(conn, lease_id);

    // 2. Renew — the runner holds during execution and extends the deadline
    //    under the SAME fencing token; the kill deadline must advance.
    const renew_resp = try renewAs(h, RUNNER_A_TOKEN, lease_id);
    defer renew_resp.deinit();
    try renew_resp.expectStatus(.ok);
    try std.testing.expect(try leaseExpiresAtOf(conn, lease_id) >= before);

    // 3. Report — the runner finishes; the lease flips to 'reported'.
    const rep = try reportAs(h, RUNNER_A_TOKEN, lease_id, lv.event_id.?, lv.fencing_token);
    defer rep.deinit();
    try rep.expectStatus(.ok);
    try std.testing.expect(try leaseStatusIs(conn, lease_id, "reported"));
}

test "a failed runner report persists the granular failure_label and increments the failure metric" {
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
    try seedRunner(conn, RUNNER_A_ID, "roundtrip-a", RUNNER_A_TOKEN);
    try seedActiveFleet(conn);
    try publishFreshEvent(h);

    // Lease (inserts the received core.fleet_events row), then report a FAILURE
    // carrying the granular reason.
    const lv = try leaseAs(h, RUNNER_A_TOKEN);
    defer lv.free();
    try std.testing.expect(lv.present);

    const rep = try reportFailureAs(h, RUNNER_A_TOKEN, lv.lease_id.?, lv.event_id.?, lv.fencing_token, "runner_crash");
    defer rep.deinit();
    try rep.expectStatus(.ok);

    // The granular cause reached the durable record (not the coarse outcome) ...
    try std.testing.expect(try leaseStatusIs(conn, lv.lease_id.?, "reported"));
    try std.testing.expect(try failureLabelMatches(conn, lv.event_id.?, "runner_crash"));

    // ... and the per-runner metrics carry both the granular reason and the
    // outcome-bucketed execution on /metrics (render via an allocating writer so
    // the assertion is robust to runners accumulated by sibling tests).
    var aw: std.Io.Writer.Allocating = .init(ALLOC);
    defer aw.deinit();
    try metrics_runner.renderPrometheus(&aw.writer);
    const metrics = aw.written();
    const failure_needle = "runner_id=\"" ++ RUNNER_A_ID ++ "\",reason=\"runner_crash\"";
    const exec_needle = "runner_id=\"" ++ RUNNER_A_ID ++ "\",outcome=\"fleet_error\"";
    try std.testing.expect(std.mem.containsAtLeast(u8, metrics, 1, failure_needle));
    try std.testing.expect(std.mem.containsAtLeast(u8, metrics, 1, exec_needle));
}

test "the reclaim chain enforces monotonic token ordering across runners" {
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
    try seedRunner(conn, RUNNER_A_ID, "roundtrip-a", RUNNER_A_TOKEN); // dead holder
    try seedRunner(conn, RUNNER_B_ID, "roundtrip-b", RUNNER_B_TOKEN); // reclaimer
    try seedActiveFleet(conn);
    try publishFreshEvent(h);

    // A takes the lease through the production path, then dies: its affinity
    // claim expires while its lease row stays 'active' — the exact state a
    // reclaim recovers. This state can no longer be seeded by SQL shortcut:
    // under ready-first discovery a fleet nothing ever published to is absent
    // from the readiness index, so a poll answers no-work before Postgres is
    // ever consulted.
    const lv_a = try leaseAs(h, RUNNER_A_TOKEN);
    defer lv_a.free();
    try std.testing.expect(lv_a.present);
    _ = try conn.exec("UPDATE fleet.runner_affinity SET leased_until = 0 WHERE fleet_id = $1::uuid", .{FLEET_ID});

    // B leases → reclaims A's event under a strictly higher token.
    const lv = try leaseAs(h, RUNNER_B_TOKEN);
    defer lv.free();
    try std.testing.expect(lv.present);
    try std.testing.expectEqualStrings(lv_a.event_id.?, lv.event_id.?); // the SAME event, re-leased
    try std.testing.expect(lv.fencing_token > lv_a.fencing_token); // A's old token < B's new token

    // A's old lease is retired by the reclaim.
    try std.testing.expect(try leaseStatusIs(conn, lv_a.lease_id.?, "expired"));

    // A's late report on the stale token is fenced out.
    const a_rep = try reportAs(h, RUNNER_A_TOKEN, lv_a.lease_id.?, lv_a.event_id.?, lv_a.fencing_token);
    defer a_rep.deinit();
    try a_rep.expectErrorCode(FENCED_ERROR_CODE);

    // B's report on its fresh lease + current token succeeds.
    const b_rep = try reportAs(h, RUNNER_B_TOKEN, lv.lease_id.?, lv.event_id.?, lv.fencing_token);
    defer b_rep.deinit();
    try b_rep.expectStatus(.ok);
}

test "integration: test_settled_report_captures_fleet_completed" {
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
    try seedRunner(conn, RUNNER_A_ID, "roundtrip-a", RUNNER_A_TOKEN);
    try seedActiveFleet(conn);
    try publishFreshEvent(h);

    const lv = try leaseAs(h, RUNNER_A_TOKEN);
    defer lv.free();
    try std.testing.expect(lv.present);

    // The handler runs on an httpz worker thread, so the cross-thread tally —
    // not TestBackend's threadlocal ring — is what this assertion can read.
    telemetry.TestBackend.resetGlobal();

    const rep = try reportAs(h, RUNNER_A_TOKEN, lv.lease_id.?, lv.event_id.?, lv.fencing_token);
    defer rep.deinit();
    try rep.expectStatus(.ok);
    try std.testing.expectEqual(@as(u32, 1), completionCaptureCount());

    // Replaying the identical report loses the fence, so the business funnel
    // stays at exactly one completion per accepted run.
    const replay = try reportAs(h, RUNNER_A_TOKEN, lv.lease_id.?, lv.event_id.?, lv.fencing_token);
    defer replay.deinit();
    try replay.expectErrorCode(FENCED_ERROR_CODE);
    try std.testing.expectEqual(@as(u32, 1), completionCaptureCount());

    // The key PostHog deduplicates on derives only from settled identifiers, so
    // a delivery retry of this same completion carries the same insertion id.
    const facts: telemetry.FleetCompleted.SettledFacts = .{
        .distinct_id = WORKSPACE_ID,
        .workspace_id = WORKSPACE_ID,
        .fleet_id = FLEET_ID,
        .event_id = lv.event_id.?,
        .tokens = 10,
        .wall_ms = 100,
        .exit_status = "processed",
        .time_to_first_token_ms = 5,
    };
    const first = telemetry.FleetCompleted.init(facts);
    const again = telemetry.FleetCompleted.init(facts);
    try std.testing.expectEqualStrings(&first.insert_id, &again.insert_id);
    for (first.insert_id) |c| {
        try std.testing.expect(std.ascii.isHex(c) and !std.ascii.isUpper(c));
    }
}

test "integration: test_unaccepted_report_never_captures_completion" {
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
    try seedRunner(conn, RUNNER_A_ID, "roundtrip-a", RUNNER_A_TOKEN);
    try seedRunner(conn, RUNNER_B_ID, "roundtrip-b", RUNNER_B_TOKEN);
    try seedActiveFleet(conn);
    try publishFreshEvent(h);

    // A leases through the production path, then dies with its lease still
    // active (see the monotonic-ordering test for why the SQL-shortcut seed no
    // longer works under ready-first); B reclaims under a higher token.
    const lv_a = try leaseAs(h, RUNNER_A_TOKEN);
    defer lv_a.free();
    try std.testing.expect(lv_a.present);
    _ = try conn.exec("UPDATE fleet.runner_affinity SET leased_until = 0 WHERE fleet_id = $1::uuid", .{FLEET_ID});
    const lv = try leaseAs(h, RUNNER_B_TOKEN);
    defer lv.free();
    try std.testing.expect(lv.present);

    telemetry.TestBackend.resetGlobal();

    // 1. Stale fence — A reports on the token B superseded.
    const stale = try reportAs(h, RUNNER_A_TOKEN, lv_a.lease_id.?, lv_a.event_id.?, lv_a.fencing_token);
    defer stale.deinit();
    try stale.expectErrorCode(FENCED_ERROR_CODE);
    try std.testing.expectEqual(@as(u32, 0), completionCaptureCount());

    // 2. Malformed body — rejected before any claim runs.
    const malformed_req = try (try h.post(protocol.PATH_RUNNER_REPORTS)
        .bearer(RUNNER_B_TOKEN)).json("{\"lease_id\":");
    const malformed = try malformed_req.send();
    defer malformed.deinit();
    try malformed.expectStatus(.bad_request);
    try std.testing.expectEqual(@as(u32, 0), completionCaptureCount());

    // 3. Retired lease — A's reclaim-expired lease id offers nothing to claim,
    //    even under B's current token, so nothing settles.
    const unknown = try reportAs(h, RUNNER_B_TOKEN, lv_a.lease_id.?, lv.event_id.?, lv.fencing_token);
    defer unknown.deinit();
    try std.testing.expect(unknown.status != 200);
    try std.testing.expectEqual(@as(u32, 0), completionCaptureCount());

    // A fourth arm — "the settle's own database write fails" — stood here. It
    // provoked the failure by occupying the (event_id, slice_seq) slot the
    // settle was about to take, which raised only because `metering_periods`
    // carried no ON CONFLICT clause. That table is gone and all three writes
    // that replaced it arbitrate their own conflicts (the ledger on
    // (event_id, charge_type), the counters on their primary key, the runner
    // events on a partial unique index), so there is no longer a database error
    // to provoke without a fault-injection seam in production code — which the
    // settle path deliberately does not have. The arm was dropped rather than
    // faked; the fail-closed path keeps its unit coverage.
}
