//! Fault-injection proofs for the ready-first lease path, riding
//! `event_lifecycle_integration_test.zig`'s harness (real schema, real Redis;
//! self-skips when either datastore is absent) like its sibling
//! `assign_ready_integration_test.zig`.
//!
//! Every test here makes the readiness index or its backing consumer group
//! MISBEHAVE — an over-full index, a key of the wrong type, a group deleted
//! out-of-band, a field no producer could have written — and proves the lease
//! path degrades to a bounded no-work answer plus self-repair instead of
//! failing a runner, stranding an event, or scanning without bound.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const base = @import("event_lifecycle_integration_test.zig");
const fixtures = @import("../db/test_fixtures.zig");
const fleet_ready = @import("../queue/fleet_ready.zig");
const queue_consts = @import("../queue/constants.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const id_format = @import("../types/id_format.zig");
const mc = @import("../observability/metrics_counters.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const TestHarness = @import("../http/test_harness.zig").TestHarness;

const CMD_DEL = "DEL";
const CMD_SET = "SET";
const CMD_HSET = "HSET";
const CMD_HGET = "HGET";
const CMD_XGROUP = "XGROUP";
const ARG_DESTROY = "DESTROY";

/// Planted where only canonical fleet ids belong: a field no in-repo producer
/// could have written (every producer validates ids at ingress), so the peek
/// must both skip it and delete it.
const STRAY_FIELD = "stray-operator-hset";
const STRAY_VALUE = "not-a-token";

/// Fleets owned by this suite. Distinct from the sibling suites' ids so a
/// shared test database never crosses them.
const FLEET_FAULT_MARK = "0195c9da-1e2a-7f13-8abc-2b3e1e0e7e01";
const FLEET_MEMO_GONE = "0195c9da-1e2a-7f13-8abc-2b3e1e0e7e02";
const FLEET_HEAL = "0195c9da-1e2a-7f13-8abc-2b3e1e0e7e03";
const FLEET_KILLED = "0195c9da-1e2a-7f13-8abc-2b3e1e0e7e04";

/// Every action auto-kills: the deterministic path to the gate's automatic
/// pause, which must clear readiness the way the operator-facing PATCH does.
const CONFIG_GATED_KILL =
    \\{"name":"fault-kill","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0},"gates":{"rules":[{"tool":"*","action":"*","behavior":"auto_kill"}],"timeout_ms":1800000}}}
;

/// Ceiling-proof population: more marked fleets than one poll may examine,
/// with a named remainder the poll must leave marked. The gap between the two
/// is what makes both assertions below meaningful.
const UNEXAMINED_REMAINDER: usize = 10;
const OVERFULL_FLEETS: usize = common.MAX_READY_CANDIDATES_PER_POLL + UNEXAMINED_REMAINDER;

/// Id template for the ceiling population — version nibble 7, variant 8, the
/// last four digits the loop index, so every id passes the canonical check and
/// the schema CHECK while staying disjoint from every named fixture above.
const OVERFULL_ID_FMT = "0195c9da-1e2a-7f13-8abc-2b3e1e0f{d:0>4}";

fn overfullFleetId(buf: *[id_format.UUID_TEXT_LEN]u8, index: usize) ![]const u8 {
    return std.fmt.bufPrint(buf, OVERFULL_ID_FMT, .{index});
}

/// Brownout-proof population: more marked fleets than the bailout threshold, so
/// "stopped early" is distinguishable from "ran out of candidates".
const BROWNOUT_FLEETS: usize = @as(usize, common.MAX_CONSECUTIVE_REDIS_FAILURES_PER_POLL) + 3;
const BROWNOUT_ID_FMT = "0195c9da-1e2a-7f13-8abc-2b3e1e10{d:0>4}";

fn brownoutFleetId(buf: *[id_format.UUID_TEXT_LEN]u8, index: usize) ![]const u8 {
    return std.fmt.bufPrint(buf, BROWNOUT_ID_FMT, .{index});
}

/// Start from an index this test fully owns — same reasoning as the sibling
/// suite: the readiness index is ONE key shared by every suite in the binary,
/// and the whole-index assertions here (depth, examined count) cannot be
/// established over a sibling's leftovers.
fn clearWholeIndex(h: *TestHarness) !void {
    var resp = try h.queue.command(&.{ CMD_DEL, queue_consts.ready_index_key });
    resp.deinit(h.queue.alloc);
}

/// Turn the index key into a plain string, so every hash command against it
/// fails with a type error — the cheapest deterministic stand-in for a Redis
/// that accepts connections but cannot serve the index.
fn corruptIndexKey(h: *TestHarness) !void {
    var resp = try h.queue.command(&.{ CMD_SET, queue_consts.ready_index_key, STRAY_VALUE });
    resp.deinit(h.queue.alloc);
}

/// Best-effort undo of `corruptIndexKey`, deferred so a failing assertion can
/// never leave the shared key poisoned for every suite that follows.
fn dropIndexKey(h: *TestHarness) void {
    var resp = h.queue.command(&.{ CMD_DEL, queue_consts.ready_index_key }) catch return;
    resp.deinit(h.queue.alloc);
}

/// Turn a fleet's event stream into a plain string, so every stream command
/// against it fails with a type error. The cheapest deterministic stand-in for
/// a Redis that still accepts connections but cannot serve reads — which is
/// what a brownout presents as to the candidate loop.
fn corruptFleetStream(h: *TestHarness, fleet_id: []const u8) !void {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var resp = try h.queue.command(&.{ CMD_SET, key, STRAY_VALUE });
    resp.deinit(h.queue.alloc);
}

fn fieldPresent(h: *TestHarness, field: []const u8) !bool {
    var resp = try h.queue.command(&.{ CMD_HGET, queue_consts.ready_index_key, field });
    defer resp.deinit(h.queue.alloc);
    return switch (resp) {
        .bulk => |v| v != null,
        else => false,
    };
}

/// Delete the fleet's consumer group out-of-band, as an operator cleanup or a
/// stream migration would — the state the group memo can only discover by
/// failing a read.
fn destroyGroup(h: *TestHarness, fleet_id: []const u8) !void {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var resp = try h.queue.command(&.{ CMD_XGROUP, ARG_DESTROY, key, queue_consts.fleet_consumer_group });
    resp.deinit(h.queue.alloc);
}

fn purgeFleet(h: *TestHarness, fleet_id: []const u8) void {
    redis_fleet.purgeFleetRedisState(&h.queue, fleet_id) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn fleetIsPaused(conn: *pg.Conn, fleet_id: []const u8) !bool {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*) FROM core.fleets WHERE id = $1::uuid AND status = 'paused'",
        .{fleet_id},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return false;
    return (try row.get(i64, 0)) == 1;
}

// ── The candidate ceiling ───────────────────────────────────────────────────

test "integration: a poll against an over-full index examines exactly the ceiling and leaves the remainder marked" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try clearWholeIndex(h);

    // Real active fleets — the candidate query must return them, or the poll
    // would examine nothing and the ceiling assertion would be vacuous. No
    // sessions are seeded because nothing here ever leases: every fleet is
    // marked without work, so the poll's whole job is examine-and-release.
    var id_buf: [id_format.UUID_TEXT_LEN]u8 = undefined;
    var name_buf: [32]u8 = undefined;
    for (0..OVERFULL_FLEETS) |i| {
        const id = try overfullFleetId(&id_buf, i);
        const name = try std.fmt.bufPrint(&name_buf, "fault-ceiling-{d}", .{i});
        try fixtures.seedFleet(conn, id, base.WORKSPACE_ID, name, base.CONFIG_PLAIN, "# fault fixture");
        fleet_ready.mark(&h.queue, id);
    }
    defer {
        for (0..OVERFULL_FLEETS) |i| {
            const id = overfullFleetId(&id_buf, i) catch continue;
            purgeFleet(h, id);
        }
        dropIndexKey(h);
    }
    try std.testing.expectEqual(@as(u64, OVERFULL_FLEETS), try fleet_ready.depth(&h.queue));

    mc.resetLeasePollMetricsForTest();
    try std.testing.expect(!try base.pollLease(h));

    // The poll examined exactly the ceiling — the bound this whole path exists
    // to impose — and cleared exactly the fleets it proved empty, so the
    // remainder is still waiting for a later poll rather than lost.
    const snap = mc.snapshot();
    try std.testing.expectEqual(@as(u64, 1), snap.lease_polls_total);
    try std.testing.expectEqual(@as(u64, common.MAX_READY_CANDIDATES_PER_POLL), snap.lease_poll_candidates_scanned_total);
    try std.testing.expectEqual(@as(u64, UNEXAMINED_REMAINDER), try fleet_ready.depth(&h.queue));
}

// ── Redis brownout: the loop must not pin a Postgres connection ─────────────

test "integration: a run of Redis failures ends the candidate loop early" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try clearWholeIndex(h);

    // Real active fleets, each marked ready and each with a stream that cannot
    // be read. The population exceeds the bailout threshold, which is what
    // makes "stopped early" distinguishable from "ran out of candidates".
    var id_buf: [id_format.UUID_TEXT_LEN]u8 = undefined;
    var name_buf: [32]u8 = undefined;
    for (0..BROWNOUT_FLEETS) |i| {
        const id = try brownoutFleetId(&id_buf, i);
        const name = try std.fmt.bufPrint(&name_buf, "fault-brownout-{d}", .{i});
        try fixtures.seedFleet(conn, id, base.WORKSPACE_ID, name, base.CONFIG_PLAIN, "# fault fixture");
        fleet_ready.mark(&h.queue, id);
        try corruptFleetStream(h, id);
    }
    defer {
        for (0..BROWNOUT_FLEETS) |i| {
            const id = brownoutFleetId(&id_buf, i) catch continue;
            purgeFleet(h, id);
        }
        dropIndexKey(h);
    }
    try std.testing.expectEqual(@as(u64, BROWNOUT_FLEETS), try fleet_ready.depth(&h.queue));

    mc.resetLeasePollMetricsForTest();
    try std.testing.expect(!try base.pollLease(h));

    // Stopped at the threshold — not at the ceiling, and not at the population.
    // Absent the bailout this reads BROWNOUT_FLEETS, which is the whole point:
    // every extra candidate costs a Redis request timeout while the poll holds
    // a pooled Postgres connection it never uses again.
    const snap = mc.snapshot();
    try std.testing.expectEqual(@as(u64, 1), snap.lease_polls_total);
    try std.testing.expectEqual(
        @as(u64, common.MAX_CONSECUTIVE_REDIS_FAILURES_PER_POLL),
        snap.lease_poll_candidates_scanned_total,
    );

    // A failed read is not evidence of an empty stream, so every fleet stays
    // marked and the sweeper still owns the recovery.
    try std.testing.expectEqual(@as(u64, BROWNOUT_FLEETS), try fleet_ready.depth(&h.queue));
}

// ── Ingress mark failure ────────────────────────────────────────────────────

test "integration: a readiness write failure still returns the entry id to ingress and is counted" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, FLEET_FAULT_MARK, "fault-mark", base.CONFIG_PLAIN, "6");
    try clearWholeIndex(h);
    defer purgeFleet(h, FLEET_FAULT_MARK);

    // With the index key holding a string, the producer's mark must fail —
    // while the append itself, on its own key, succeeds.
    try corruptIndexKey(h);
    defer dropIndexKey(h);

    mc.resetLeasePollMetricsForTest();
    const event_id = try base.publishEvent(h, FLEET_FAULT_MARK);
    defer h.queue.alloc.free(event_id);

    // The caller got its entry id — an accepted event is never failed over a
    // lost hint — and the loss is visible as exactly one counted failure.
    try std.testing.expect(event_id.len > 0);
    try std.testing.expectEqual(@as(u64, 1), mc.snapshot().fleet_ready_write_failures_total);
}

// ── Peek failure ────────────────────────────────────────────────────────────

test "integration: a failed peek answers no-work with zero Postgres round-trips" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    try clearWholeIndex(h);

    // Same corruption as the mark test, now on the read side: the peek errors,
    // and the poll must answer no-work WITHOUT falling back to the unbounded
    // scan this path replaced — zero database round-trips is that proof.
    try corruptIndexKey(h);
    defer dropIndexKey(h);

    mc.resetLeasePollMetricsForTest();
    try std.testing.expect(!try base.pollLease(h));

    const snap = mc.snapshot();
    try std.testing.expectEqual(@as(u64, 1), snap.lease_polls_total);
    try std.testing.expectEqual(@as(u64, 0), snap.lease_poll_db_roundtrips_total);
    try std.testing.expectEqual(@as(u64, 0), snap.lease_poll_candidates_scanned_total);
}

// ── Consumer-group repair ───────────────────────────────────────────────────

test "integration: a group deleted out-of-band is repaired in place and its backlog still leases" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, FLEET_MEMO_GONE, "fault-memo", base.CONFIG_PLAIN, "7");
    try clearWholeIndex(h);
    defer purgeFleet(h, FLEET_MEMO_GONE);

    // Publish once: the group is created for real.
    const first_event = try base.publishEvent(h, FLEET_MEMO_GONE);
    defer h.queue.alloc.free(first_event);

    // An operator (or a failover) deletes the group out from under the fleet.
    try destroyGroup(h, FLEET_MEMO_GONE);

    // This poll now LEASES rather than costing one no-work answer. Under the
    // group memo, the read simply failed and the memo drop was the recovery, so
    // the fleet lost a poll. `redis_fleet.readGroup` now recreates the group
    // and reads again in place, so the repairing poll itself can lease.
    //
    // The old "exactly one no-work answer" was a consequence of the mechanism, not
    // a safety property: the invariant it protected is that a read which did NOT
    // succeed must never be read as an empty PEL, and after a successful repair the
    // retry genuinely succeeds. Reporting a fault instead would trip the Redis
    // brownout bailout (the test below), starving every candidate behind this one.
    try std.testing.expect(try base.pollLease(h));

    // The test used to publish a SECOND event here and poll again, because under
    // the old mechanism the poll above could not lease and recovery needed its own
    // proof. It no longer can: `pollLease` never completes the lease it takes, so
    // the fleet's slot is now occupied by the backlog event this poll leased, and a
    // second poll would answer null for that reason rather than for the group's.
    // The assertion above is the recovery proof, so the second half is not lost
    // coverage — it is a step whose purpose moved one line up.
}

// ── Peek self-heal ──────────────────────────────────────────────────────────

test "integration: a stray non-canonical index field is healed while the real fleet still leases" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, FLEET_HEAL, "fault-heal", base.CONFIG_PLAIN, "8");
    try clearWholeIndex(h);
    defer purgeFleet(h, FLEET_HEAL);
    defer dropIndexKey(h);

    // The stray HSET an operator could issue by hand: not a canonical UUIDv7,
    // so the candidate query could never bind it — unhealed, it would poison
    // the uuid[] cast for every poll that sampled it, forever.
    {
        var resp = try h.queue.command(&.{ CMD_HSET, queue_consts.ready_index_key, STRAY_FIELD, STRAY_VALUE });
        resp.deinit(h.queue.alloc);
    }

    const event_id = try base.publishEvent(h, FLEET_HEAL);
    defer h.queue.alloc.free(event_id);

    // The real fleet leases — the stray field cost it nothing...
    try std.testing.expect(try base.pollLease(h));

    // ...and the peek deleted the stray on its way through, so the index has
    // healed itself rather than carrying the poison to the next poll.
    try std.testing.expect(!try fieldPresent(h, STRAY_FIELD));
}

// ── The gate's automatic pause ──────────────────────────────────────────────

test "integration: a gate auto-kill pauses the fleet and clears its readiness mark" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, FLEET_KILLED, "fault-kill", CONFIG_GATED_KILL, "9");
    try clearWholeIndex(h);
    defer purgeFleet(h, FLEET_KILLED);

    const event_id = try base.publishEvent(h, FLEET_KILLED);
    defer h.queue.alloc.free(event_id);
    try std.testing.expect(try fieldPresent(h, FLEET_KILLED));

    // The poll reads the event, the kill-all policy fires, and the gate
    // pauses the fleet — no lease is issued.
    try std.testing.expect(!try base.pollLease(h));
    try std.testing.expect(try fleetIsPaused(conn, FLEET_KILLED));

    // The automatic pause writes `core.fleets` directly, bypassing the status
    // PATCH handler — so it must clear readiness itself. A paused fleet never
    // re-enters the candidate query, and an uncleared field would squat in the
    // bounded peek sample for the whole pause.
    try std.testing.expect(!try fieldPresent(h, FLEET_KILLED));
}
