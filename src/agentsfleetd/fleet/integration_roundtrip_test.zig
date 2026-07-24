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
const SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd01";
const AFFINITY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dde01";
const LEASE_OLD_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddf01";
const METERING_COLLISION_UID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dd0f1";
const FENCED_ERROR_CODE = "UZ-RUN-005";
const SEEDED_EVENT_ID = "evt-seed-1";

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
    try base.seedFleetSession(conn, SESSION_ID, FLEET_ID, "{}");
}

fn seedAffinity(conn: *pg.Conn, last_runner_id: []const u8, fencing_seq: i64, leased_until: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, fleet_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (fleet_id) DO UPDATE
        \\  SET last_runner_id = EXCLUDED.last_runner_id,
        \\      fencing_seq = EXCLUDED.fencing_seq, leased_until = EXCLUDED.leased_until
    , .{ AFFINITY_ID, FLEET_ID, last_runner_id, fencing_seq, leased_until });
}

fn seedActiveLease(conn: *pg.Conn, lease_id: []const u8, runner_id: []const u8, fencing_token: i64) !void {
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
    , .{ lease_id, runner_id, FLEET_ID, WORKSPACE_ID, base.TEST_TENANT_ID, fencing_token, clock.nowMillis() + 60_000 });
}

fn fundLargeBalance(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'roundtrip-test', 0, 0)
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
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(token)).json("{}");
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

/// The slice ordinal the next settle will claim. `claimAndSettle` writes its
/// final slice at `meter_slice_seq + 1`, so this is what a collision must target.
fn nextMeterSliceSeq(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT meter_slice_seq FROM fleet.runner_affinity WHERE fleet_id = $1::uuid",
        .{FLEET_ID},
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.AffinityRowMissing;
    return try row.get(i64, 0) + 1;
}

/// Occupy the slot the settle INSERT is about to take. `metering_periods` has
/// no ON CONFLICT clause, so the unique (event_id, slice_seq) key turns the
/// whole claim+settle statement into a database error — a real failure at the
/// settlement step, with no fault-injection seam in production code.
fn blockNextSettleSlice(conn: *pg.Conn, event_id: []const u8, slice_seq: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.metering_periods
        \\  (uid, event_id, slice_seq, d_input_tokens, d_cached_tokens, d_output_tokens,
        \\   run_ms, run_fee_nanos, token_cost_nanos, charged_nanos, created_at)
        \\VALUES ($1::uuid, $2, $3, 0, 0, 0, 0, 0, 0, 0, 0)
    , .{ METERING_COLLISION_UID, event_id, slice_seq });
}

fn completionCaptureCount() u32 {
    return telemetry.TestBackend.globalCount(.fleet_completed);
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
    // The reclaim test settles under this FIXED event id; without this delete
    // the leaked slice makes any rerun collide on
    // uq_metering_periods_event_id_slice_seq (the sibling renewal/concurrency
    // suites all carry the same cleanup for their own fixed ids).
    execIgnore(conn, "DELETE FROM fleet.metering_periods WHERE event_id = $1", .{SEEDED_EVENT_ID});
    execIgnore(conn, "DELETE FROM fleet.metering_periods WHERE uid = $1::uuid", .{METERING_COLLISION_UID});
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
    try seedRunner(conn, RUNNER_A_ID, "roundtrip-a", RUNNER_A_TOKEN); // dead holder
    try seedRunner(conn, RUNNER_B_ID, "roundtrip-b", RUNNER_B_TOKEN); // reclaimer
    try seedActiveFleet(conn);
    // A holds an expired affinity (claimable) at token 1 + its still-active lease
    // carrying the durable event envelope to re-lease.
    try seedAffinity(conn, RUNNER_A_ID, 1, 0);
    try seedActiveLease(conn, LEASE_OLD_ID, RUNNER_A_ID, 1);

    // B leases → reclaims A's event under a strictly higher token.
    const lv = try leaseAs(h, RUNNER_B_TOKEN);
    defer lv.free();
    try std.testing.expect(lv.present);
    try std.testing.expect(lv.fencing_token > 1); // A's old token (1) < B's new token

    // A's old lease is retired by the reclaim.
    try std.testing.expect(try leaseStatusIs(conn, LEASE_OLD_ID, "expired"));

    // A's late report on the stale token is fenced out (the reclaimed event id
    // is the seeded 'evt-seed-1' B re-leased).
    const a_rep = try reportAs(h, RUNNER_A_TOKEN, LEASE_OLD_ID, SEEDED_EVENT_ID, 1);
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
    try seedAffinity(conn, RUNNER_A_ID, 1, 0);
    try seedActiveLease(conn, LEASE_OLD_ID, RUNNER_A_ID, 1);

    // B reclaims A's event under a strictly higher token.
    const lv = try leaseAs(h, RUNNER_B_TOKEN);
    defer lv.free();
    try std.testing.expect(lv.present);

    telemetry.TestBackend.resetGlobal();

    // 1. Stale fence — A reports on the token B superseded.
    const stale = try reportAs(h, RUNNER_A_TOKEN, LEASE_OLD_ID, SEEDED_EVENT_ID, 1);
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

    // 3. Unknown lease — no lease to claim, so nothing settles.
    const unknown = try reportAs(h, RUNNER_B_TOKEN, LEASE_OLD_ID, lv.event_id.?, lv.fencing_token);
    defer unknown.deinit();
    try std.testing.expect(unknown.status != 200);
    try std.testing.expectEqual(@as(u32, 0), completionCaptureCount());

    // 4. Database failure at settlement — the slice this settle would write is
    //    already taken, so claim+settle raises and the report answers 500.
    const seq = try nextMeterSliceSeq(conn);
    try blockNextSettleSlice(conn, lv.event_id.?, seq);
    const db_failed = try reportAs(h, RUNNER_B_TOKEN, lv.lease_id.?, lv.event_id.?, lv.fencing_token);
    defer db_failed.deinit();
    try std.testing.expect(db_failed.status >= 500);
    try std.testing.expectEqual(@as(u32, 0), completionCaptureCount());
}
