const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const protocol = @import("contract").protocol;
const queue_consts = @import("../queue/constants.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const event_rows = @import("event_rows.zig");
const reclaim_sweeper = @import("reclaim_sweeper.zig");
const base = @import("event_lifecycle_integration_test.zig");
const fixtures = @import("../db/test_fixtures.zig");
const assign = @import("assign.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

// The fault-injection constraint names live in `test_fixtures`
// (`fixtures.RECLAIM_FAIL_CONSTRAINT` / `fixtures.RELEASE_FAIL_CONSTRAINT`) so
// that every test-DB conn opener can drop a leaked one before *any* test
// touches the shared tables — see `dropInjectedFaultConstraints`. This file
// only arms/disarms them per test.

const AffinitySlot = struct { fencing_seq: i64, leased_until: i64 };

fn affinitySlot(conn: *pg.Conn, fleet_id: []const u8) !AffinitySlot {
    var q = PgQuery.from(try conn.query(
        "SELECT fencing_seq, leased_until FROM fleet.runner_affinity WHERE fleet_id = $1::uuid",
        .{fleet_id},
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.AffinityRowMissing;
    return .{ .fencing_seq = try row.get(i64, 0), .leased_until = try row.get(i64, 1) };
}

fn activeLeaseCount(conn: *pg.Conn, fleet_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT count(*) FROM fleet.runner_leases WHERE fleet_id = $1::uuid AND status = $2",
        .{ fleet_id, protocol.RUNNER_LEASE_STATUS_ACTIVE },
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.LeaseRowMissing;
    return row.get(i64, 0);
}

/// Reject the reclaim's `status = 'expired'` write for `fleet_id`, so
/// `reclaimPriorActive` errors *after* `affinity.claim` has already won the slot
/// — the precise ordering the release-on-error branch exists for. Injecting at
/// the Postgres layer (rather than through `select`'s allocator) keeps this
/// deterministic: the candidate scan and the claim both allocate before the
/// reclaim probe, so no stable `fail_index` isolates the reclaim's own dupes.
///
/// The CHECK is **scoped to `fleet_id`** — the test's own throwaway fleet, which
/// no other suite touches — so even if a killed run leaks the constraint (or a
/// raw-pool test like `schema_migration_test` bypasses the conn-opener cleanup),
/// it can only reject writes to that one fleet's rows. Other suites, which use
/// different fleet ids, are unaffected regardless of how they open the DB.
/// `NOT VALID` skips the scan of pre-existing rows.
fn armReclaimFailure(conn: *pg.Conn, fleet_id: []const u8) !void {
    // `setup()` (via the conn openers) cleared any leaked constraint before this
    // test ran, so the ADD starts from a clean slot; the per-test
    // `defer disarmReclaimFailure` drops it on the way out.
    var sql_buf: [256]u8 = undefined;
    const sql = try std.fmt.bufPrint(
        &sql_buf,
        "ALTER TABLE fleet.runner_leases ADD CONSTRAINT {s} CHECK (NOT (fleet_id = '{s}'::uuid AND status = '{s}')) NOT VALID",
        .{ fixtures.RECLAIM_FAIL_CONSTRAINT, fleet_id, protocol.RUNNER_LEASE_STATUS_EXPIRED },
    );
    _ = try conn.exec(sql, .{});
}

fn disarmReclaimFailure(conn: *pg.Conn) void {
    var sql_buf: [128]u8 = undefined;
    const sql = std.fmt.bufPrint(
        &sql_buf,
        "ALTER TABLE fleet.runner_leases DROP CONSTRAINT IF EXISTS {s}",
        .{fixtures.RECLAIM_FAIL_CONSTRAINT},
    ) catch return;
    _ = conn.exec(sql, .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

/// Make `affinity.release` itself fail for `fleet_id`, so releasing a won slot
/// degrades to the claim's own TTL expiry. The two writers of `runner_affinity`
/// separate by the *shape* of their write, not by the clock: `claim` sets
/// `leased_until = now + LEASE_TTL_MS` against `updated_at = now` (unequal),
/// while `release` sets both to the same `now` (equal). Rejecting an
/// equal-timestamps write on this fleet admits every claim and rejects the
/// release, with no wall-clock margin that could go stale under load.
///
/// Scoped to `fleet_id` for the same reason as `armReclaimFailure`: a leak can
/// only ever affect this test's throwaway fleet. Crucially, the `leased_until =
/// updated_at` shape is *also* what a plain zero-init insert produces, so a
/// leaked unscoped version rejected unrelated inserts (e.g. `schema_migration_
/// test` writing `(0, 0)`) — the fleet scope closes exactly that. `NOT VALID`
/// skips the existing row, which the caller has just parked at `leased_until = 0`.
fn armReleaseFailure(conn: *pg.Conn, fleet_id: []const u8) !void {
    var sql_buf: [256]u8 = undefined;
    const sql = try std.fmt.bufPrint(
        &sql_buf,
        "ALTER TABLE fleet.runner_affinity ADD CONSTRAINT {s} CHECK (NOT (fleet_id = '{s}'::uuid AND leased_until = updated_at)) NOT VALID",
        .{ fixtures.RELEASE_FAIL_CONSTRAINT, fleet_id },
    );
    _ = try conn.exec(sql, .{});
}

fn disarmReleaseFailure(conn: *pg.Conn) void {
    var sql_buf: [128]u8 = undefined;
    const sql = std.fmt.bufPrint(
        &sql_buf,
        "ALTER TABLE fleet.runner_affinity DROP CONSTRAINT IF EXISTS {s}",
        .{fixtures.RELEASE_FAIL_CONSTRAINT},
    ) catch return;
    _ = conn.exec(sql, .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

test "approval deadline expiry writes the terminal row: gate_blocked + approval_expired + XACK" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.AGENTSFLEET_GATED_EXP, "lifecycle-gatex", base.CONFIG_GATED_FAST, "8");

    const event_id = try base.publishEvent(h, base.AGENTSFLEET_GATED_EXP);
    defer h.queue.alloc.free(event_id);

    try std.testing.expect(!try base.pollLease(h));
    try base.expectRow(conn, base.AGENTSFLEET_GATED_EXP, event_id, event_rows.STATUS_RECEIVED, "");
    try std.testing.expectEqual(@as(i64, 1), try base.pendingCount(h, base.AGENTSFLEET_GATED_EXP));

    @import("common").sleepNanos(5 * std.time.ns_per_ms);
    try std.testing.expect(!try base.pollLease(h));
    try base.expectRow(conn, base.AGENTSFLEET_GATED_EXP, event_id, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_APPROVAL_EXPIRED);
    try std.testing.expectEqual(@as(i64, 0), try base.pendingCount(h, base.AGENTSFLEET_GATED_EXP));
}

test "markBlocked is guarded: terminal rows never reopen, second transition affects zero rows" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.AGENTSFLEET_ROW, "lifecycle-row", base.CONFIG_PLAIN, "6");
    const EVENT_ID = "1700000000000-7";
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (uid, fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, created_at, updated_at)
        \\VALUES ('0195c9da-1e2a-7f13-8abc-2b3e1e0d7e01'::uuid, $1::uuid, $2, $3::uuid,
        \\        'steer:test', 'chat', $4, '{}'::jsonb, 0, 0)
        \\ON CONFLICT (fleet_id, event_id) DO UPDATE SET status = EXCLUDED.status, failure_label = NULL
    , .{ base.AGENTSFLEET_ROW, EVENT_ID, base.WORKSPACE_ID, event_rows.STATUS_RECEIVED });

    try std.testing.expectEqual(@as(i64, 1), try event_rows.markBlocked(h.pool, base.AGENTSFLEET_ROW, EVENT_ID, event_rows.LABEL_BALANCE_EXHAUSTED));
    try base.expectRow(conn, base.AGENTSFLEET_ROW, EVENT_ID, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_BALANCE_EXHAUSTED);
    try std.testing.expectEqual(@as(i64, 0), try event_rows.markBlocked(h.pool, base.AGENTSFLEET_ROW, EVENT_ID, event_rows.LABEL_APPROVAL_DENIED));
    try base.expectRow(conn, base.AGENTSFLEET_ROW, EVENT_ID, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_BALANCE_EXHAUSTED);
}

test "terminal entry re-delivered from the PEL is re-acked, never re-executed" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.AGENTSFLEET_REACK, "lifecycle-reack", base.CONFIG_PLAIN, "7");

    const event_id = try base.publishEvent(h, base.AGENTSFLEET_REACK);
    defer h.queue.alloc.free(event_id);
    try std.testing.expect(try base.pollLease(h));
    try std.testing.expectEqual(@as(i64, 1), try base.pendingCount(h, base.AGENTSFLEET_REACK));

    _ = try conn.exec(
        "UPDATE core.fleet_events SET status = $3 WHERE fleet_id = $1::uuid AND event_id = $2",
        .{ base.AGENTSFLEET_REACK, event_id, event_rows.STATUS_PROCESSED },
    );
    _ = try conn.exec("DELETE FROM fleet.runner_leases WHERE workspace_id = $1::uuid", .{base.WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{base.AGENTSFLEET_REACK});

    try std.testing.expect(!try base.pollLease(h));
    try std.testing.expectEqual(@as(i64, 0), try base.pendingCount(h, base.AGENTSFLEET_REACK));
    try base.expectRow(conn, base.AGENTSFLEET_REACK, event_id, event_rows.STATUS_PROCESSED, "");
}

test "consumer identity is stable: repeated idle probes leave one consumer in the group" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.FLEET_IDLE, "lifecycle-idle", base.CONFIG_PLAIN, "4");
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, base.FLEET_IDLE);

    var i: usize = 0;
    while (i < 25) : (i += 1) _ = try base.pollLease(h);
    try std.testing.expectEqual(@as(usize, 1), try base.consumerCount(h, base.FLEET_IDLE));
}

test "reclaim sweep recovers a stranded delivery from a dead consumer and re-leases it" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.AGENTSFLEET_STRAND, "lifecycle-strand", base.CONFIG_PLAIN, "5");

    const event_id = try base.publishEvent(h, base.AGENTSFLEET_STRAND);
    defer h.queue.alloc.free(event_id);
    try base.deliverToDeadConsumer(h, base.AGENTSFLEET_STRAND);
    try base.forceIdle(h, base.AGENTSFLEET_STRAND, event_id, base.FORCED_IDLE_MS);

    const stats = try reclaim_sweeper.sweepOnce(h.pool, &h.queue, std.testing.allocator);
    try std.testing.expect(stats.reclaimed_entries >= 1);
    try std.testing.expect(try base.pollLease(h));
    try base.expectRow(conn, base.AGENTSFLEET_STRAND, event_id, event_rows.STATUS_RECEIVED, "");
}

test "reclaim sweep never touches an entry inside the lease window" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.AGENTSFLEET_STRAND, "lifecycle-strand", base.CONFIG_PLAIN, "5");

    const event_id = try base.publishEvent(h, base.AGENTSFLEET_STRAND);
    defer h.queue.alloc.free(event_id);
    try base.deliverToDeadConsumer(h, base.AGENTSFLEET_STRAND);

    const stats = try reclaim_sweeper.sweepOnce(h.pool, &h.queue, std.testing.allocator);
    try std.testing.expectEqual(@as(i64, 0), stats.reclaimed_entries);
    try std.testing.expectEqual(@as(i64, 1), try base.pendingCount(h, base.AGENTSFLEET_STRAND));
}

test "a reclaim-stage failure releases the won slot instead of stalling the fleet for the lease TTL" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.AGENTSFLEET_RECLAIM_FAIL, "lifecycle-reclaim-fail", base.CONFIG_PLAIN, "9");

    // A real prior holder, issued through the production lease path.
    const event_id = try base.publishEvent(h, base.AGENTSFLEET_RECLAIM_FAIL);
    defer h.queue.alloc.free(event_id);
    try std.testing.expect(try base.pollLease(h));

    // That holder dies: its affinity claim expires while its lease row stays
    // `active` — exactly the state a reclaim is meant to recover.
    _ = try conn.exec("UPDATE fleet.runner_affinity SET leased_until = 0 WHERE fleet_id = $1::uuid", .{base.AGENTSFLEET_RECLAIM_FAIL});

    try armReclaimFailure(conn, base.AGENTSFLEET_RECLAIM_FAIL);
    {
        defer disarmReclaimFailure(conn);
        // This poll wins the slot (fencing_seq 1 -> 2), then the reclaim write
        // trips the constraint. The claim must not survive the error.
        try std.testing.expect(!try base.pollLease(h));

        const slot = try affinitySlot(conn, base.AGENTSFLEET_RECLAIM_FAIL);
        try std.testing.expectEqual(@as(i64, 2), slot.fencing_seq);
        // Pre-fix this held `claim_ts + LEASE_TTL_MS`, ~30s in the future.
        try std.testing.expect(slot.leased_until <= clock.nowMillis());
        // The failed reclaim rolled back: the event is still leasable, not lost.
        try std.testing.expectEqual(@as(i64, 1), try activeLeaseCount(conn, base.AGENTSFLEET_RECLAIM_FAIL));
    }

    // The released slot is claimable at once: the next poll reclaims the same
    // event rather than waiting out the dead claim's TTL.
    try std.testing.expect(try base.pollLease(h));
}

test "a fresh-acquisition envelope allocation failure releases the won slot" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.AGENTSFLEET_FRESH_FAIL, "lifecycle-fresh-fail", base.CONFIG_PLAIN, "a");

    const event_id = try base.publishEvent(h, base.AGENTSFLEET_FRESH_FAIL);
    defer h.queue.alloc.free(event_id);

    // Derive — don't guess — the failing allocation index. `fromFresh`'s dupes
    // are the LAST allocations `select` makes, so failing the final one lands
    // inside `fromFresh` no matter how many fields it dupes. An arena backs the
    // FailingAllocator because production hands `select` a request arena; this
    // is an error-path test, not a leak-freedom test.
    var probe_arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer probe_arena.deinit();
    var probe = std.testing.FailingAllocator.init(probe_arena.allocator(), .{ .fail_index = std.math.maxInt(usize) });
    const acquired = assign.select(&h.ctx, probe.allocator(), base.RUNNER_ID) orelse return error.ProbeAcquireFailed;
    try std.testing.expectEqual(assign.Kind.fresh, acquired.kind);
    // Guards the `- 1` below: if the envelope ever stops allocating, say so here
    // rather than trapping on a usize underflow.
    try std.testing.expect(probe.alloc_index > 0);

    // Free the slot the probe won. Its event now sits in the consumer's PEL, so
    // the replay re-enters via the own-PEL-first branch and reaches the same
    // `fromFresh` dupes — an identical allocation count on this allocator.
    _ = try conn.exec("UPDATE fleet.runner_affinity SET leased_until = 0 WHERE fleet_id = $1::uuid", .{base.AGENTSFLEET_FRESH_FAIL});

    var fail_arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer fail_arena.deinit();
    var failing = std.testing.FailingAllocator.init(fail_arena.allocator(), .{ .fail_index = probe.alloc_index - 1 });
    try std.testing.expect(assign.select(&h.ctx, failing.allocator(), base.RUNNER_ID) == null);

    const slot = try affinitySlot(conn, base.AGENTSFLEET_FRESH_FAIL);
    try std.testing.expectEqual(@as(i64, 2), slot.fencing_seq);
    // Pre-fix this exit held `claim_ts + LEASE_TTL_MS`, stalling the fleet ~30s.
    try std.testing.expect(slot.leased_until <= clock.nowMillis());
}

test "a failed release degrades to TTL expiry and never masks the original reclaim error" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, base.AGENTSFLEET_RELEASE_FAIL, "lifecycle-release-fail", base.CONFIG_PLAIN, "b");

    const event_id = try base.publishEvent(h, base.AGENTSFLEET_RELEASE_FAIL);
    defer h.queue.alloc.free(event_id);
    try std.testing.expect(try base.pollLease(h));
    _ = try conn.exec("UPDATE fleet.runner_affinity SET leased_until = 0 WHERE fleet_id = $1::uuid", .{base.AGENTSFLEET_RELEASE_FAIL});

    // Both the reclaim probe AND the compensating release now fail — scoped to
    // this test's own fleet so a leak cannot reach any other suite.
    try armReclaimFailure(conn, base.AGENTSFLEET_RELEASE_FAIL);
    defer disarmReclaimFailure(conn);
    try armReleaseFailure(conn, base.AGENTSFLEET_RELEASE_FAIL);
    defer disarmReleaseFailure(conn);

    // The release error is swallowed and reported (`released=false`), the reclaim
    // error still propagates: `select` collapses to null, no lease, no panic.
    try std.testing.expect(!try base.pollLease(h));

    const slot = try affinitySlot(conn, base.AGENTSFLEET_RELEASE_FAIL);
    try std.testing.expectEqual(@as(i64, 2), slot.fencing_seq);
    // Documented fallback: the slot stays leased and expires on its own TTL —
    // the pre-fix behaviour, never worse.
    try std.testing.expect(slot.leased_until > clock.nowMillis());
    // And the event is still leasable once that TTL lapses.
    try std.testing.expectEqual(@as(i64, 1), try activeLeaseCount(conn, base.AGENTSFLEET_RELEASE_FAIL));
}

test "reclaim min-idle exceeds the lease window" {
    try std.testing.expect(queue_consts.fleet_xautoclaim_min_idle_ms_int > @import("common").LEASE_TTL_MS);
}
