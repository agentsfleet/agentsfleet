// Integration tests for the runner-memory handler's lease authorization — the
// fencing + IDOR cross-check that gates BOTH verbs:
//   POST /v1/runners/me/memory/{fleet_id}  → pushLeaseSeq (capture)
//   GET  /v1/runners/me/memory/{fleet_id}  → liveLeaseSeq (hydrate)
//
// Drives the resolver functions directly against seeded fleet rows on a live
// test-DB connection (mirrors renewal_integration_test): the live seq is
// COALESCE(affinity.fencing_seq, lease.fencing_token), so a reclaim that bumped
// the slot strands the old holder below it (stale fence → handler rejects). A
// lease that exists but for another fleet/lease yields null — the IDOR guard
// is the WHERE itself. Requires LIVE_DB=1; skips when TEST_DATABASE_URL is unset.

const std = @import("std");
const pg = @import("pg");
const base = @import("../../../db/test_fixtures.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const auth_mw = @import("../../../auth/middleware/mod.zig");
const memory = @import("memory.zig");

const ALLOC = std.testing.allocator;

// Distinct UUIDv7 literals (no collision with sibling fleet tests).
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e8011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e8a01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e8c01";
const OTHER_AGENT = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e8c99";
const AFFINITY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e8e01";
const LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e8f01";
const OTHER_LEASE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e8f99";
const EVENT_ID = "evt-mem-fence-1";
const NOW_MS: i64 = 1_900_000_000_000;

fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn seedRunner(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'mem-host', 'mem-hash', 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{RUNNER_ID});
}

fn seedAffinity(conn: *pg.Conn, fencing_seq: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, fleet_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (fleet_id) DO UPDATE SET fencing_seq = EXCLUDED.fencing_seq
    , .{ AFFINITY_ID, FLEET_ID, RUNNER_ID, fencing_seq, NOW_MS + 30_000 });
}

/// Seed an active, unexpired lease for the runner naming `lease_id`/`fleet_id`.
fn seedLease(conn: *pg.Conn, lease_id: []const u8, fleet_id: []const u8, fencing_token: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, 'steer:test',
        \\        'chat', '{"message":"hi"}', 0, 'platform', 'p', 'm', 0, 0, 0, 0,
        \\        $7, $8, 'active', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ lease_id, RUNNER_ID, fleet_id, WORKSPACE_ID, base.TEST_TENANT_ID, EVENT_ID, fencing_token, NOW_MS + 30_000 });
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn teardown(conn: *pg.Conn) void {
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE id IN ($1::uuid, $2::uuid)", .{ LEASE_ID, OTHER_LEASE });
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

fn harnessConn() !?struct { h: *TestHarness, conn: *pg.Conn } {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = noopRegistry }) catch |err| {
        if (err == error.SkipZigTest) return null;
        return err;
    };
    const conn = try h.acquireConn();
    return .{ .h = h, .conn = conn };
}

test "pushLeaseSeq: held lease returns the live seq; wrong fleet/lease is null (IDOR)" {
    const ctx = (try harnessConn()) orelse return error.SkipZigTest;
    defer ctx.h.deinit();
    defer ctx.h.releaseConn(ctx.conn);
    const conn = ctx.conn;

    teardown(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn);
    try seedLease(conn, LEASE_ID, FLEET_ID, 5);
    defer teardown(conn);

    // Held: COALESCE(no affinity → lease.fencing_token) = 5.
    try std.testing.expectEqual(@as(?u64, 5), try memory.pushLeaseSeq(conn, RUNNER_ID, LEASE_ID, FLEET_ID, NOW_MS));
    // IDOR: a lease that exists but is named with another fleet → null.
    try std.testing.expectEqual(@as(?u64, null), try memory.pushLeaseSeq(conn, RUNNER_ID, LEASE_ID, OTHER_AGENT, NOW_MS));
    // A lease_id the runner does not hold → null.
    try std.testing.expectEqual(@as(?u64, null), try memory.pushLeaseSeq(conn, RUNNER_ID, OTHER_LEASE, FLEET_ID, NOW_MS));
}

test "pushLeaseSeq: a reclaim that bumped the affinity fence strands the old holder above its token" {
    const ctx = (try harnessConn()) orelse return error.SkipZigTest;
    defer ctx.h.deinit();
    defer ctx.h.releaseConn(ctx.conn);
    const conn = ctx.conn;

    teardown(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn);
    try seedLease(conn, LEASE_ID, FLEET_ID, 5);
    try seedAffinity(conn, 7); // a newer holder bumped the slot to seq 7
    defer teardown(conn);

    // Live seq is the affinity seq (7), so a push fenced at token 5 (< 7) is
    // rejected by the handler as stale — the resolver surfaces the higher seq.
    try std.testing.expectEqual(@as(?u64, 7), try memory.pushLeaseSeq(conn, RUNNER_ID, LEASE_ID, FLEET_ID, NOW_MS));
}

test "liveLeaseSeq: hydrate authorizes a held fleet; an unheld fleet is null" {
    const ctx = (try harnessConn()) orelse return error.SkipZigTest;
    defer ctx.h.deinit();
    defer ctx.h.releaseConn(ctx.conn);
    const conn = ctx.conn;

    teardown(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn);
    try seedLease(conn, LEASE_ID, FLEET_ID, 5);
    defer teardown(conn);

    try std.testing.expectEqual(@as(?u64, 5), try memory.liveLeaseSeq(conn, RUNNER_ID, FLEET_ID, NOW_MS));
    // No live lease for this fleet → null (hydrate rejected UZ-RUN-005).
    try std.testing.expectEqual(@as(?u64, null), try memory.liveLeaseSeq(conn, RUNNER_ID, OTHER_AGENT, NOW_MS));
}
