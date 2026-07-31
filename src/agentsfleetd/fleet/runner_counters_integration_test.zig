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
const schema = @import("schema");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const base = @import("../db/test_fixtures.zig");
const protocol = @import("contract").protocol;
const assign = @import("assign.zig");
const sql = @import("sql.zig");
const renewal_settle = @import("renewal_settle.zig");
const reclaim = @import("reclaim.zig");
const runner_events = @import("runner_events.zig");
const id_format = @import("../types/id_format.zig");

const ALLOC = std.testing.allocator;

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-3c0e1e0c0011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-3c0e1e0c0a01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-3c0e1e0c0c01";
const AFFINITY_ID = "0195b4ba-8d3a-7f13-8abc-3c0e1e0c0e01";
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
const REQUEST_JSON = "{}";
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
        \\INSERT INTO fleet.runner_affinity (id, fleet_id, last_runner_id, fencing_seq,
        \\   leased_until, metered_input_tokens, metered_cached_tokens, metered_output_tokens,
        \\   last_metered_at_ms, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 1, $4, 0, 0, 0, $5, 0, 0)
        \\ON CONFLICT (fleet_id) DO UPDATE SET fencing_seq = 1,
        \\   leased_until = EXCLUDED.leased_until, last_metered_at_ms = EXCLUDED.last_metered_at_ms
    , .{ AFFINITY_ID, FLEET_ID, RUNNER_ID, NOW_MS + LEASE_AHEAD_MS, NOW_MS });
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
    execIgnore(conn, "DELETE FROM fleet.metering_periods WHERE event_id LIKE $1", .{EVENT_PREFIX ++ "%"});
    execIgnore(conn, "DELETE FROM core.fleet_execution_telemetry WHERE event_id LIKE $1", .{EVENT_PREFIX ++ "%"});
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
        REQUEST_JSON,
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

/// The slot carrying the counter table and its backfill.
const COUNTER_SLOT_VERSION: i32 = 43;
const BACKFILL_MARKER = "INSERT INTO fleet.runner_lifetime_counters";

/// The backfill statement sliced out of the REAL migration text, so this test
/// can never drift from what a deploy runs. Slot 43 is `CREATE TABLE …;
/// GRANT …; INSERT … ON CONFLICT DO UPDATE;` — everything from the INSERT on
/// is the backfill, and it is the only part that has to survive a reapply.
fn backfillSql() ![]const u8 {
    for (schema.migrations) |m| {
        if (m.version != COUNTER_SLOT_VERSION) continue;
        const start = std.mem.indexOf(u8, m.sql, BACKFILL_MARKER) orelse return error.BackfillStatementMissing;
        return m.sql[start..];
    }
    return error.BackfillSlotMissing;
}

/// The Fleet event a settled lease delivered, in its terminal state.
///
/// Load-bearing for the backfill and NOT for the write arms, which is the
/// asymmetry worth knowing: the runtime tally reads the settle verdict handed
/// to `claimAndSettle`, while the backfill re-derives it from
/// `core.fleet_events.status` — a table a different write path owns. A settled
/// lease whose event row is missing or still non-terminal therefore backfills
/// as acquired-but-unclassified. Seeded here so the reconstruction is asked
/// against the state a real upgraded database is in.
fn seedTerminalFleetEvent(conn: *pg.Conn, event_id: []const u8, outcome: protocol.Outcome) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (uid, fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, created_at, updated_at)
        \\VALUES (overlay(md5($1 || $2)::uuid::text placing '7' from 15 for 1)::uuid,
        \\        $1::uuid, $2, $3::uuid, $4, $5, $6, '{}'::jsonb, $7, $7)
        \\ON CONFLICT (fleet_id, event_id) DO UPDATE SET status = EXCLUDED.status
    , .{ FLEET_ID, event_id, WORKSPACE_ID, ACTOR, EVENT_TYPE_CHAT, @tagName(outcome), NOW_MS });
}

const Counters = struct { acquired: i64, succeeded: i64, failed: i64, expired: i64 };

fn expectSameCounters(want: Counters, got: Counters) !void {
    try std.testing.expectEqual(want.acquired, got.acquired);
    try std.testing.expectEqual(want.succeeded, got.succeeded);
    try std.testing.expectEqual(want.failed, got.failed);
    try std.testing.expectEqual(want.expired, got.expired);
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

test "the migration backfill reconstructs the tallies and is idempotent on reapply" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);
    cleanup(ctx.conn);
    try setupBase(ctx.conn);

    // History first, written by the production paths — so the backfill is held
    // against the write-time arms rather than against a hand-computed number.
    for (LEASE_POOL, 0..) |lease_id, i| {
        var event_buf: [64]u8 = undefined;
        const event_id = try std.fmt.bufPrint(&event_buf, EVENT_PREFIX ++ "back-{d}", .{i});
        try acquireLease(ctx.conn, lease_id, event_id, @as(i64, @intCast(i + 1)));
    }
    try std.testing.expect((try settleLease(ctx.conn, LEASE_POOL[0], true)).claimed);
    try std.testing.expect((try settleLease(ctx.conn, LEASE_POOL[1], true)).claimed);
    try std.testing.expect((try settleLease(ctx.conn, LEASE_POOL[2], false)).claimed);
    try expireLatestActive(ctx.conn);
    // The event rows those settles correspond to — see `seedTerminalFleetEvent`
    // for why the backfill needs them and the runtime arms do not. Only the
    // reported leases get one; an expired lease's event is legitimately still
    // open, and the backfill counts it by lease status alone.
    try seedTerminalFleetEvent(ctx.conn, EVENT_PREFIX ++ "back-0", .processed);
    try seedTerminalFleetEvent(ctx.conn, EVENT_PREFIX ++ "back-1", .processed);
    try seedTerminalFleetEvent(ctx.conn, EVENT_PREFIX ++ "back-2", .fleet_error);
    const live = try counterRow(ctx.conn);

    // Drop to the pre-migration world: the history is on disk, the tally row is
    // not. This is exactly what slot 43 meets on an upgraded database.
    _ = try ctx.conn.exec("DELETE FROM fleet.runner_lifetime_counters WHERE runner_id = $1::uuid", .{RUNNER_ID});

    const backfill = try backfillSql();
    _ = try ctx.conn.exec(backfill, .{});
    const rebuilt = try counterRow(ctx.conn);
    // A fresh database and an upgraded one must converge: the recount the
    // migration derives has to equal what the write arms had been keeping.
    try expectSameCounters(live, rebuilt);
    try expectCountersMatchRecount(ctx.conn, 2, 1);

    // Reapply. The DO UPDATE arm takes GREATEST of the stored tally and the
    // recount rather than adding to it, so a re-run is a no-op on unchanged
    // history and never doubles a live runner's tally.
    _ = try ctx.conn.exec(backfill, .{});
    try expectSameCounters(rebuilt, try counterRow(ctx.conn));

    // And once retention has pruned, where the recount stops being a source of
    // truth: lifetime tallies count transitions, not surviving rows. An
    // absolute assignment would silently zero a mature runner's totals here.
    // GREATEST cannot lower anything, which is what keeps this statement safe
    // to hand an operator as the repair for the rolling-deploy gap — the
    // window where `release_command` has applied the migration but replicas
    // without the tally arms are still writing leases.
    _ = try ctx.conn.exec("DELETE FROM fleet.runner_leases WHERE runner_id = $1::uuid", .{RUNNER_ID});
    _ = try ctx.conn.exec(backfill, .{});
    try expectSameCounters(rebuilt, try counterRow(ctx.conn));

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

    // One serial acquire+settle materializes the counter row before the race.
    // The table carries TWO unique indexes over the same value (the generated
    // uid primary key and the runner_id UNIQUE the tally arms arbitrate on),
    // and ON CONFLICT can arbitrate only one of them — concurrent FIRST-touch
    // inserts can therefore die on the uid key instead of taking the update
    // arm. With the row present, every racing write goes through the update
    // arm, which is the sustained-increment invariant this test pins.
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
