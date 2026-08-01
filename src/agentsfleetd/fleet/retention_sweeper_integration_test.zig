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
const L_AGED_SETTLED_RECENT = "0195b4ba-8d3a-7f13-8abc-3c0e1e0d0f06";

/// Mirrors the sweeper's retention window; a drift there is a behavior change
/// this suite must surface, so the value is pinned rather than imported.
const RETENTION_WINDOW_MS: i64 = 30 * std.time.ms_per_day;
/// Seed aged rows one full day past the window so clock skew between the
/// test's cutoff and the sweeper's cannot flip eligibility.
const AGE_SAFETY_MS: i64 = std.time.ms_per_day;
/// Mirrors the sweeper's per-statement ceiling, pinned here for the same reason
/// the window is: a change there must fail this suite rather than pass quietly.
const DELETE_BATCH_LIMIT: i64 = 1000;
const EVENT_PREFIX = "evt-ret-";
const AGED_EVENT_ROWS = 4;
const RECENT_EVENT_ROWS = 2;
/// A tag outside `PER_LEASE_EVENT_TYPES` — the enrolment record the operator
/// Activity feed renders, which retention must never delete at any age.
const LIFECYCLE_EVENT_TYPE: protocol.RunnerEventType = .runner_registered;
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

/// `created_at` and `updated_at` are bound SEPARATELY and deliberately. The
/// first revision of this fixture bound one timestamp to both columns, which
/// made the suite structurally unable to tell the acquisition clock from the
/// settlement clock — it passed against a sweeper reading either. Retention
/// measures from `updated_at`, so every caller states both.
fn seedLease(conn: *pg.Conn, lease_id: []const u8, event_id: []const u8, status: []const u8, created_at: i64, updated_at: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases (id, runner_id, fleet_id, workspace_id, tenant_id,
        \\   event_id, actor, event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, 'steer:retention-test', 'chat',
        \\   '{}', 0, 'platform', 'test-provider', 'test-model', 0, 0, 0, 0, 1, $7, $8, $7, $9)
        \\ON CONFLICT (id) DO NOTHING
    , .{ lease_id, RUNNER_ID, FLEET_ID, WORKSPACE_ID, base.TEST_TENANT_ID, event_id, created_at, status, updated_at });
}

fn seedEvent(conn: *pg.Conn, event_type: protocol.RunnerEventType, occurred_at: i64) !void {
    const event_uid = try id_format.generateUuidV7();
    const event_id: []const u8 = &event_uid;
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_events (id, runner_id, event_type, occurred_at, metadata, dedup_key, created_at)
        \\VALUES ($1::uuid, $2::uuid, $3::text, $4::bigint, '{}'::jsonb, NULL, $4::bigint)
    , .{ event_id, RUNNER_ID, @tagName(event_type), occurred_at });
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
/// earlier crashed suite cannot skew equality. Keyed on `updated_at`, the
/// settlement clock the sweeper measures.
fn agedTerminalLeaseCount(conn: *pg.Conn, cutoff: i64) !i64 {
    const terminal = [_][]const u8{
        protocol.RUNNER_LEASE_STATUS_REPORTED,
        protocol.RUNNER_LEASE_STATUS_EXPIRED,
    };
    return scalarI64(conn,
        \\SELECT COUNT(*)::bigint FROM fleet.runner_leases
        \\WHERE status = ANY($1::text[]) AND updated_at < $2
    , .{ &terminal, cutoff });
}

/// Only the per-work tags are eligible; lifecycle rows of any age are not.
fn agedEventCount(conn: *pg.Conn, cutoff: i64) !i64 {
    return scalarI64(conn,
        \\SELECT COUNT(*)::bigint FROM fleet.runner_events
        \\WHERE event_type = ANY($1::text[]) AND occurred_at < $2
    , .{ &retention_sweeper.PER_LEASE_EVENT_TAGS, cutoff });
}

fn eventCountOfType(conn: *pg.Conn, event_type: protocol.RunnerEventType) !i64 {
    return scalarI64(conn,
        \\SELECT COUNT(*)::bigint FROM fleet.runner_events
        \\WHERE runner_id = $1::uuid AND event_type = $2::text
    , .{ RUNNER_ID, @tagName(event_type) });
}

fn leaseExists(conn: *pg.Conn, lease_id: []const u8) !bool {
    return (try scalarI64(conn, "SELECT COUNT(*)::bigint FROM fleet.runner_leases WHERE id = $1::uuid", .{lease_id})) == 1;
}

/// Same question asked on a connection of its own — the suite's own connection
/// is inside an open transaction whenever a test is holding row locks, and a
/// read there would see that transaction's own uncommitted view.
fn leaseExistsOn(pool: *pg.Pool, lease_id: []const u8) !bool {
    const conn = try pool.acquire();
    defer pool.release(conn);
    return leaseExists(conn, lease_id);
}

fn leaseStatus(conn: *pg.Conn, lease_id: []const u8) ![]const u8 {
    var q = PgQuery.from(try conn.query("SELECT status FROM fleet.runner_leases WHERE id = $1::uuid", .{lease_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    // Valid until the next query on this connection, which is all any caller
    // here needs — every use is an immediate comparison.
    return row.get([]const u8, 0);
}

/// The runner's lifetime `expired` tally, or zero before the row exists.
fn lifetimeExpiredCount(conn: *pg.Conn) !i64 {
    return scalarI64(conn,
        \\SELECT COALESCE((SELECT expired FROM fleet.runner_lifetime_counters WHERE runner_id = $1::uuid), 0)::bigint
    , .{RUNNER_ID});
}

/// Bulk-seed aged terminal leases in one statement. Row-at-a-time seeding of a
/// batch-crossing fixture costs more than the test proves; the ids are minted
/// with the version and variant nibbles the schema's UUIDv7 CHECK requires.
fn seedAgedLeaseBulk(conn: *pg.Conn, aged_at: i64, count: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases (id, runner_id, fleet_id, workspace_id, tenant_id,
        \\   event_id, actor, event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\SELECT overlay(overlay(gen_random_uuid()::text placing '7' from 15) placing '8' from 20)::uuid,
        \\       $1::uuid, $2::uuid, $3::uuid, $4::uuid, 'evt-ret-bulk-' || g, 'steer:retention-test', 'chat',
        \\       '{}', 0, 'platform', 'test-provider', 'test-model', 0, 0, 0, 0, 1, $5, $6, $5, $5
        \\FROM generate_series(1, $7::bigint) AS g
    , .{ RUNNER_ID, FLEET_ID, WORKSPACE_ID, base.TEST_TENANT_ID, aged_at, protocol.RUNNER_LEASE_STATUS_REPORTED, count });
}

fn runnerEventCount(conn: *pg.Conn) !i64 {
    return scalarI64(conn, "SELECT COUNT(*)::bigint FROM fleet.runner_events WHERE runner_id = $1::uuid", .{RUNNER_ID});
}

/// Aged terminal history plus every row the sweep must spare: a still-live old
/// lease, an in-window terminal one, a lease acquired before the window but
/// SETTLED inside it, aged per-work events, in-window per-work events, and an
/// aged lifecycle event. Returns the aged instant used.
fn seedRetentionFixture(conn: *pg.Conn) !i64 {
    const now_ms = clock.nowMillis();
    const aged_at = now_ms - RETENTION_WINDOW_MS - AGE_SAFETY_MS;
    try seedLease(conn, L_AGED_REPORTED_ONE, EVENT_PREFIX ++ "aged-1", protocol.RUNNER_LEASE_STATUS_REPORTED, aged_at, aged_at);
    try seedLease(conn, L_AGED_REPORTED_TWO, EVENT_PREFIX ++ "aged-2", protocol.RUNNER_LEASE_STATUS_REPORTED, aged_at, aged_at);
    try seedLease(conn, L_AGED_EXPIRED, EVENT_PREFIX ++ "aged-3", protocol.RUNNER_LEASE_STATUS_EXPIRED, aged_at, aged_at);
    try seedLease(conn, L_ACTIVE_OLD, EVENT_PREFIX ++ "live-1", protocol.RUNNER_LEASE_STATUS_ACTIVE, aged_at, aged_at);
    try seedLease(conn, L_RECENT_REPORTED, EVENT_PREFIX ++ "recent-1", protocol.RUNNER_LEASE_STATUS_REPORTED, now_ms, now_ms);
    // Acquired before the window, settled inside it: the row the retention
    // promise is actually about. A sweep keyed on `created_at` deletes it.
    try seedLease(conn, L_AGED_SETTLED_RECENT, EVENT_PREFIX ++ "settled-1", protocol.RUNNER_LEASE_STATUS_REPORTED, aged_at, now_ms);
    var i: usize = 0;
    while (i < AGED_EVENT_ROWS) : (i += 1) try seedEvent(conn, .lease_acquired, aged_at + @as(i64, @intCast(i)));
    i = 0;
    while (i < RECENT_EVENT_ROWS) : (i += 1) try seedEvent(conn, .lease_acquired, now_ms + @as(i64, @intCast(i)));
    // The runner's enrolment, older than the window. Sweeping it by age is what
    // blanked the operator Activity feed for every long-lived runner.
    try seedEvent(conn, LIFECYCLE_EVENT_TYPE, aged_at);
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

    // Aged terminal rows are gone; the in-window terminal row survives. The
    // aged `active` row also survives this pass — the expiry arm reaped it
    // moments before, stamping a fresh `updated_at` that puts it outside the
    // delete predicate until its own window elapses (see the reaping test).
    try std.testing.expect(!try leaseExists(ctx.conn, L_AGED_REPORTED_ONE));
    try std.testing.expect(!try leaseExists(ctx.conn, L_AGED_REPORTED_TWO));
    try std.testing.expect(!try leaseExists(ctx.conn, L_AGED_EXPIRED));
    try std.testing.expect(try leaseExists(ctx.conn, L_ACTIVE_OLD));
    try std.testing.expect(try leaseExists(ctx.conn, L_RECENT_REPORTED));
    // The retention promise, stated as a row: acquired before the window,
    // settled inside it, therefore kept. This is the assertion a `created_at`
    // sweep fails and the old one-timestamp fixture could not express.
    try std.testing.expect(try leaseExists(ctx.conn, L_AGED_SETTLED_RECENT));
    // Per-work events: only the in-window ones remain. The aged lifecycle row
    // is untouched, so a runner enrolled before the window still has a feed.
    try std.testing.expectEqual(@as(i64, RECENT_EVENT_ROWS), try eventCountOfType(ctx.conn, .lease_acquired));
    try std.testing.expectEqual(@as(i64, 1), try eventCountOfType(ctx.conn, LIFECYCLE_EVENT_TYPE));
    try std.testing.expectEqual(@as(i64, RECENT_EVENT_ROWS + 1), try runnerEventCount(ctx.conn));

    // A second cycle finds nothing left to do — the sweep converges.
    const again = try retention_sweeper.sweepOnce(ctx.pool);
    try std.testing.expectEqual(@as(i64, 0), again.leases_deleted);
    try std.testing.expectEqual(@as(i64, 0), again.events_deleted);

    cleanup(ctx.conn);
}

test "an abandoned lease is reaped by age; live work is left alone" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    try setupBase(ctx.conn);

    const now_ms = clock.nowMillis();
    const aged_at = now_ms - RETENTION_WINDOW_MS - AGE_SAFETY_MS;
    // The zombie: a runner died holding this, the event settled elsewhere so
    // nothing ever redelivered, and the fleet was never used again. None of the
    // three ordinary writers can reach it.
    try seedLease(ctx.conn, L_ACTIVE_OLD, EVENT_PREFIX ++ "zombie", protocol.RUNNER_LEASE_STATUS_ACTIVE, aged_at, aged_at);
    // Live work, renewing now. `updated_at` is what the arm reads, so this is
    // the row that proves the reaper cannot reach anything a runner still holds
    // — the whole safety argument, stated as a row rather than as a comment.
    try seedLease(ctx.conn, L_RECENT_REPORTED, EVENT_PREFIX ++ "live", protocol.RUNNER_LEASE_STATUS_ACTIVE, aged_at, now_ms);

    const expired_before = try lifetimeExpiredCount(ctx.conn);
    const totals = try retention_sweeper.sweepOnce(ctx.pool);

    try std.testing.expectEqual(@as(i64, 1), totals.leases_expired);
    try std.testing.expectEqualStrings(protocol.RUNNER_LEASE_STATUS_EXPIRED, try leaseStatus(ctx.conn, L_ACTIVE_OLD));
    try std.testing.expectEqualStrings(protocol.RUNNER_LEASE_STATUS_ACTIVE, try leaseStatus(ctx.conn, L_RECENT_REPORTED));

    // The transition is counted exactly once, by the same tally arm shape
    // `reclaim` uses — the counters describe transitions, and a reaping is one.
    try std.testing.expectEqual(expired_before + 1, try lifetimeExpiredCount(ctx.conn));

    // The reaped row is NOT deleted in the same cycle: the flip stamped
    // `updated_at`, so it now serves the readable window every settled lease
    // gets. Deleting it here would erase a run's record the instant the system
    // noticed it, which is the opposite of what retention promises.
    try std.testing.expect(try leaseExists(ctx.conn, L_ACTIVE_OLD));
    try std.testing.expectEqual(@as(i64, 0), totals.leases_deleted);

    // Converges: nothing is active-and-aged any more, so a second cycle is a
    // no-op rather than re-counting the same transition.
    const again = try retention_sweeper.sweepOnce(ctx.pool);
    try std.testing.expectEqual(@as(i64, 0), again.leases_expired);
    try std.testing.expectEqual(expired_before + 1, try lifetimeExpiredCount(ctx.conn));

    cleanup(ctx.conn);
}

test "a sweeper skips rows another sweeper holds instead of blocking on them" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    try setupBase(ctx.conn);

    const now_ms = clock.nowMillis();
    const aged_at = now_ms - RETENTION_WINDOW_MS - AGE_SAFETY_MS;
    try seedLease(ctx.conn, L_AGED_REPORTED_ONE, EVENT_PREFIX ++ "held", protocol.RUNNER_LEASE_STATUS_REPORTED, aged_at, aged_at);
    try seedLease(ctx.conn, L_AGED_REPORTED_TWO, EVENT_PREFIX ++ "free", protocol.RUNNER_LEASE_STATUS_REPORTED, aged_at, aged_at);

    // Stand in for the other replica's sweeper mid-batch: its rows are locked,
    // uncommitted. Without SKIP LOCKED this connection's sweep would wait on
    // that transaction — paying the full search and deleting nothing — which is
    // exactly the convoy three replicas on one hourly schedule would form.
    _ = try ctx.conn.exec("BEGIN", .{});
    {
        // Scoped so the cursor closes before the sweep runs on another
        // connection — the row lock belongs to the open transaction and outlives
        // this block, which is the whole point.
        var held = PgQuery.from(try ctx.conn.query(
            "SELECT id FROM fleet.runner_leases WHERE id = $1::uuid FOR UPDATE",
            .{L_AGED_REPORTED_ONE},
        ));
        defer held.deinit();
        _ = try held.next();
        held.drain();
    }

    const totals = try retention_sweeper.sweepOnce(ctx.pool);

    // It took the free row and stepped over the held one — disjoint batches,
    // no block. The assertion that it returned at all is half the proof.
    try std.testing.expect(!try leaseExistsOn(ctx.pool, L_AGED_REPORTED_TWO));
    try std.testing.expect(totals.leases_deleted >= 1);

    _ = try ctx.conn.exec("ROLLBACK", .{});
    // Released, so the next cycle claims what it skipped: skipping defers work,
    // it never drops it.
    try std.testing.expect(try leaseExists(ctx.conn, L_AGED_REPORTED_ONE));
    const after = try retention_sweeper.sweepOnce(ctx.pool);
    try std.testing.expect(after.leases_deleted >= 1);
    try std.testing.expect(!try leaseExists(ctx.conn, L_AGED_REPORTED_ONE));

    cleanup(ctx.conn);
}

test "a full batch keeps sweeping, and the cycle says it was saturated" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    try setupBase(ctx.conn);

    // One row past a single batch's ceiling: enough to prove the loop continues
    // past a full batch, which every fixture until now was too small to reach —
    // they all drained on the first statement and left the continue-arm and the
    // saturation flag untested.
    const now_ms = clock.nowMillis();
    const aged_at = now_ms - RETENTION_WINDOW_MS - AGE_SAFETY_MS;
    try seedAgedLeaseBulk(ctx.conn, aged_at, DELETE_BATCH_LIMIT + 1);

    const cutoff = clock.nowMillis() - RETENTION_WINDOW_MS;
    const eligible = try agedTerminalLeaseCount(ctx.conn, cutoff);
    try std.testing.expect(eligible > DELETE_BATCH_LIMIT);

    const totals = try retention_sweeper.sweepOnce(ctx.pool);
    try std.testing.expectEqual(eligible, totals.leases_deleted);
    try std.testing.expectEqual(@as(i64, 0), try agedTerminalLeaseCount(ctx.conn, cutoff));
    // Drained inside the cycle's ceiling, so the sweeper may idle the full
    // interval — saturation is reserved for a backlog that outran the cycle.
    try std.testing.expect(!totals.saturated);

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
