// Integration tests for runner liveness sweep and lease-slot expiry.

const std = @import("std");
const constants = @import("common");
const clock = constants.clock;
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const base = @import("../db/test_fixtures.zig");
const affinity = @import("affinity.zig");
const reclaim = @import("reclaim.zig");
const sweeper = @import("liveness_sweeper.zig");
const protocol = @import("contract").protocol;

const ALLOC = std.testing.allocator;

const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e106011";
const RUNNER_STALE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e106a01";
const RUNNER_LIVE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e106b01";
const AGENTSFLEET_ONE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e106c01";
const AGENTSFLEET_TWO_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e106c02";
const AFFINITY_ONE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e106e01";
const LEASE_ONE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e106f01";
const LEASE_TWO_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e106f02";
const STALE_HASH = "runner_liveness_stale_hash";
const LIVE_HASH = "runner_liveness_live_hash";
const STALE_HOST = "runner-liveness-stale";
const LIVE_HOST = "runner-liveness-live";
const EVENT_ID_ONE = "evt-liveness-one";
const EVENT_ID_TWO = "evt-liveness-two";
const ACTOR = "steer:liveness-test";
const REQUEST_JSON = "{\"message\":\"liveness\"}";
const TEST_POSTURE = "platform";
const TEST_PROVIDER = "test-provider";
const TEST_MODEL = "test-model";
const CONCURRENT_SWEEPERS: usize = 4;

fn seedRunner(conn: *pg.Conn, runner_id: []const u8, host_id: []const u8, token_hash: []const u8, state: protocol.AdminState, last_seen_at: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, 'dev_none', $4, '[]'::jsonb, NULL, $5, $5, $5)
        \\ON CONFLICT (id) DO UPDATE
        \\  SET admin_state = EXCLUDED.admin_state,
        \\      last_seen_at = EXCLUDED.last_seen_at,
        \\      updated_at = EXCLUDED.updated_at
    , .{ runner_id, host_id, token_hash, @tagName(state), last_seen_at });
}

fn seedAffinity(conn: *pg.Conn, affinity_id: []const u8, fleet_id: []const u8, runner_id: []const u8, leased_until: i64, token: i64) !void {
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
    , .{ affinity_id, fleet_id, runner_id, token, leased_until });
}

fn seedLease(conn: *pg.Conn, lease_id: []const u8, runner_id: []const u8, fleet_id: []const u8, event_id: []const u8, token: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens,
        \\   last_metered_at_ms, fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, $7,
        \\        'chat', $8, 0, $9, $10, $11, 0, 0, 0, 0, $12, $13, $14, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{
        lease_id,
        runner_id,
        fleet_id,
        WORKSPACE_ID,
        base.TEST_TENANT_ID,
        event_id,
        ACTOR,
        REQUEST_JSON,
        TEST_POSTURE,
        TEST_PROVIDER,
        TEST_MODEL,
        token,
        clock.nowMillis() + constants.LEASE_TTL_MS,
        protocol.RUNNER_LEASE_STATUS_ACTIVE,
    });
}

/// Seed the tenant/workspace/fleet rows the lease + affinity inserts now depend
/// on (runner_leases.fleet_id / runner_affinity.fleet_id → core.fleets FK).
/// Idempotent; both fleets are seeded unconditionally (an unused fleet row does
/// not affect any per-runner lease/event count assertion).
fn seedFleetBase(conn: *pg.Conn) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedFleet(conn, AGENTSFLEET_ONE_ID, WORKSPACE_ID, "liveness-one", "{}", "# z");
    try base.seedFleet(conn, AGENTSFLEET_TWO_ID, WORKSPACE_ID, "liveness-two", "{}", "# z");
}

fn cleanup(conn: *pg.Conn) void {
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id IN ($1::uuid, $2::uuid)", .{ AGENTSFLEET_ONE_ID, AGENTSFLEET_TWO_ID });
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id IN ($1::uuid, $2::uuid)", .{ RUNNER_STALE_ID, RUNNER_LIVE_ID });
    // Fleets (cascades any residual lease/affinity via the new FK) before the
    // workspace — core.fleets.workspace_id has no on-delete-cascade.
    base.teardownFleets(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| {
        std.log.warn("ignored liveness sweeper cleanup error: {s}", .{@errorName(err)});
        return;
    };
}

fn scalarI64(conn: *pg.Conn, sql: []const u8, args: anytype) !i64 {
    var q = PgQuery.from(try conn.query(sql, args));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return row.get(i64, 0);
}

fn eventCount(conn: *pg.Conn, event_type: protocol.RunnerEventType) !i64 {
    return scalarI64(conn,
        \\SELECT COUNT(*)::bigint FROM fleet.runner_events
        \\WHERE runner_id = $1::uuid AND event_type = $2
    , .{ RUNNER_STALE_ID, @tagName(event_type) });
}

fn activeCount(conn: *pg.Conn, runner_id: []const u8) !i64 {
    return scalarI64(conn,
        \\SELECT COUNT(*)::bigint FROM fleet.runner_leases
        \\WHERE runner_id = $1::uuid AND status = $2
    , .{ runner_id, protocol.RUNNER_LEASE_STATUS_ACTIVE });
}

fn leaseStatus(conn: *pg.Conn, lease_id: []const u8) ![]const u8 {
    var q = PgQuery.from(try conn.query("SELECT status FROM fleet.runner_leases WHERE id = $1::uuid", .{lease_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return row.get([]const u8, 0);
}

fn runnerState(conn: *pg.Conn, runner_id: []const u8) !protocol.AdminState {
    var q = PgQuery.from(try conn.query("SELECT admin_state FROM fleet.runners WHERE id = $1::uuid", .{runner_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const raw = try row.get([]const u8, 0);
    return std.meta.stringToEnum(protocol.AdminState, raw) orelse error.TestUnexpectedResult;
}

fn leasedUntil(conn: *pg.Conn, fleet_id: []const u8) !i64 {
    return scalarI64(conn, "SELECT leased_until FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{fleet_id});
}

fn seedStaleActiveLease(conn: *pg.Conn, fleet_id: []const u8, affinity_id: []const u8, lease_id: []const u8, event_id: []const u8, token: i64) !void {
    const now_ms = clock.nowMillis();
    try seedRunner(conn, RUNNER_STALE_ID, STALE_HOST, STALE_HASH, .active, now_ms - constants.RUNNER_OFFLINE_AFTER_MS - constants.HEARTBEAT_INTERVAL_MS);
    try seedAffinity(conn, affinity_id, fleet_id, RUNNER_STALE_ID, now_ms + constants.LEASE_TTL_MS, token);
    try seedLease(conn, lease_id, RUNNER_STALE_ID, fleet_id, event_id, token);
}

fn reclaimAsLive(conn: *pg.Conn, fleet_id: []const u8) !void {
    const claim = try affinity.claim(conn, ALLOC, fleet_id, RUNNER_LIVE_ID, constants.LEASE_TTL_MS);
    const won = switch (claim) {
        .won => |w| w,
        .taken => return error.TestUnexpectedResult,
    };
    try std.testing.expect(won.token > 0);
    const prior = (try reclaim.reclaimPriorActive(conn, ALLOC, fleet_id)) orelse return error.TestUnexpectedResult;
    defer freePrior(prior);
}

fn freePrior(prior: reclaim.PriorLease) void {
    ALLOC.free(prior.lease_id);
    ALLOC.free(prior.event_id);
    ALLOC.free(prior.actor);
    ALLOC.free(prior.event_type);
    ALLOC.free(prior.request_json);
    ALLOC.free(prior.workspace_id);
    ALLOC.free(prior.tenant_id);
    ALLOC.free(prior.posture);
    ALLOC.free(prior.model);
}

test "stale runner swept and work reassigned" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    defer cleanup(ctx.conn);
    try seedFleetBase(ctx.conn);

    try seedStaleActiveLease(ctx.conn, AGENTSFLEET_ONE_ID, AFFINITY_ONE_ID, LEASE_ONE_ID, EVENT_ID_ONE, 1);
    try seedRunner(ctx.conn, RUNNER_LIVE_ID, LIVE_HOST, LIVE_HASH, .active, clock.nowMillis());
    const stats = try sweeper.sweepOnce(ctx.pool, ALLOC);

    try std.testing.expectEqual(@as(i64, 1), stats.offline_events);
    try std.testing.expectEqual(@as(i64, 1), try eventCount(ctx.conn, .runner_offline));
    try std.testing.expect(try leasedUntil(ctx.conn, AGENTSFLEET_ONE_ID) < clock.nowMillis());

    try reclaimAsLive(ctx.conn, AGENTSFLEET_ONE_ID);
    try std.testing.expectEqualStrings(protocol.RUNNER_LEASE_STATUS_EXPIRED, try leaseStatus(ctx.conn, LEASE_ONE_ID));
}

test "reassignment holds when no eligible target" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    defer cleanup(ctx.conn);
    try seedFleetBase(ctx.conn);

    try seedStaleActiveLease(ctx.conn, AGENTSFLEET_ONE_ID, AFFINITY_ONE_ID, LEASE_ONE_ID, EVENT_ID_ONE, 1);
    _ = try sweeper.sweepOnce(ctx.pool, ALLOC);
    try std.testing.expectEqualStrings(protocol.RUNNER_LEASE_STATUS_ACTIVE, try leaseStatus(ctx.conn, LEASE_ONE_ID));

    try seedRunner(ctx.conn, RUNNER_LIVE_ID, LIVE_HOST, LIVE_HASH, .active, clock.nowMillis());
    try reclaimAsLive(ctx.conn, AGENTSFLEET_ONE_ID);
    try std.testing.expectEqualStrings(protocol.RUNNER_LEASE_STATUS_EXPIRED, try leaseStatus(ctx.conn, LEASE_ONE_ID));
}

test "liveness derives active lease set without singular column" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    defer cleanup(ctx.conn);
    try seedFleetBase(ctx.conn);

    try seedRunner(ctx.conn, RUNNER_STALE_ID, STALE_HOST, STALE_HASH, .active, clock.nowMillis());
    try std.testing.expectEqual(@as(i64, 0), try activeCount(ctx.conn, RUNNER_STALE_ID));
    try seedLease(ctx.conn, LEASE_ONE_ID, RUNNER_STALE_ID, AGENTSFLEET_ONE_ID, EVENT_ID_ONE, 1);
    try std.testing.expectEqual(@as(i64, 1), try activeCount(ctx.conn, RUNNER_STALE_ID));
    try seedLease(ctx.conn, LEASE_TWO_ID, RUNNER_STALE_ID, AGENTSFLEET_TWO_ID, EVENT_ID_TWO, 2);
    try std.testing.expectEqual(@as(i64, 2), try activeCount(ctx.conn, RUNNER_STALE_ID));
    try std.testing.expectEqual(@as(i64, 0), try scalarI64(ctx.conn,
        \\SELECT COUNT(*)::bigint FROM information_schema.columns
        \\WHERE table_schema = 'fleet' AND table_name = 'runners' AND column_name = 'current_lease_id'
    , .{}));
}

test "idle draining runner becomes drained on the next sweep" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    defer cleanup(ctx.conn);
    try seedFleetBase(ctx.conn);

    try seedRunner(ctx.conn, RUNNER_STALE_ID, STALE_HOST, STALE_HASH, .draining, clock.nowMillis());
    try std.testing.expectEqual(@as(i64, 0), try activeCount(ctx.conn, RUNNER_STALE_ID));

    const stats = try sweeper.sweepOnce(ctx.pool, ALLOC);
    try std.testing.expectEqual(@as(i64, 1), stats.drained_runners);
    try std.testing.expectEqual(protocol.AdminState.drained, try runnerState(ctx.conn, RUNNER_STALE_ID));
    try std.testing.expectEqual(@as(i64, 1), try eventCount(ctx.conn, .runner_drained));
}

const SweepWorker = struct {
    pool: *pg.Pool,
    err: ?anyerror = null,

    fn run(self: *SweepWorker) void {
        _ = sweeper.sweepOnce(self.pool, std.heap.page_allocator) catch |err| {
            self.err = err;
        };
    }
};

test "concurrent sweepers emit one offline event" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    defer cleanup(ctx.conn);
    try seedFleetBase(ctx.conn);

    try seedStaleActiveLease(ctx.conn, AGENTSFLEET_ONE_ID, AFFINITY_ONE_ID, LEASE_ONE_ID, EVENT_ID_ONE, 1);
    var workers: [CONCURRENT_SWEEPERS]SweepWorker = undefined;
    var threads: [CONCURRENT_SWEEPERS]std.Thread = undefined;
    for (&workers, &threads) |*worker, *thread| {
        worker.* = .{ .pool = ctx.pool };
        thread.* = try std.Thread.spawn(.{}, SweepWorker.run, .{worker});
    }
    for (&threads) |*thread| thread.join();
    for (&workers) |worker| try std.testing.expect(worker.err == null);
    try std.testing.expectEqual(@as(i64, 1), try eventCount(ctx.conn, .runner_offline));
}
