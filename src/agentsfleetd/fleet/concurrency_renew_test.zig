// Concurrency proof for the dual-row lease renewal under 100 simultaneous
// `renewal.renew` calls from the SAME runner on the SAME active lease, each on
// its own pooled connection. The renewal is a single writable-CTE statement
// guarded by the live fence — it is not a one-shot claim, so every call that
// sees the fence holding extends both rows to the SAME clamped deadline. The
// invariant under contention is therefore CONVERGENCE, not a single winner:
//   - every renew returns either `.renewed{D}` for one shared deadline D, or
//     `.lost` (none should be lost here — no reclaim competes), never a
//     diverged value;
//   - after all 100 join, the lease row and the affinity slot hold the SAME
//     deadline (the dual-row divergence guard held under the full race);
//   - no pool exhaustion / hang — all 100 threads complete.
//
// Each thread passes an explicit shared `now_ms` so the clamped target is one
// deterministic value across all 100, which is what lets the convergence
// assertion be exact. Requires LIVE_DB=1; skipped when TEST_DATABASE_URL unset.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const harness_mod = @import("../http/test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const base = @import("../db/test_fixtures.zig");
const constants = @import("common");
const renewal = @import("renewal.zig");
const renewal_settle = @import("renewal_settle.zig");
const tenant_billing = @import("../state/tenant_billing.zig");

const EVENT_ID = "evt-conc-renew-1";

const ALLOC = std.testing.allocator;

// A realistic platform rate set + a 20s cursor baseline for the no-double-charge
// race. METER carries the runner's cumulative token counts (identical across all
// 100 threads — the cumulative-diff is the idempotency key).
const RATES = tenant_billing.SliceRates{
    .run_nanos_per_sec = tenant_billing.RUN_NANOS_PER_SEC,
    .input_nanos_per_mtok = 3_000_000,
    .cached_input_nanos_per_mtok = 300_000,
    .output_nanos_per_mtok = 15_000_000,
};
const CURSOR_BASE_MS: i64 = NOW_MS - 20_000;
const METER = renewal.MeterInputs{
    .cumulative_input = TEST_TOKEN_COUNT,
    .cumulative_cached = 500,
    .cumulative_output = 800,
    .run_nanos_per_sec = RATES.run_nanos_per_sec,
    .input_nanos_per_mtok = RATES.input_nanos_per_mtok,
    .cached_input_nanos_per_mtok = RATES.cached_input_nanos_per_mtok,
    .output_nanos_per_mtok = RATES.output_nanos_per_mtok,
};

const auth_mw = @import("../auth/middleware/mod.zig");
const TEST_TOKEN_COUNT = 1000;
const MS_PER_SECOND = 1_000;

fn noopRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    _ = reg;
    _ = h;
}

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0db011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dba01";
const AGENTSFLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dbc01";
const AFFINITY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dbe01";
const LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dbf01";

const NOW_MS: i64 = 1_900_000_000_000;
const N_RENEWERS = 100;

fn seedRunner(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'conc-renew-host', 'conc-renew-hash', 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{RUNNER_ID});
}

fn seedAffinity(conn: *pg.Conn, fencing_seq: i64, leased_until: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, agent_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (agent_id) DO UPDATE
        \\  SET fencing_seq = EXCLUDED.fencing_seq, leased_until = EXCLUDED.leased_until
    , .{ AFFINITY_ID, AGENTSFLEET_ID, RUNNER_ID, fencing_seq, leased_until });
}

fn seedLease(conn: *pg.Conn, fencing_token: i64, created_at: i64, lease_expires_at: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, agent_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, 'evt-conc-renew-1',
        \\        'steer:test', 'chat', '{"message":"hi"}', 0, 'platform',
        \\        'test-provider', 'test-model', 0, 0, 0, 0, $6, $7, 'active', $8, $8)
        \\ON CONFLICT (id) DO NOTHING
    , .{ LEASE_ID, RUNNER_ID, AGENTSFLEET_ID, WORKSPACE_ID, base.TEST_TENANT_ID, fencing_token, lease_expires_at, created_at });
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

// N_RENEWERS (100) worker threads share a 4-connection pool (POOL_SIZE_DEFAULT),
// so most workers must wait for a connection. Under full-suite DB load the
// metered renew holds each connection long enough that a worker's first acquire
// can exceed the pool's acquire timeout — treat that as "pool busy" and retry,
// since the invariant under test is convergence + exactly-once charging, not
// connection-pool capacity. Bounded so a genuinely dead pool still fails (returns
// null → the worker records an error) rather than hanging.
fn acquireRetry(h: *TestHarness) ?*pg.Conn {
    var attempt: usize = 0;
    while (attempt < 30) : (attempt += 1) {
        return h.acquireConn() catch {
            @import("common").sleepNanos(20 * std.time.ns_per_ms);
            continue;
        };
    }
    return null;
}

fn teardown(conn: *pg.Conn) void {
    // Metering rows are keyed by event_id — the three tests in this file share
    // one (EVENT_ID), and the renew CTE writes a breakdown + an accumulating
    // ledger row per renewal. Clear them so a count-based assertion (slices) is
    // not polluted by a sibling test that ran earlier in the seed-shuffled order.
    execIgnore(conn, "DELETE FROM fleet.metering_periods WHERE event_id = $1", .{EVENT_ID});
    execIgnore(conn, "DELETE FROM core.agent_execution_telemetry WHERE event_id = $1", .{EVENT_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE agent_id = $1::uuid", .{AGENTSFLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    base.teardownTenant(conn);
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

fn readBigint(conn: *pg.Conn, sql: []const u8, id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(sql, .{id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    return row.get(i64, 0);
}

/// One renew on its own pooled connection. The verdict is recorded into the
/// caller's slot: 1 = renewed to the shared deadline, 2 = lost, 0 = error.
const RenewSlot = struct {
    code: u8 = 0,
    renewed_to: i64 = 0,
};

// A runner renewing its lease on its own connection. The verdict is recorded
// into the caller's slot: 1 = renewed to the shared deadline, 2 = lost, 0 = error.
const Renewer = struct {
    fn run(h: *TestHarness, slot: *RenewSlot) void {
        const conn = acquireRetry(h) orelse return;
        defer h.releaseConn(conn);
        const outcome = renewal.renew(conn, LEASE_ID, RUNNER_ID, NOW_MS, .{}) catch return;
        switch (outcome) {
            .renewed => |until| slot.* = .{ .code = 1, .renewed_to = until },
            .lost => slot.* = .{ .code = 2 },
            .max_runtime => slot.* = .{ .code = 3 },
        }
    }
};

test "100 concurrent renews on one active lease converge to a single shared deadline" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = noopRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const c_init = try h.acquireConn();
    defer h.releaseConn(c_init);

    teardown(c_init);
    try base.seedTenant(c_init);
    try base.seedWorkspace(c_init, WORKSPACE_ID);
    try seedRunner(c_init);
    // Fence holds (token == seq); created_at recent so the cap is far away and
    // every renew clamps to the same now+TTL target.
    try seedAffinity(c_init, 5, NOW_MS - MS_PER_SECOND);
    try seedLease(c_init, 5, NOW_MS - 2_000, NOW_MS - MS_PER_SECOND);
    defer teardown(c_init);

    var slots: [N_RENEWERS]RenewSlot = @splat(RenewSlot{});
    var threads: [N_RENEWERS]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = try std.Thread.spawn(.{}, Renewer.run, .{ h, &slots[i] });
    }
    for (threads) |t| t.join();

    const want = NOW_MS + constants.LEASE_TTL_MS;
    var renewed: usize = 0;
    for (slots) |s| {
        // No reclaim competes, so none should be lost and none should error.
        try std.testing.expectEqual(@as(u8, 1), s.code);
        // Every renew that ran extended to the SAME clamped deadline — no
        // diverged target across the 100-way race.
        try std.testing.expectEqual(want, s.renewed_to);
        renewed += 1;
    }
    try std.testing.expectEqual(@as(usize, N_RENEWERS), renewed);

    // The dual-row divergence guard held under contention: the lease row and
    // the affinity slot both hold the one shared deadline, never split.
    const lease_until = try readBigint(c_init, "SELECT lease_expires_at FROM fleet.runner_leases WHERE id = $1::uuid", LEASE_ID);
    const aff_until = try readBigint(c_init, "SELECT leased_until FROM fleet.runner_affinity WHERE agent_id = $1::uuid", AGENTSFLEET_ID);
    try std.testing.expectEqual(want, lease_until);
    try std.testing.expectEqual(want, aff_until);
}

fn seedBalance(conn: *pg.Conn, balance: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'conc-meter', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, balance });
}

const MeteredRenewer = struct {
    fn run(h: *TestHarness, slot: *RenewSlot) void {
        const conn = acquireRetry(h) orelse return;
        defer h.releaseConn(conn);
        const outcome = renewal.renew(conn, LEASE_ID, RUNNER_ID, NOW_MS, METER) catch return;
        switch (outcome) {
            .renewed => |until| slot.* = .{ .code = 1, .renewed_to = until },
            .lost => slot.* = .{ .code = 2 },
            .max_runtime => slot.* = .{ .code = 3 },
        }
    }
};

test "100 concurrent metered renews on one lease charge the slice exactly once (no double-charge)" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = noopRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const c = try h.acquireConn();
    defer h.releaseConn(c);

    teardown(c);
    try base.seedTenant(c);
    try base.seedWorkspace(c, WORKSPACE_ID);
    try seedRunner(c);
    try seedAffinity(c, 5, NOW_MS - MS_PER_SECOND);
    try seedLease(c, 5, NOW_MS - 2_000, NOW_MS - MS_PER_SECOND);
    // Cursor baseline: 20s of runtime elapsed, no tokens metered yet. The first
    // renewal to win prices this slice off the cursor; `FOR UPDATE OF l, a` makes
    // the other 99 block, re-read the advanced cursor (Δ=0), and charge ≈0 —
    // exactly-once under the race. Without the lock each would re-charge the full
    // slice off the stale pre-advance cursor (the P0 double-charge).
    _ = try c.exec("UPDATE fleet.runner_affinity SET last_metered_at_ms = $2 WHERE agent_id = $1::uuid", .{ AGENTSFLEET_ID, CURSOR_BASE_MS });
    const balance: i64 = 1_000_000_000_000;
    try seedBalance(c, balance);
    defer teardown(c);

    var slots: [N_RENEWERS]RenewSlot = @splat(RenewSlot{});
    var threads: [N_RENEWERS]std.Thread = undefined;
    for (&threads, 0..) |*t, i| t.* = try std.Thread.spawn(.{}, MeteredRenewer.run, .{ h, &slots[i] });
    for (threads) |t| t.join();

    // Exactly ONE slice left the wallet across the 100-way race.
    const one_slice = tenant_billing.sliceCharge(RATES, NOW_MS - CURSOR_BASE_MS, 1000, 500, 800);
    const remaining = try readBigint(c, "SELECT balance_nanos FROM billing.tenant_billing WHERE tenant_id = $1::uuid", base.TEST_TENANT_ID);
    try std.testing.expectEqual(one_slice, balance - remaining);
}

fn leaseReported(conn: *pg.Conn) !bool {
    var q = PgQuery.from(try conn.query("SELECT status FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    return std.mem.eql(u8, try row.get([]const u8, 0), "reported");
}

const ReportSlot = struct { ran: bool = false, claimed: bool = false, charged: i64 = 0 };

const Reporter = struct {
    fn run(h: *TestHarness, slot: *ReportSlot) void {
        const conn = acquireRetry(h) orelse return;
        defer h.releaseConn(conn);
        const out = renewal_settle.claimAndSettle(conn, LEASE_ID, RUNNER_ID, NOW_MS, METER) catch return;
        slot.* = .{ .ran = true, .claimed = out.claimed, .charged = out.charged_nanos };
    }
};

// Simulates the conflicting write a reclaim makes: bump the affinity fence under
// the row lock (a plain UPDATE locks the row, contending with claim+settle's
// `FOR UPDATE OF l, a`). This is precisely the operation the fold must serialise.
const Reclaimer = struct {
    fn run(h: *TestHarness) void {
        const conn = acquireRetry(h) orelse return;
        defer h.releaseConn(conn);
        _ = conn.exec("UPDATE fleet.runner_affinity SET fencing_seq = fencing_seq + 1, updated_at = $2 WHERE agent_id = $1::uuid", .{ AGENTSFLEET_ID, NOW_MS }) catch return;
    }
};

test "claim+settle racing a reclaim never reports without charging the final slice" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = noopRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const c = try h.acquireConn();
    defer h.releaseConn(c);

    teardown(c);
    try base.seedTenant(c);
    try base.seedWorkspace(c, WORKSPACE_ID);
    try seedRunner(c);
    // Fence holds at issue (token == seq == 5) so the claim CAN win; a racing
    // reclaim bumps the sequence to 6+, which would fence the claim out.
    try seedAffinity(c, 5, NOW_MS - MS_PER_SECOND);
    try seedLease(c, 5, NOW_MS - 2_000, NOW_MS - MS_PER_SECOND);
    // 20s of run elapsed (bounded slice); ample balance so no clamp.
    _ = try c.exec("UPDATE fleet.runner_affinity SET last_metered_at_ms = $2 WHERE agent_id = $1::uuid", .{ AGENTSFLEET_ID, CURSOR_BASE_MS });
    const balance: i64 = 1_000_000_000_000;
    try seedBalance(c, balance);
    defer teardown(c); // also clears metering_periods + telemetry for EVENT_ID

    // One claim+settle (the report) racing 8 reclaim fence-bumps on the same slot.
    var slot = ReportSlot{};
    var threads: [9]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, Reporter.run, .{ h, &slot });
    for (threads[1..]) |*t| t.* = try std.Thread.spawn(.{}, Reclaimer.run, .{h});
    for (threads) |t| t.join();
    try std.testing.expect(slot.ran);

    const reported = try leaseReported(c);
    const debited = balance - try readBigint(c, "SELECT balance_nanos FROM billing.tenant_billing WHERE tenant_id = $1::uuid", base.TEST_TENANT_ID);
    const slices = try readBigint(c, "SELECT count(*)::bigint FROM fleet.metering_periods WHERE event_id = $1", EVENT_ID);
    const one_slice = tenant_billing.sliceCharge(RATES, NOW_MS - CURSOR_BASE_MS, 1000, 500, 800);

    // The fold's invariant: the active→reported flip and the slice debit are ONE
    // atomic outcome. Either the claim won the fence (reported + charged + 1 slice)
    // or a reclaim bumped the sequence first (still active + nothing charged) —
    // NEVER reported-without-charge (the P1 race) nor charged-without-report.
    if (slot.claimed) {
        try std.testing.expect(reported);
        try std.testing.expectEqual(one_slice, debited);
        try std.testing.expectEqual(one_slice, slot.charged);
        try std.testing.expectEqual(@as(i64, 1), slices);
    } else {
        try std.testing.expect(!reported);
        try std.testing.expectEqual(@as(i64, 0), debited);
        try std.testing.expectEqual(@as(i64, 0), slices);
    }
}

// ── audit rows == wallet drain under same-tenant exhaustion ──
//
// Two concurrent money ops for the SAME tenant on DIFFERENT leases do not
// contend on l/a (distinct rows), so before the balance-row lock (the `bal`
// CTE) both priced `charged` off the same stale pre-lock balance read and the
// audit rows (fleet.metering_periods + the telemetry breakdown — the invoice
// substrate) summed to MORE than the wallet actually drained. The balance-row
// lock serialises them, so the loser charges only the remaining balance.

const AGENTSFLEET_ID_2 = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dbc02";
const AFFINITY_ID_2 = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dbe02";
const LEASE_ID_2 = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dbf02";
const EVENT_ID_2 = "evt-conc-renew-2";

// A second active lease for the SAME tenant on its own agent, with the same
// 20s cursor baseline so its slice price equals the first lease's.
fn seedSecondLease(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, agent_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 5, $4, 0, 0, 0, $5, 0, 0)
        \\ON CONFLICT (agent_id) DO UPDATE
        \\  SET fencing_seq = 5, leased_until = EXCLUDED.leased_until, last_metered_at_ms = EXCLUDED.last_metered_at_ms,
        \\      metered_input_tokens = 0, metered_cached_tokens = 0, metered_output_tokens = 0
    , .{ AFFINITY_ID_2, AGENTSFLEET_ID_2, RUNNER_ID, NOW_MS - MS_PER_SECOND, CURSOR_BASE_MS });
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, agent_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6,
        \\        'steer:test', 'chat', '{"message":"hi"}', 0, 'platform',
        \\        'test-provider', 'test-model', 0, 0, 0, 0, 5, $7, 'active', $8, $8)
        \\ON CONFLICT (id) DO NOTHING
    , .{ LEASE_ID_2, RUNNER_ID, AGENTSFLEET_ID_2, WORKSPACE_ID, base.TEST_TENANT_ID, EVENT_ID_2, NOW_MS - MS_PER_SECOND, NOW_MS - 2_000 });
}

fn teardownSecond(conn: *pg.Conn) void {
    execIgnore(conn, "DELETE FROM fleet.metering_periods WHERE event_id = $1", .{EVENT_ID_2});
    execIgnore(conn, "DELETE FROM core.agent_execution_telemetry WHERE event_id = $1", .{EVENT_ID_2});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID_2});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE agent_id = $1::uuid", .{AGENTSFLEET_ID_2});
}

fn auditSum(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COALESCE(SUM(charged_nanos),0)::bigint FROM fleet.metering_periods WHERE event_id IN ($1, $2)",
        .{ EVENT_ID, EVENT_ID_2 },
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    return row.get(i64, 0);
}


/// Wait (bounded) until `min_waiters` backends are blocked on a row lock
/// inside the renew/settle CTE (`WITH probe ...`). Replaces a fixed sleep:
/// on a loaded machine a sleep can elapse before the workers even acquire
/// their pool connections, silently degrading the forced interleaving the
/// exhaustion tests exist to pin.
fn waitForRenewLockWaiters(conn: *pg.Conn, min_waiters: i64) !void {
    var attempts: usize = 0;
    while (attempts < 250) : (attempts += 1) { // 250 × 20 ms = 5 s cap
        const waiting = blk: {
            var q = PgQuery.from(try conn.query(
                "SELECT count(*)::bigint FROM pg_stat_activity WHERE wait_event_type = 'Lock' AND query LIKE 'WITH probe%'",
                .{},
            ));
            defer q.deinit();
            const row = (try q.next()) orelse break :blk @as(i64, 0);
            break :blk try row.get(i64, 0);
        };
        if (waiting >= min_waiters) return;
        constants.sleepNanos(20 * std.time.ns_per_ms);
    }
    return error.RenewWorkersNeverBlocked;
}

const LeaseRenewer = struct {
    fn run(h: *TestHarness, lease_id: []const u8, slot: *RenewSlot) void {
        const conn = acquireRetry(h) orelse return;
        defer h.releaseConn(conn);
        const outcome = renewal.renew(conn, lease_id, RUNNER_ID, NOW_MS, METER) catch return;
        switch (outcome) {
            .renewed => |until| slot.* = .{ .code = 1, .renewed_to = until },
            .lost => slot.* = .{ .code = 2 },
            .max_runtime => slot.* = .{ .code = 3 },
        }
    }
};

const SettleSlot = struct { charged: i64 = 0, claimed: bool = false, ran: bool = false };

const SettleWorker = struct {
    fn run(h: *TestHarness, lease_id: []const u8, slot: *SettleSlot) void {
        const conn = acquireRetry(h) orelse return;
        defer h.releaseConn(conn);
        const out = renewal_settle.claimAndSettle(conn, lease_id, RUNNER_ID, NOW_MS, METER) catch return;
        slot.* = .{ .ran = true, .claimed = out.claimed, .charged = out.charged_nanos };
    }
};

test "integration: two same-tenant renews at exhaustion record audit rows summing to the wallet drain" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = noopRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const c = try h.acquireConn();
    defer h.releaseConn(c);

    teardown(c);
    teardownSecond(c);
    try base.seedTenant(c);
    try base.seedWorkspace(c, WORKSPACE_ID);
    try seedRunner(c);
    try seedAffinity(c, 5, NOW_MS - MS_PER_SECOND);
    try seedLease(c, 5, NOW_MS - 2_000, NOW_MS - MS_PER_SECOND);
    _ = try c.exec("UPDATE fleet.runner_affinity SET last_metered_at_ms = $2 WHERE agent_id = $1::uuid", .{ AGENTSFLEET_ID, CURSOR_BASE_MS });
    try seedSecondLease(c);
    defer teardown(c);
    defer teardownSecond(c);

    // Each lease prices the same slice off the 20s cursor. Fund only 1.5 slices,
    // so the two concurrent renews exhaust the balance: the loser must charge
    // the remaining half, not a second full slice.
    const slice = tenant_billing.sliceCharge(RATES, NOW_MS - CURSOR_BASE_MS, 1000, 500, 800);
    const balance: i64 = slice + @divTrunc(slice, 2);
    try seedBalance(c, balance);

    // Force the interleaving deterministically: a blocker transaction holds the
    // balance row lock so BOTH renewals reach their balance read/update before
    // either commits. Without the `tb` lock both price `charged` off the full
    // balance (the over-report this test pins); with it they serialise on the
    // probe and the loser charges only the remainder.
    const blocker = try h.acquireConn();
    _ = try blocker.exec("BEGIN", .{});
    {
        var bq = PgQuery.from(try blocker.query("SELECT balance_nanos FROM billing.tenant_billing WHERE tenant_id = $1::uuid FOR UPDATE", .{base.TEST_TENANT_ID}));
        defer bq.deinit();
        _ = try bq.next();
    }
    var slots: [2]RenewSlot = @splat(RenewSlot{});
    var threads: [2]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, LeaseRenewer.run, .{ h, @as([]const u8, LEASE_ID), &slots[0] });
    threads[1] = try std.Thread.spawn(.{}, LeaseRenewer.run, .{ h, @as([]const u8, LEASE_ID_2), &slots[1] });
    try waitForRenewLockWaiters(c, 2); // both renewals provably blocked on the balance row
    _ = try blocker.exec("COMMIT", .{});
    h.releaseConn(blocker);
    for (threads) |t| t.join();

    const remaining = try readBigint(c, "SELECT balance_nanos FROM billing.tenant_billing WHERE tenant_id = $1::uuid", base.TEST_TENANT_ID);
    try std.testing.expectEqual(@as(i64, 0), remaining); // 1.5 slices funded, ≥1.5 wanted → drained to zero
    // The audit rows (one breakdown per event) sum to the real drain — never
    // 2×slice, which the un-locked-balance probe used to record.
    try std.testing.expectEqual(balance - remaining, try auditSum(c));
}

test "integration: two same-tenant settles at exhaustion record audit rows summing to the wallet drain" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = noopRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const c = try h.acquireConn();
    defer h.releaseConn(c);

    teardown(c);
    teardownSecond(c);
    try base.seedTenant(c);
    try base.seedWorkspace(c, WORKSPACE_ID);
    try seedRunner(c);
    try seedAffinity(c, 5, NOW_MS - MS_PER_SECOND);
    try seedLease(c, 5, NOW_MS - 2_000, NOW_MS - MS_PER_SECOND);
    _ = try c.exec("UPDATE fleet.runner_affinity SET last_metered_at_ms = $2 WHERE agent_id = $1::uuid", .{ AGENTSFLEET_ID, CURSOR_BASE_MS });
    try seedSecondLease(c);
    defer teardown(c);
    defer teardownSecond(c);

    const slice = tenant_billing.sliceCharge(RATES, NOW_MS - CURSOR_BASE_MS, 1000, 500, 800);
    const balance: i64 = slice + @divTrunc(slice, 2);
    try seedBalance(c, balance);

    const blocker = try h.acquireConn();
    _ = try blocker.exec("BEGIN", .{});
    {
        var bq = PgQuery.from(try blocker.query("SELECT balance_nanos FROM billing.tenant_billing WHERE tenant_id = $1::uuid FOR UPDATE", .{base.TEST_TENANT_ID}));
        defer bq.deinit();
        _ = try bq.next();
    }
    var slots: [2]SettleSlot = @splat(SettleSlot{});
    var threads: [2]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, SettleWorker.run, .{ h, @as([]const u8, LEASE_ID), &slots[0] });
    threads[1] = try std.Thread.spawn(.{}, SettleWorker.run, .{ h, @as([]const u8, LEASE_ID_2), &slots[1] });
    try waitForRenewLockWaiters(c, 2); // both settles provably blocked on the balance row
    _ = try blocker.exec("COMMIT", .{});
    h.releaseConn(blocker);
    for (threads) |t| t.join();

    try std.testing.expect(slots[0].claimed and slots[1].claimed); // both settled their lease
    const remaining = try readBigint(c, "SELECT balance_nanos FROM billing.tenant_billing WHERE tenant_id = $1::uuid", base.TEST_TENANT_ID);
    try std.testing.expectEqual(@as(i64, 0), remaining);
    // Returned charges AND the persisted audit rows both equal the real drain.
    try std.testing.expectEqual(balance - remaining, slots[0].charged + slots[1].charged);
    try std.testing.expectEqual(balance - remaining, try auditSum(c));
}

test "integration: a regressed cumulative token report charges zero tokens and never rewinds the cursor" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = noopRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const c = try h.acquireConn();
    defer h.releaseConn(c);

    teardown(c);
    try base.seedTenant(c);
    try base.seedWorkspace(c, WORKSPACE_ID);
    try seedRunner(c);
    try seedAffinity(c, 5, NOW_MS - MS_PER_SECOND);
    try seedLease(c, 5, NOW_MS - 2_000, NOW_MS - MS_PER_SECOND);
    // Stored cursor sits ABOVE the report we are about to send, and 20s of
    // runtime already elapsed.
    const seeded_cursor: i64 = 1000;
    const balance: i64 = 1_000_000_000_000;
    _ = try c.exec("UPDATE fleet.runner_affinity SET metered_input_tokens = $2, metered_cached_tokens = $2, metered_output_tokens = $2, last_metered_at_ms = $3 WHERE agent_id = $1::uuid", .{ AGENTSFLEET_ID, seeded_cursor, CURSOR_BASE_MS });
    _ = try c.exec("UPDATE fleet.runner_leases SET metered_input_tokens = $2, metered_cached_tokens = $2, metered_output_tokens = $2 WHERE id = $1::uuid", .{ LEASE_ID, seeded_cursor });
    try seedBalance(c, balance);
    defer teardown(c);

    // Report LOWER cumulative tokens than the stored cursor — a regression.
    const regressed = renewal.MeterInputs{
        .cumulative_input = 500,
        .cumulative_cached = 500,
        .cumulative_output = 500,
        .run_nanos_per_sec = RATES.run_nanos_per_sec,
        .input_nanos_per_mtok = RATES.input_nanos_per_mtok,
        .cached_input_nanos_per_mtok = RATES.cached_input_nanos_per_mtok,
        .output_nanos_per_mtok = RATES.output_nanos_per_mtok,
    };
    const out = try renewal.renew(c, LEASE_ID, RUNNER_ID, NOW_MS, regressed);
    try std.testing.expect(out == .renewed);

    // The cursor held (GREATEST clamp), never rewound to the regressed 500.
    try std.testing.expectEqual(seeded_cursor, try readBigint(c, "SELECT metered_input_tokens FROM fleet.runner_affinity WHERE agent_id = $1::uuid", AGENTSFLEET_ID));
    // Zero token delta charged for this slice (run_fee may be > 0; token cost is 0).
    try std.testing.expectEqual(@as(i64, 0), try readBigint(c, "SELECT COALESCE(token_cost_nanos,0)::bigint FROM fleet.metering_periods WHERE event_id = $1 ORDER BY slice_seq DESC LIMIT 1", EVENT_ID));
}
