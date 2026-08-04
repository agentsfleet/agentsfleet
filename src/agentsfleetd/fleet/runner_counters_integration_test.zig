// Lifetime tally maintenance for `fleet.runner_lifetime_counters`. The table
// has no writer of its own — each of the three lease write paths bumps its
// tally in the SAME statement as the transition it counts (acquire in
// `sql_lease_row.INSERT_LEASE_WITH_EVENT`, settle in `renewal_settle`'s claim
// CTE, expire in `reclaim.reclaimPriorActive`), so a counter can never drift
// from the rows it counts: not under concurrency, not under retries. These
// tests drive the REAL statements against the live schema and hold the counter
// row against a recount of the lease rows. Requires TEST_DATABASE_URL;
// self-skips otherwise.

const std = @import("std");
const constants = @import("common");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const base = @import("../db/test_fixtures.zig");
const protocol = @import("contract").protocol;
const assign = @import("assign.zig");
const sql = @import("sql.zig");
const renewal_settle = @import("renewal_settle.zig");
const reclaim = @import("reclaim.zig");
const runner_events = @import("runner_events.zig");
const event_rows = @import("event_rows.zig");
const id_format = @import("../types/id_format.zig");

const ALLOC = std.testing.allocator;

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-3c0e1e0c0011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-3c0e1e0c0a01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-3c0e1e0c0c01";
const LEASE_POOL = [_][]const u8{
    "0195b4ba-8d3a-7f13-8abc-3c0e1e0c0f01",
    "0195b4ba-8d3a-7f13-8abc-3c0e1e0c0f02",
    "0195b4ba-8d3a-7f13-8abc-3c0e1e0c0f03",
    "0195b4ba-8d3a-7f13-8abc-3c0e1e0c0f04",
    "0195b4ba-8d3a-7f13-8abc-3c0e1e0c0f05",
    "0195b4ba-8d3a-7f13-8abc-3c0e1e0c0f06",
};

const NOW_MS: i64 = 1_900_000_000_000;
const LEASE_AHEAD_MS: i64 = 60_000;
const EVENT_PREFIX = "evt-cnt-";
const ACTOR = "steer:counters-test";
const EVENT_TYPE_CHAT = "chat";
const TEST_POSTURE = "platform";
const TEST_PROVIDER = "test-provider";
const TEST_MODEL = "test-model";

const N_WORKERS = 8;
const CYCLES_PER_WORKER = 3;
const ACQUIRE_ATTEMPTS = 30;
const ACQUIRE_BACKOFF_NS: u64 = 20 * std.time.ns_per_ms;

fn seedRunner(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners (id, host_id, token_hash, sandbox_tier, admin_state,
        \\   labels, tenant_id, last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'counters-host', 'counters-hash', 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{RUNNER_ID});
}

// Fence holds for every lease below (token 1 == seq 1), so each settle's guard
// passes and the tally arm is reached.
fn seedAffinity(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity (fleet_id, last_runner_id, fencing_seq,
        \\   leased_until, metered_input_tokens, metered_cached_tokens, metered_output_tokens,
        \\   last_metered_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 1, $3, 0, 0, 0, $4, 0, 0)
        \\ON CONFLICT (fleet_id) DO UPDATE SET fencing_seq = 1,
        \\   leased_until = EXCLUDED.leased_until, last_metered_at = EXCLUDED.last_metered_at
    , .{ FLEET_ID, RUNNER_ID, NOW_MS + LEASE_AHEAD_MS, NOW_MS });
}

fn setupBase(conn: *pg.Conn) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "counters-fleet", "{}", "# z");
    try seedRunner(conn);
    try seedAffinity(conn);
}

fn execIgnore(conn: *pg.Conn, sql_text: []const u8, args: anytype) void {
    _ = conn.exec(sql_text, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn cleanup(conn: *pg.Conn) void {
    // Settles write audit rows keyed by event id; clear them so a crashed run
    // cannot pollute a sibling suite's count-based assertions.
    execIgnore(conn, "DELETE FROM billing.usage_ledger WHERE event_id LIKE $1", .{EVENT_PREFIX ++ "%"});
    execIgnore(conn, "DELETE FROM core.fleet_events WHERE event_id LIKE $1", .{EVENT_PREFIX ++ "%"});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE runner_id = $1::uuid", .{RUNNER_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{FLEET_ID});
    // Cascades the audit events and the counter row with the runner.
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    base.teardownFleets(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

/// Drive the production acquire statement — the lease insert, its audit event,
/// and the acquired tally land in ONE statement, bound exactly as
/// `service_lease_row.insertLeaseRow` binds it.
fn acquireLease(conn: *pg.Conn, lease_id: []const u8, event_id: []const u8, fencing_token: i64) !void {
    // The event row the lease names. `INSERT_LEASE_WITH_EVENT` writes the lease,
    // its runner-audit row and the acquired tally — but NOT `core.fleet_events`,
    // and `reclaim.reclaimPriorActive` reads the body through an INNER JOIN on
    // `(fleet_id, event_id)`. Without this the expire arm reclaims nothing and
    // the tally under test never moves. Removed by `cleanup`'s LIKE sweep.
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3::uuid, $4, $5, $6, '{}'::jsonb, $7, $7)
        \\ON CONFLICT (fleet_id, event_id) DO NOTHING
    , .{ FLEET_ID, event_id, WORKSPACE_ID, ACTOR, EVENT_TYPE_CHAT, event_rows.STATUS_RECEIVED, NOW_MS });

    const audit_uid = try id_format.generateUuidV7();
    const audit_id: []const u8 = &audit_uid;
    _ = try conn.exec(sql.INSERT_LEASE_WITH_EVENT, .{
        lease_id,
        RUNNER_ID,
        FLEET_ID,
        WORKSPACE_ID,
        base.TEST_TENANT_ID,
        event_id,
        ACTOR,
        EVENT_TYPE_CHAT,
        NOW_MS,
        TEST_POSTURE,
        TEST_PROVIDER,
        TEST_MODEL,
        fencing_token,
        NOW_MS + LEASE_AHEAD_MS,
        protocol.RUNNER_LEASE_STATUS_ACTIVE,
        NOW_MS,
        audit_id,
        @tagName(protocol.RunnerEventType.lease_acquired),
        runner_events.META_LEASE_ID,
        runner_events.META_FLEET_ID,
        runner_events.META_AGENTSFLEET_EVENT_ID,
        runner_events.META_KIND,
        @tagName(assign.Kind.fresh),
    });
}

/// Settle through the production claim CTE. Zero meter/rates: the money legs
/// charge nothing, so only the claim and its tally arm are load-bearing here.
fn settleLease(conn: *pg.Conn, lease_id: []const u8, succeeded: bool) !renewal_settle.SettleOutcome {
    return renewal_settle.claimAndSettle(conn, lease_id, RUNNER_ID, NOW_MS, .{}, succeeded);
}

/// Expire the fleet's latest active lease through the production reclaim
/// statement, freeing the returned envelope immediately (only the counter side
/// effect matters here).
fn expireLatestActive(conn: *pg.Conn) !void {
    const prior = (try reclaim.reclaimPriorActive(conn, ALLOC, FLEET_ID)) orelse
        return error.NoActiveLeaseToReclaim;
    prior.deinit(ALLOC);
}

const Counters = struct { acquired: i64, succeeded: i64, failed: i64, expired: i64 };

fn expectSameCounters(want: Counters, got: Counters) !void {
    try std.testing.expectEqual(want.acquired, got.acquired);
    try std.testing.expectEqual(want.succeeded, got.succeeded);
    try std.testing.expectEqual(want.failed, got.failed);
    try std.testing.expectEqual(want.expired, got.expired);
}

/// Counter rows for this runner. One, always — `runner_id` IS the primary key,
/// so a second row would mean the single-key rewrite (C4) had been undone.
fn counterRowCount(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::bigint FROM fleet.runner_lifetime_counters WHERE runner_id = $1::uuid",
        .{RUNNER_ID},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return 0;
    return row.get(i64, 0);
}

fn counterRow(conn: *pg.Conn) !Counters {
    var q = PgQuery.from(try conn.query(
        \\SELECT acquired, succeeded, failed, expired
        \\FROM fleet.runner_lifetime_counters WHERE runner_id = $1::uuid
    , .{RUNNER_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.CounterRowMissing;
    return .{
        .acquired = try row.get(i64, 0),
        .succeeded = try row.get(i64, 1),
        .failed = try row.get(i64, 2),
        .expired = try row.get(i64, 3),
    };
}

/// Recount of the runner's surviving lease rows; null status counts them all.
fn leaseCount(conn: *pg.Conn, status: ?[]const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT COUNT(*)::bigint FROM fleet.runner_leases
        \\WHERE runner_id = $1::uuid AND ($2::text IS NULL OR status = $2)
    , .{ RUNNER_ID, status }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.LeaseCountMissing;
    return row.get(i64, 0);
}

/// The counter row against a recount of the lease rows per class. succeeded
/// and failed split one recount class (reported) by the settle verdicts, which
/// the caller supplies.
fn expectCountersMatchRecount(conn: *pg.Conn, want_succeeded: i64, want_failed: i64) !void {
    const got = try counterRow(conn);
    try std.testing.expectEqual(try leaseCount(conn, null), got.acquired);
    try std.testing.expectEqual(try leaseCount(conn, protocol.RUNNER_LEASE_STATUS_EXPIRED), got.expired);
    try std.testing.expectEqual(try leaseCount(conn, protocol.RUNNER_LEASE_STATUS_REPORTED), got.succeeded + got.failed);
    try std.testing.expectEqual(want_succeeded, got.succeeded);
    try std.testing.expectEqual(want_failed, got.failed);
}

test "counter row equals a recount of the lease rows after mixed transitions" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    try setupBase(ctx.conn);

    // Six acquires through the real insert; tokens ascend so the reclaims below
    // deterministically pick the two newest active holders.
    for (LEASE_POOL, 0..) |lease_id, i| {
        var event_buf: [64]u8 = undefined;
        const event_id = try std.fmt.bufPrint(&event_buf, EVENT_PREFIX ++ "mix-{d}", .{i});
        try acquireLease(ctx.conn, lease_id, event_id, @as(i64, @intCast(i + 1)));
    }
    // Two succeed, one fails, two expire via reclaim; one stays active.
    try std.testing.expect((try settleLease(ctx.conn, LEASE_POOL[0], true)).claimed);
    try std.testing.expect((try settleLease(ctx.conn, LEASE_POOL[1], true)).claimed);
    try std.testing.expect((try settleLease(ctx.conn, LEASE_POOL[2], false)).claimed);
    try expireLatestActive(ctx.conn);
    try expireLatestActive(ctx.conn);

    try expectCountersMatchRecount(ctx.conn, 2, 1);
    try std.testing.expectEqual(@as(i64, 1), try leaseCount(ctx.conn, protocol.RUNNER_LEASE_STATUS_ACTIVE));

    cleanup(ctx.conn);
}

fn acquireRetry(pool: *pg.Pool) ?*pg.Conn {
    // Workers outnumber the pool; treat an acquire timeout as "pool busy" and
    // retry, bounded so a dead pool still fails instead of hanging.
    var attempt: usize = 0;
    while (attempt < ACQUIRE_ATTEMPTS) : (attempt += 1) {
        return pool.acquire() catch {
            constants.sleepNanos(ACQUIRE_BACKOFF_NS);
            continue;
        };
    }
    return null;
}

// One worker's acquire+settle cycles on its own leases and pooled connection.
// The verdict parity (index + cycle) makes the suite-wide succeeded/failed
// split deterministic. Joins are the only synchronization.
const CycleWorker = struct {
    pool: *pg.Pool,
    index: usize,
    err: ?anyerror = null,

    fn run(self: *CycleWorker) void {
        self.runInner() catch |err| {
            self.err = err;
        };
    }

    fn runInner(self: *CycleWorker) !void {
        const conn = acquireRetry(self.pool) orelse return error.PoolExhausted;
        defer self.pool.release(conn);
        var cycle: usize = 0;
        while (cycle < CYCLES_PER_WORKER) : (cycle += 1) {
            const lease_uuid = try id_format.generateUuidV7();
            const lease_id: []const u8 = &lease_uuid;
            var event_buf: [64]u8 = undefined;
            const event_id = try std.fmt.bufPrint(&event_buf, EVENT_PREFIX ++ "conc-{d}-{d}", .{ self.index, cycle });
            try acquireLease(conn, lease_id, event_id, 1);
            const out = try settleLease(conn, lease_id, (self.index + cycle) % 2 == 0);
            if (!out.claimed) return error.SettleNotClaimed;
        }
    }
};

test "counter row equals a recount after concurrent acquire and settle cycles" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    try setupBase(ctx.conn);

    // One serial acquire+settle materializes the counter row before the race,
    // so what this test pins is the SUSTAINED-increment invariant: every racing
    // write takes the `ON CONFLICT (id)` update arm and none is lost.
    //
    // The other half — concurrent FIRST touch of a runner with no counter row —
    // is pinned separately by `concurrent first touches of a new runner's
    // counter row all land`, because it exercises a different failure. Slot 43
    // originally carried two unique keys over the same value (a generated
    // identity column plus a `runner_id` UNIQUE) and `ON CONFLICT` arbitrates
    // exactly one, so first-touch racers died on the other index instead of
    // updating. The rebuilt table has ONE unique key — `runner_id` is the plain
    // primary key, with no twin to tie it to — and that test is what would fail if
    // a second one were ever reintroduced (spec Discovery C4).
    try acquireLease(ctx.conn, LEASE_POOL[0], EVENT_PREFIX ++ "conc-seed", 1);
    try std.testing.expect((try settleLease(ctx.conn, LEASE_POOL[0], true)).claimed);

    var workers: [N_WORKERS]CycleWorker = undefined;
    var threads: [N_WORKERS]std.Thread = undefined;
    for (&workers, &threads, 0..) |*worker, *thread, i| {
        worker.* = .{ .pool = ctx.pool, .index = i };
        thread.* = try std.Thread.spawn(.{}, CycleWorker.run, .{worker});
    }
    for (&threads) |*thread| thread.join();
    for (&workers, 0..) |worker, i| {
        if (worker.err) |err| {
            std.debug.print("cycle worker {d} failed: {s}\n", .{ i, @errorName(err) });
            return error.CycleWorkerFailed;
        }
    }

    // Every cycle settled, so the recount classes are exact: all rows reported,
    // the verdict parity splitting the raced cycles evenly on top of the one
    // serial seed settle.
    const raced: i64 = N_WORKERS * CYCLES_PER_WORKER;
    try expectCountersMatchRecount(ctx.conn, @divExact(raced, 2) + 1, @divExact(raced, 2));
    try std.testing.expectEqual(raced + 1, try leaseCount(ctx.conn, protocol.RUNNER_LEASE_STATUS_REPORTED));

    cleanup(ctx.conn);
}

test "concurrent first touches of a new runner's counter row all land" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    try setupBase(ctx.conn);

    // Deliberately NO serial seed: every worker races to create the counter row
    // itself. This is the half its sibling test cannot reach — with the row
    // already present, every write takes the update arm and a table with two
    // unique keys over the same value would never be caught. Here the losers of
    // the insert race are the whole point: with a second unique key they die on
    // the index `ON CONFLICT` is not arbitrating, which was a live 500 under
    // concurrent acquire (spec Discovery C4).
    try std.testing.expectError(error.CounterRowMissing, counterRow(ctx.conn));

    var workers: [N_WORKERS]CycleWorker = undefined;
    var threads: [N_WORKERS]std.Thread = undefined;
    for (&workers, &threads, 0..) |*worker, *thread, i| {
        worker.* = .{ .pool = ctx.pool, .index = i };
        thread.* = try std.Thread.spawn(.{}, CycleWorker.run, .{worker});
    }
    for (&threads) |*thread| thread.join();
    for (&workers, 0..) |worker, i| {
        if (worker.err) |err| {
            std.debug.print("first-touch worker {d} failed: {s}\n", .{ i, @errorName(err) });
            return error.CycleWorkerFailed;
        }
    }

    // One row, and every racing acquire counted in it: the insert race resolves
    // to a single winner and N-1 updates, never to a lost tally or an error.
    const raced: i64 = N_WORKERS * CYCLES_PER_WORKER;
    try expectCountersMatchRecount(ctx.conn, @divExact(raced, 2), @divExact(raced, 2));
    try std.testing.expectEqual(@as(i64, 1), try counterRowCount(ctx.conn));

    cleanup(ctx.conn);
}

test "retried settle increments the tally exactly once" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    try setupBase(ctx.conn);

    try acquireLease(ctx.conn, LEASE_POOL[0], EVENT_PREFIX ++ "retry-0", 1);
    // First settle claims the active→reported flip and its tally; the retry
    // finds no active row, so the claim CTE is empty and the tally arm —
    // which selects FROM claim — writes nothing.
    try std.testing.expect((try settleLease(ctx.conn, LEASE_POOL[0], true)).claimed);
    try std.testing.expect(!(try settleLease(ctx.conn, LEASE_POOL[0], true)).claimed);

    const got = try counterRow(ctx.conn);
    try std.testing.expectEqual(@as(i64, 1), got.acquired);
    try std.testing.expectEqual(@as(i64, 1), got.succeeded);
    try std.testing.expectEqual(@as(i64, 0), got.failed);
    try std.testing.expectEqual(@as(i64, 0), got.expired);

    cleanup(ctx.conn);
}

// Dimension 7.4 — the expired tally rides the SAME statement as the status flip.
//
// Every other reclaim here has an event row still present, so the outer SELECT
// returns something and a tally written by a SECOND statement would look
// identical. This drives the one case that tells the two apart. The body join
// is INNER, so deleting the event row leaves the outer SELECT with nothing to
// return — but a data-modifying Common Table Expression (CTE) runs to
// completion whether or not the primary query reads its output, so the flip and
// the tally must still commit while reclaim reports "no prior lease".
//
// Split across two statements, this is exactly where the counter drifts: the
// caller sees null, returns early, and the increment never runs while the lease
// already sits `expired`. The counter would then under-count expiries forever,
// silently, and only on the arm nobody exercises by hand.
test "reclaim tallies the expiry even when the event body is gone and it returns nothing" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    try setupBase(ctx.conn);

    const orphan_event = EVENT_PREFIX ++ "orphaned-body";
    try acquireLease(ctx.conn, LEASE_POOL[0], orphan_event, 1);
    const before = try counterRow(ctx.conn);

    // The lease outlives its event row — an ordinary delete, not a shortcut
    // that seeds an otherwise unreachable state.
    _ = try ctx.conn.exec("DELETE FROM core.fleet_events WHERE event_id = $1", .{orphan_event});

    const prior = try reclaim.reclaimPriorActive(ctx.conn, ALLOC, FLEET_ID);
    try std.testing.expect(prior == null);

    // The flip committed...
    try std.testing.expectEqual(@as(i64, 0), try leaseCount(ctx.conn, protocol.RUNNER_LEASE_STATUS_ACTIVE));
    try std.testing.expectEqual(@as(i64, 1), try leaseCount(ctx.conn, protocol.RUNNER_LEASE_STATUS_EXPIRED));
    // ...and so did the tally that counts it, in the same breath.
    const after = try counterRow(ctx.conn);
    try std.testing.expectEqual(before.expired + 1, after.expired);

    cleanup(ctx.conn);
}
