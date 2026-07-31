// Retention sweep proofs for `fleet.runner_leases` / `fleet.runner_events`.
// The sweeper deletes ONLY terminal-status lease rows and only rows older than
// the retention window — live work and in-window history are untouchable by
// construction — and its cycle totals are what the maintenance metric reports.
// Runs `sweepOnce` (and, for the metric, the real `run` loop) against the live
// schema. Requires TEST_DATABASE_URL; self-skips otherwise.

const std = @import("std");
const constants = @import("common");
const clock = constants.clock;
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const base = @import("../db/test_fixtures.zig");
const protocol = @import("contract").protocol;
const retention_sweeper = @import("retention_sweeper.zig");
const mc = @import("../observability/metrics_counters.zig");
const id_format = @import("../types/id_format.zig");

const ALLOC = std.testing.allocator;

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-3c0e1e0d0011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-3c0e1e0d0a01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-3c0e1e0d0c01";
const L_AGED_REPORTED_ONE = "0195b4ba-8d3a-7f13-8abc-3c0e1e0d0f01";
const L_AGED_REPORTED_TWO = "0195b4ba-8d3a-7f13-8abc-3c0e1e0d0f02";
const L_AGED_EXPIRED = "0195b4ba-8d3a-7f13-8abc-3c0e1e0d0f03";
const L_ACTIVE_OLD = "0195b4ba-8d3a-7f13-8abc-3c0e1e0d0f04";
const L_RECENT_REPORTED = "0195b4ba-8d3a-7f13-8abc-3c0e1e0d0f05";

/// Mirrors the sweeper's retention window; a drift there is a behavior change
/// this suite must surface, so the value is pinned rather than imported.
const RETENTION_WINDOW_MS: i64 = 30 * std.time.ms_per_day;
/// Seed aged rows one full day past the window so clock skew between the
/// test's cutoff and the sweeper's cannot flip eligibility.
const AGE_SAFETY_MS: i64 = std.time.ms_per_day;
const EVENT_PREFIX = "evt-ret-";
const AGED_EVENT_ROWS = 4;
const RECENT_EVENT_ROWS = 2;
const METRIC_POLL_ATTEMPTS = 500;
const METRIC_POLL_STEP_NS: u64 = 20 * std.time.ns_per_ms;

fn seedRunner(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners (id, host_id, token_hash, sandbox_tier, admin_state,
        \\   labels, tenant_id, last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'retention-host', 'retention-hash', 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{RUNNER_ID});
}

fn seedLease(conn: *pg.Conn, lease_id: []const u8, event_id: []const u8, status: []const u8, created_at: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases (id, runner_id, fleet_id, workspace_id, tenant_id,
        \\   event_id, actor, event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, 'steer:retention-test', 'chat',
        \\   '{}', 0, 'platform', 'test-provider', 'test-model', 0, 0, 0, 0, 1, $7, $8, $7, $7)
        \\ON CONFLICT (id) DO NOTHING
    , .{ lease_id, RUNNER_ID, FLEET_ID, WORKSPACE_ID, base.TEST_TENANT_ID, event_id, created_at, status });
}

fn seedEvent(conn: *pg.Conn, occurred_at: i64) !void {
    const event_uid = try id_format.generateUuidV7();
    const event_id: []const u8 = &event_uid;
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_events (id, runner_id, event_type, occurred_at, metadata, dedup_key, created_at)
        \\VALUES ($1::uuid, $2::uuid, $3::text, $4::bigint, '{}'::jsonb, NULL, $4::bigint)
    , .{ event_id, RUNNER_ID, @tagName(protocol.RunnerEventType.lease_acquired), occurred_at });
}

fn setupBase(conn: *pg.Conn) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "retention-fleet", "{}", "# z");
    try seedRunner(conn);
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn cleanup(conn: *pg.Conn) void {
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE runner_id = $1::uuid", .{RUNNER_ID});
    // Cascades this suite's runner_events rows with the runner.
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    base.teardownFleets(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

fn scalarI64(conn: *pg.Conn, sql: []const u8, args: anytype) !i64 {
    var q = PgQuery.from(try conn.query(sql, args));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return row.get(i64, 0);
}

/// Sweep-eligible lease rows table-wide, with the sweeper's own predicate —
/// the totals assertion counts what the sweep counts, so residue from an
/// earlier crashed suite cannot skew equality.
fn agedTerminalLeaseCount(conn: *pg.Conn, cutoff: i64) !i64 {
    const terminal = [_][]const u8{
        protocol.RUNNER_LEASE_STATUS_REPORTED,
        protocol.RUNNER_LEASE_STATUS_EXPIRED,
    };
    return scalarI64(conn,
        \\SELECT COUNT(*)::bigint FROM fleet.runner_leases
        \\WHERE status = ANY($1::text[]) AND created_at < $2
    , .{ &terminal, cutoff });
}

fn agedEventCount(conn: *pg.Conn, cutoff: i64) !i64 {
    return scalarI64(conn, "SELECT COUNT(*)::bigint FROM fleet.runner_events WHERE occurred_at < $1", .{cutoff});
}

fn leaseExists(conn: *pg.Conn, lease_id: []const u8) !bool {
    return (try scalarI64(conn, "SELECT COUNT(*)::bigint FROM fleet.runner_leases WHERE id = $1::uuid", .{lease_id})) == 1;
}

fn runnerEventCount(conn: *pg.Conn) !i64 {
    return scalarI64(conn, "SELECT COUNT(*)::bigint FROM fleet.runner_events WHERE runner_id = $1::uuid", .{RUNNER_ID});
}

/// Aged terminal history plus the two rows the sweep must spare: a still-live
/// old lease and an in-window terminal one, with events on both sides of the
/// window. Returns the aged instant used.
fn seedRetentionFixture(conn: *pg.Conn) !i64 {
    const now_ms = clock.nowMillis();
    const aged_at = now_ms - RETENTION_WINDOW_MS - AGE_SAFETY_MS;
    try seedLease(conn, L_AGED_REPORTED_ONE, EVENT_PREFIX ++ "aged-1", protocol.RUNNER_LEASE_STATUS_REPORTED, aged_at);
    try seedLease(conn, L_AGED_REPORTED_TWO, EVENT_PREFIX ++ "aged-2", protocol.RUNNER_LEASE_STATUS_REPORTED, aged_at);
    try seedLease(conn, L_AGED_EXPIRED, EVENT_PREFIX ++ "aged-3", protocol.RUNNER_LEASE_STATUS_EXPIRED, aged_at);
    try seedLease(conn, L_ACTIVE_OLD, EVENT_PREFIX ++ "live-1", protocol.RUNNER_LEASE_STATUS_ACTIVE, aged_at);
    try seedLease(conn, L_RECENT_REPORTED, EVENT_PREFIX ++ "recent-1", protocol.RUNNER_LEASE_STATUS_REPORTED, now_ms);
    var i: usize = 0;
    while (i < AGED_EVENT_ROWS) : (i += 1) try seedEvent(conn, aged_at + @as(i64, @intCast(i)));
    i = 0;
    while (i < RECENT_EVENT_ROWS) : (i += 1) try seedEvent(conn, now_ms + @as(i64, @intCast(i)));
    return aged_at;
}

test "one sweep deletes aged terminal history and spares live and in-window rows" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    try setupBase(ctx.conn);
    _ = try seedRetentionFixture(ctx.conn);

    // Count eligibility with the sweeper's own predicates just before the
    // sweep, so the totals assertion is exact even against foreign residue.
    const cutoff = clock.nowMillis() - RETENTION_WINDOW_MS;
    const eligible_leases = try agedTerminalLeaseCount(ctx.conn, cutoff);
    const eligible_events = try agedEventCount(ctx.conn, cutoff);
    try std.testing.expect(eligible_leases >= 3);
    try std.testing.expect(eligible_events >= AGED_EVENT_ROWS);

    const totals = try retention_sweeper.sweepOnce(ctx.pool);
    try std.testing.expectEqual(eligible_leases, totals.leases_deleted);
    try std.testing.expectEqual(eligible_events, totals.events_deleted);

    // Aged terminal rows are gone; the live-old and in-window terminal rows
    // survive, and only the in-window events remain.
    try std.testing.expect(!try leaseExists(ctx.conn, L_AGED_REPORTED_ONE));
    try std.testing.expect(!try leaseExists(ctx.conn, L_AGED_REPORTED_TWO));
    try std.testing.expect(!try leaseExists(ctx.conn, L_AGED_EXPIRED));
    try std.testing.expect(try leaseExists(ctx.conn, L_ACTIVE_OLD));
    try std.testing.expect(try leaseExists(ctx.conn, L_RECENT_REPORTED));
    try std.testing.expectEqual(@as(i64, RECENT_EVENT_ROWS), try runnerEventCount(ctx.conn));

    // A second cycle finds nothing left to do — the sweep converges.
    const again = try retention_sweeper.sweepOnce(ctx.pool);
    try std.testing.expectEqual(@as(i64, 0), again.leases_deleted);
    try std.testing.expectEqual(@as(i64, 0), again.events_deleted);

    cleanup(ctx.conn);
}

test "sweep loop reports deleted rows to the retention metric" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    try setupBase(ctx.conn);
    _ = try seedRetentionFixture(ctx.conn);

    const cutoff = clock.nowMillis() - RETENTION_WINDOW_MS;
    const expected: u64 = @intCast((try agedTerminalLeaseCount(ctx.conn, cutoff)) + (try agedEventCount(ctx.conn, cutoff)));
    try std.testing.expect(expected > 0);

    // The metric is written by the run loop, not by `sweepOnce` — drive the
    // real loop with its shutdown flag: first cycle sweeps and reports, the
    // interruptible sleep then honors the stop within its poll slice.
    mc.resetRunnerMaintenanceMetricsForTest();
    var shutdown = std.atomic.Value(bool).init(false);
    const sweeper_thread = try std.Thread.spawn(.{}, retention_sweeper.run, .{ ctx.pool, &shutdown });
    var attempt: usize = 0;
    while (attempt < METRIC_POLL_ATTEMPTS) : (attempt += 1) {
        if (mc.snapshot().runner_retention_swept_total > 0) break;
        constants.sleepNanos(METRIC_POLL_STEP_NS);
    }
    shutdown.store(true, .release); // safe because: pairs with the run loop's acquire-load stop checks.
    sweeper_thread.join();

    try std.testing.expectEqual(expected, mc.snapshot().runner_retention_swept_total);
    try std.testing.expect(try leaseExists(ctx.conn, L_ACTIVE_OLD));
    try std.testing.expect(!try leaseExists(ctx.conn, L_AGED_EXPIRED));

    cleanup(ctx.conn);
}
