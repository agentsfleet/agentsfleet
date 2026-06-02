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

const ALLOC = std.testing.allocator;

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
        \\  (id, zombie_id, last_runner_id, fencing_seq, leased_until, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, 0, 0)
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

fn teardown(conn: *pg.Conn) void {
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

const Worker = struct {
    fn run(h: *TestHarness, slot: *RenewSlot) void {
        const conn = h.acquireConn() catch return;
        defer h.releaseConn(conn);
        const outcome = renewal.renew(conn, LEASE_ID, RUNNER_ID, NOW_MS) catch return;
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
        t.* = try std.Thread.spawn(.{}, Worker.run, .{ h, &slots[i] });
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
