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
    .cumulative_input = 1000,
    .cumulative_cached = 500,
    .cumulative_output = 800,
    .run_nanos_per_sec = RATES.run_nanos_per_sec,
    .input_nanos_per_mtok = RATES.input_nanos_per_mtok,
    .cached_input_nanos_per_mtok = RATES.cached_input_nanos_per_mtok,
    .output_nanos_per_mtok = RATES.output_nanos_per_mtok,
};

const auth_mw = @import("../auth/middleware/mod.zig");

fn noopRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    _ = reg;
    _ = h;
}

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0db011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dba01";
const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dbc01";
const AFFINITY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dbe01";
const LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dbf01";

const NOW_MS: i64 = 1_900_000_000_000;
const N_RENEWERS = 100;

fn seedRunner(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, status, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'conc-renew-host', 'conc-renew-hash', 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{RUNNER_ID});
}

fn seedAffinity(conn: *pg.Conn, fencing_seq: i64, leased_until: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, zombie_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (zombie_id) DO UPDATE
        \\  SET fencing_seq = EXCLUDED.fencing_seq, leased_until = EXCLUDED.leased_until
    , .{ AFFINITY_ID, ZOMBIE_ID, RUNNER_ID, fencing_seq, leased_until });
}

fn seedLease(conn: *pg.Conn, fencing_token: i64, created_at: i64, lease_expires_at: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, zombie_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, 'evt-conc-renew-1',
        \\        'steer:test', 'chat', '{"message":"hi"}', 0, 'platform',
        \\        'test-provider', 'test-model', 0, 0, 0, 0, $6, $7, 'active', $8, $8)
        \\ON CONFLICT (id) DO NOTHING
    , .{ LEASE_ID, RUNNER_ID, ZOMBIE_ID, WORKSPACE_ID, base.TEST_TENANT_ID, fencing_token, lease_expires_at, created_at });
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
    execIgnore(conn, "DELETE FROM core.zombie_execution_telemetry WHERE event_id = $1", .{EVENT_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE zombie_id = $1::uuid", .{ZOMBIE_ID});
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
    try seedAffinity(c_init, 5, NOW_MS - 1_000);
    try seedLease(c_init, 5, NOW_MS - 2_000, NOW_MS - 1_000);
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
    const aff_until = try readBigint(c_init, "SELECT leased_until FROM fleet.runner_affinity WHERE zombie_id = $1::uuid", ZOMBIE_ID);
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
    try seedAffinity(c, 5, NOW_MS - 1_000);
    try seedLease(c, 5, NOW_MS - 2_000, NOW_MS - 1_000);
    // Cursor baseline: 20s of runtime elapsed, no tokens metered yet. The first
    // renewal to win prices this slice off the cursor; `FOR UPDATE OF l, a` makes
    // the other 99 block, re-read the advanced cursor (Δ=0), and charge ≈0 —
    // exactly-once under the race. Without the lock each would re-charge the full
    // slice off the stale pre-advance cursor (the P0 double-charge).
    _ = try c.exec("UPDATE fleet.runner_affinity SET last_metered_at_ms = $2 WHERE zombie_id = $1::uuid", .{ ZOMBIE_ID, CURSOR_BASE_MS });
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
        _ = conn.exec("UPDATE fleet.runner_affinity SET fencing_seq = fencing_seq + 1, updated_at = $2 WHERE zombie_id = $1::uuid", .{ ZOMBIE_ID, NOW_MS }) catch return;
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
    try seedAffinity(c, 5, NOW_MS - 1_000);
    try seedLease(c, 5, NOW_MS - 2_000, NOW_MS - 1_000);
    // 20s of run elapsed (bounded slice); ample balance so no clamp.
    _ = try c.exec("UPDATE fleet.runner_affinity SET last_metered_at_ms = $2 WHERE zombie_id = $1::uuid", .{ ZOMBIE_ID, CURSOR_BASE_MS });
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
