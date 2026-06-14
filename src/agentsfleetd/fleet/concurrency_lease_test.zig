// Concurrency proof for the per-zombie lease SLOT under 100 simultaneous
// `affinity.claim` calls racing for ONE free zombie, each on its own pooled
// connection. The claim is a single conditional UPSERT (`ON CONFLICT ... WHERE
// leased_until < now`), so exactly one of the N racers wins the row and the
// other 99 see `.taken`. This is the exactly-one-winner invariant the whole
// fencing model rests on: a loser has read no event (the claim precedes the
// event read), so nothing is orphaned, and the winner's `fencing_seq` is the
// single monotonic token the report/renew fence later compares against.
//
// Invariants asserted after all 100 join:
//   - exactly one `.won`, the rest `.taken` (no double-claim, no lost update);
//   - the winner's token is unique — no two racers report the same token;
//   - no pool exhaustion / hang — all 100 threads complete.
//
// Requires LIVE_DB=1; skipped when TEST_DATABASE_URL is unset.

const std = @import("std");
const pg = @import("pg");
const harness_mod = @import("../http/test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const base = @import("../db/test_fixtures.zig");
const constants = @import("common");
const affinity = @import("affinity.zig");

const ALLOC = std.testing.allocator;

const auth_mw = @import("../auth/middleware/mod.zig");

fn noopRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    _ = reg;
    _ = h;
}

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dc011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dca01";
const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dcc01";

const N_CLAIMERS = 100;

fn seedRunner(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'conc-lease-host', 'conc-lease-hash', 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{RUNNER_ID});
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn teardown(conn: *pg.Conn) void {
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE zombie_id = $1::uuid", .{ZOMBIE_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    base.teardownTenant(conn);
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

/// One claim attempt on its own pooled connection: 1 = won, 2 = taken, 0 =
/// error. `token` carries the won fencing token so the test can assert
/// uniqueness across winners.
const ClaimSlot = struct {
    code: u8 = 0,
    token: u64 = 0,
};

const Worker = struct {
    fn run(h: *TestHarness, slot: *ClaimSlot) void {
        const conn = h.acquireConn() catch return;
        defer h.releaseConn(conn);
        const c = affinity.claim(conn, ALLOC, ZOMBIE_ID, RUNNER_ID, constants.LEASE_TTL_MS) catch return;
        switch (c) {
            .won => |w| slot.* = .{ .code = 1, .token = w.token },
            .taken => slot.* = .{ .code = 2 },
        }
    }
};

test "100 concurrent claims on one free zombie yield exactly one winner" {
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
    // No affinity row seeded → the zombie's slot is unclaimed; the INSERT branch
    // of the UPSERT wins for exactly one racer, the ON CONFLICT guard rejects
    // the rest (a live claim now holds leased_until in the future).
    defer teardown(c_init);

    var slots: [N_CLAIMERS]ClaimSlot = @splat(ClaimSlot{});
    var threads: [N_CLAIMERS]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = try std.Thread.spawn(.{}, Worker.run, .{ h, &slots[i] });
    }
    for (threads) |t| t.join();

    var won: usize = 0;
    var taken: usize = 0;
    var winning_token: u64 = 0;
    for (slots) |s| {
        switch (s.code) {
            1 => {
                won += 1;
                winning_token = s.token;
            },
            2 => taken += 1,
            else => return error.ClaimWorkerErrored,
        }
    }
    // Exactly one winner per zombie — the losers consumed no event, so nothing
    // is orphaned; the fence has a single owner.
    try std.testing.expectEqual(@as(usize, 1), won);
    try std.testing.expectEqual(@as(usize, N_CLAIMERS - 1), taken);
    try std.testing.expect(winning_token >= 1);
}
