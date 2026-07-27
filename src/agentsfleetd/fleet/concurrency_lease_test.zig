// Concurrency proof for the per-fleet lease SLOT under 100 simultaneous
// `affinity.claim` calls racing for ONE free fleet, each on its own pooled
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
const serve_runner_lookup = @import("../cmd/serve_runner_lookup.zig");
const api_key = @import("../auth/api_key.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const queue_consts = @import("../queue/constants.zig");
const protocol = @import("contract").protocol;
const clock = constants.clock;

const ALLOC = std.testing.allocator;

const auth_mw = @import("../auth/middleware/mod.zig");

fn noopRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    _ = reg;
    _ = h;
}

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dc011";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dca01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dcc01";

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
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    // Fleet before tenant/workspace — core.fleets.workspace_id has no cascade;
    // cascades any residual affinity via the new FK.
    base.teardownFleets(conn, WORKSPACE_ID);
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
        const c = affinity.claim(conn, ALLOC, FLEET_ID, RUNNER_ID, constants.LEASE_TTL_MS) catch return;
        switch (c) {
            .won => |w| slot.* = .{ .code = 1, .token = w.token },
            .taken => slot.* = .{ .code = 2 },
        }
    }
};

test "100 concurrent claims on one free fleet yield exactly one winner" {
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
    try base.seedFleet(c_init, FLEET_ID, WORKSPACE_ID, "conc-lease", "{}", "# z");
    try seedRunner(c_init);
    // No affinity row seeded → the fleet's slot is unclaimed; the INSERT branch
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
    // Exactly one winner per fleet — the losers consumed no event, so nothing
    // is orphaned; the fence has a single owner.
    try std.testing.expectEqual(@as(usize, 1), won);
    try std.testing.expectEqual(@as(usize, N_CLAIMERS - 1), taken);
    try std.testing.expect(winning_token >= 1);
}

// ── The same invariant through the full HTTP lease path ─────────────────────
//
// The claim-layer proof above races `affinity.claim` directly; this one races
// one hundred real `POST /v1/runners/me/leases` requests — router, runner
// bearer middleware, readiness peek, candidate query, claim, stream read,
// lease insert — against ONE ready fleet holding ONE event. Exactly one
// request walks away with the lease; the other ninety-nine get a well-formed
// no-work answer, never an error. Requires live DB + Redis; skips otherwise.

const CMD_DEL = "DEL";

// UUIDv7 literals (version nibble 7, variant 8), disjoint from the claim-layer
// fixtures above so the two tests never share database state.
const HTTP_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dc012";
const HTTP_RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dca02";
const HTTP_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dcc02";
const HTTP_SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dcd02";
const HTTP_RUNNER_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "c" ** 64;
const LARGE_BALANCE_NANOS: i64 = 1000000000000;

const CONFIG_HTTP_RACE =
    \\{"name":"conc-http-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;

// SAFETY: populated by httpRegistry (with the harness pool) before the
// middleware chain — and thus the lookup — ever reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn httpRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn seedHttpRunner(conn: *pg.Conn) !void {
    const hash = api_key.sha256Hex(HTTP_RUNNER_TOKEN);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'conc-http-host', $2, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ HTTP_RUNNER_ID, hash[0..] });
}

fn fundHttpBalance(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'conc-http-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, LARGE_BALANCE_NANOS });
}

fn publishHttpEvent(h: *TestHarness) !void {
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, HTTP_FLEET_ID);
    const id = try redis_fleet.xaddFleetEvent(&h.queue, .{
        .event_id = "",
        .fleet_id = HTTP_FLEET_ID,
        .workspace_id = HTTP_WORKSPACE_ID,
        .actor = "steer:test-user",
        .event_type = .chat,
        .request_json = "{\"message\":\"ping\"}",
        .created_at = clock.nowMillis(),
    });
    h.queue.alloc.free(id);
}

/// Start from a readiness index this test fully owns: `peek` samples ONE hash
/// shared by every suite in the binary, and the single-winner count below is
/// only meaningful when this fleet's mark is the only leasable entry.
fn dropReadyIndex(h: *TestHarness) void {
    var resp = h.queue.command(&.{ CMD_DEL, queue_consts.ready_index_key }) catch return;
    resp.deinit(h.queue.alloc);
}

fn cleanupHttp(h: *TestHarness, conn: *pg.Conn) void {
    redis_fleet.purgeFleetRedisState(&h.queue, HTTP_FLEET_ID) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE fleet_id = $1::uuid", .{HTTP_FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{HTTP_FLEET_ID});
    execIgnore(conn, "DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid", .{HTTP_FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{HTTP_RUNNER_ID});
    base.teardownPlatformProvider(conn, HTTP_WORKSPACE_ID);
    base.teardownFleets(conn, HTTP_WORKSPACE_ID);
    base.teardownWorkspace(conn, HTTP_WORKSPACE_ID);
    base.teardownTenant(conn);
}

/// One full HTTP lease poll. `status` stays 0 when the request errored before
/// a reply arrived, so the assertions below catch transport failures as
/// loudly as a wrong status code.
const HttpPollWorker = struct {
    fn run(
        h: *TestHarness,
        status: *u16,
        leased: *bool,
        ready_count: *std.atomic.Value(usize),
        start_gate: *std.atomic.Value(bool),
    ) void {
        // safe because: the increment pairs with the spawner's acquire spin,
        // and the gate's release store pairs with this acquire load; the
        // thread join is the final synchronization point for the results.
        _ = ready_count.fetchAdd(1, .acq_rel);
        while (!start_gate.load(.acquire)) std.atomic.spinLoopHint();
        const with_bearer = h.post(protocol.PATH_RUNNER_LEASES).bearer(HTTP_RUNNER_TOKEN) catch return;
        const req = with_bearer.json("{}") catch return;
        const resp = req.send() catch return;
        defer resp.deinit();
        status.* = resp.status;
        leased.* = std.mem.indexOf(u8, resp.body, "\"lease\":null") == null;
    }
};

test "100 concurrent HTTP lease polls on one ready fleet yield exactly one lease" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = httpRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    cleanupHttp(h, conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, HTTP_WORKSPACE_ID);
    try base.seedPlatformProvider(ALLOC, conn, HTTP_WORKSPACE_ID);
    try fundHttpBalance(conn);
    try seedHttpRunner(conn);
    try base.seedFleet(conn, HTTP_FLEET_ID, HTTP_WORKSPACE_ID, "conc-http-bot", CONFIG_HTTP_RACE, "# race");
    try base.seedFleetSession(conn, HTTP_SESSION_ID, HTTP_FLEET_ID, "{}");
    defer cleanupHttp(h, conn);
    dropReadyIndex(h);
    try publishHttpEvent(h);

    var statuses: [N_CLAIMERS]u16 = .{0} ** N_CLAIMERS;
    var leased: [N_CLAIMERS]bool = .{false} ** N_CLAIMERS;
    var ready = std.atomic.Value(usize).init(0);
    var gate = std.atomic.Value(bool).init(false);
    var threads: [N_CLAIMERS]std.Thread = undefined;
    for (&threads, 0..) |*t, i| t.* = try std.Thread.spawn(.{}, HttpPollWorker.run, .{
        h, &statuses[i], &leased[i], &ready, &gate,
    });
    // safe because: the acquire spin pairs with the workers' acq_rel
    // increments, and the release store publishes the gate to their spins.
    while (ready.load(.acquire) != N_CLAIMERS) std.atomic.spinLoopHint();
    gate.store(true, .release);
    for (threads) |t| t.join();

    // Every racer got the protocol's well-formed always-200 poll reply, and
    // exactly one of them holds the lease. The losers read no event — the
    // claim precedes the stream read — so nothing was consumed twice.
    var winners: usize = 0;
    for (statuses, leased) |status, won_lease| {
        try std.testing.expectEqual(@as(u16, 200), status);
        if (won_lease) winners += 1;
    }
    try std.testing.expectEqual(@as(usize, 1), winners);
}
