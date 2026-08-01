//! Integration proofs for the ready-first lease path and the readiness index.
//!
//! Rides `event_lifecycle_integration_test.zig`'s harness (real schema, real
//! Redis) so fixtures are production-shaped per RULE ITF. Self-skips when the
//! test datastores are absent.
//!
//! The load-bearing proof here is the zero-Postgres idle poll: it asserts the
//! per-poll database round-trip counter, not a review reading of the code, which
//! is why that counter is part of the shipped metric surface rather than a
//! test-only hook.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const base = @import("event_lifecycle_integration_test.zig");
const fleet_ready = @import("../queue/fleet_ready.zig");
const queue_consts = @import("../queue/constants.zig");
const id_format = @import("../types/id_format.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const mc = @import("../observability/metrics_counters.zig");
const TestHarness = @import("../http/test_harness.zig").TestHarness;

const ALLOC = std.testing.allocator;

/// Fleets owned by this suite. Distinct from the sibling suites' ids so a shared
/// test database never crosses them.
const FLEET_READY_A = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7e01";
const FLEET_READY_B = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7e02";
const FLEET_TAGGED = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7e03";
const FLEET_MEMO = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7e04";
const FLEET_REPAIR = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7e05";

/// Polls the group-memo proof issues after the one real create.
const LEASE_POLLS: usize = 10;

/// Gap between marks in the token-order proof: wide enough that each mint
/// lands in a later millisecond, so the leading time field must differ.
const TOKEN_MINT_GAP_MS: u64 = 2;

const CMD_DEL = "DEL";
const CMD_XGROUP = "XGROUP";
const CMD_DESTROY = "DESTROY";
const CMD_HGET = "HGET";
const CMD_HSET = "HSET";
const CMD_HDEL = "HDEL";

/// Synthetic fleets the bound proof marks, and the bound it reads them back with.
/// The gap between them is what makes the assertion meaningful.
const SYNTHETIC_FLEETS: usize = 20;
const PEEK_BOUND: usize = 5;

/// Start from an index this test fully owns.
///
/// The readiness index is ONE key shared by the whole deployment, and therefore
/// by every other suite in this test binary. Several assertions here are about
/// the index AS A WHOLE — "an empty index costs no Postgres", "a poll finds
/// nothing deliverable" — and those cannot be established while a sibling's fleet
/// is still marked.
///
/// Wiping is safe rather than hostile because tests run sequentially and every
/// suite marks its own fleet immediately before it polls: there is never a
/// sibling mark in flight to un-ready. Field-scoped cleanup was tried first and
/// is strictly worse — it leaves this suite's assertions at the mercy of whatever
/// ran before it, which is how a shared-state failure becomes intermittent.
///
/// Assertions that CAN be scoped to this suite's own fleets are written that way
/// regardless, so a future parallel runner degrades them rather than breaking them.
fn clearWholeIndex(h: *TestHarness) !void {
    var resp = try h.queue.command(&.{ CMD_DEL, queue_consts.ready_index_key });
    resp.deinit(h.queue.alloc);
}

/// The token stored for `fleet_id` as reported by `peek`, or null when the peek
/// did not return that fleet. Lets a test assert about its OWN fleet without
/// asserting how many entries the shared index happens to hold.
fn peekedToken(entries: []const fleet_ready.Ready, fleet_id: []const u8) ?[]const u8 {
    for (entries) |entry| {
        if (std.mem.eql(u8, entry.fleet_id, fleet_id)) return entry.token;
    }
    return null;
}

/// The token currently stored for `fleet_id`, or null when absent.
/// Caller must free.
fn storedToken(h: *TestHarness, fleet_id: []const u8) !?[]u8 {
    var resp = try h.queue.command(&.{ CMD_HGET, queue_consts.ready_index_key, fleet_id });
    defer resp.deinit(h.queue.alloc);
    return switch (resp) {
        .bulk => |v| if (v) |s| try ALLOC.dupe(u8, s) else null,
        else => null,
    };
}

fn isMarked(h: *TestHarness, fleet_id: []const u8) !bool {
    const token = try storedToken(h, fleet_id) orelse return false;
    ALLOC.free(token);
    return true;
}

fn setRequiredTags(conn: *pg.Conn, fleet_id: []const u8, tags: []const []const u8) !void {
    _ = try conn.exec(
        "UPDATE core.fleets SET required_tags = $2::text[] WHERE id = $1::uuid",
        .{ fleet_id, tags },
    );
}

// ── Ingress marks readiness ─────────────────────────────────────────────────

test "integration: an accepted fleet event leaves its fleet in the readiness index" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, FLEET_READY_A, "ready-mark", base.CONFIG_PLAIN);
    try clearWholeIndex(h);

    try std.testing.expect(!try isMarked(h, FLEET_READY_A));
    const event_id = try base.publishEvent(h, FLEET_READY_A);
    defer h.queue.alloc.free(event_id);

    // Marked by the producer itself, not by any handler: all five ingress paths
    // funnel through `redis_fleet.xaddFleetEvent`, so recording it there is what
    // keeps the index correct without touching a single handler.
    try std.testing.expect(try isMarked(h, FLEET_READY_A));
}

test "integration: a token is never reused across marks of the same fleet" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    try clearWholeIndex(h);

    // Mark, capture, clear (deleting the field), then mark again. This is the
    // exact cycle every counter shape gets wrong: a per-fleet count restarts from
    // the beginning once its field is gone, re-minting a token a stale poll may
    // still be holding. A minted identifier cannot repeat here.
    fleet_ready.mark(&h.queue, FLEET_READY_A);
    const first = (try storedToken(h, FLEET_READY_A)) orelse return error.MarkMissing;
    defer ALLOC.free(first);
    try std.testing.expect(id_format.isUuidV7(first));

    fleet_ready.clear(&h.queue, FLEET_READY_A, first);
    try std.testing.expect(!try isMarked(h, FLEET_READY_A));

    common.sleepNanos(TOKEN_MINT_GAP_MS * std.time.ns_per_ms);
    fleet_ready.mark(&h.queue, FLEET_READY_A);
    const second = (try storedToken(h, FLEET_READY_A)) orelse return error.MarkMissing;
    defer ALLOC.free(second);
    try std.testing.expect(id_format.isUuidV7(second));
    try std.testing.expect(!std.mem.eql(u8, first, second));

    // Successive marks also differ without any clear in between.
    common.sleepNanos(TOKEN_MINT_GAP_MS * std.time.ns_per_ms);
    fleet_ready.mark(&h.queue, FLEET_READY_A);
    const third = (try storedToken(h, FLEET_READY_A)) orelse return error.MarkMissing;
    defer ALLOC.free(third);
    try std.testing.expect(!std.mem.eql(u8, second, third));

    // Tokens minted in ascending wall-clock order also SORT ascending — the
    // leading millisecond field of the identifier is big-endian text, so an
    // operator reading the index sees marks in the order they were issued.
    // A readability property only: uniqueness above never rested on it.
    try std.testing.expect(std.mem.order(u8, first, second) == .lt);
    try std.testing.expect(std.mem.order(u8, second, third) == .lt);
}

test "integration: a clear holding a stale token leaves the newer mark intact" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    try clearWholeIndex(h);

    // This is the racing-ingress case, reproduced deterministically at the level
    // where the property actually lives. A poll observes one token, decides the
    // fleet holds nothing, and clears with that same token — but ingress appended
    // and re-marked in between, so the stored value has been replaced. An
    // unconditional delete would erase the mark for that genuinely undelivered
    // event and nothing would rediscover it until a sweep pass.
    fleet_ready.mark(&h.queue, FLEET_READY_A);
    const observed = (try storedToken(h, FLEET_READY_A)) orelse return error.MarkMissing;
    defer ALLOC.free(observed);

    fleet_ready.mark(&h.queue, FLEET_READY_A); // the racing ingress mark
    const advanced = (try storedToken(h, FLEET_READY_A)) orelse return error.MarkMissing;
    defer ALLOC.free(advanced);
    try std.testing.expect(!std.mem.eql(u8, observed, advanced));

    fleet_ready.clear(&h.queue, FLEET_READY_A, observed);

    // Still marked, and still carrying the NEWER token — the clear was a no-op.
    const after = (try storedToken(h, FLEET_READY_A)) orelse return error.ReadinessErased;
    defer ALLOC.free(after);
    try std.testing.expectEqualStrings(advanced, after);

    // And the matching token does clear it, so the guard is not simply broken.
    fleet_ready.clear(&h.queue, FLEET_READY_A, after);
    try std.testing.expect(!try isMarked(h, FLEET_READY_A));
}

test "integration: peek returns each fleet with the token stored for it" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    try clearWholeIndex(h);

    fleet_ready.mark(&h.queue, FLEET_READY_A);
    fleet_ready.mark(&h.queue, FLEET_READY_B);

    const peeked = try fleet_ready.peek(&h.queue, ALLOC, 16);
    defer fleet_ready.freePeeked(ALLOC, peeked);

    // Pairing matters: `clear` compares against the token peek reported, so a
    // decoder that mis-associated ids and tokens would make every clear a no-op
    // and let the index grow without bound. Asserted per-fleet rather than by
    // total count, so the proof does not depend on the index holding nothing else.
    for ([_][]const u8{ FLEET_READY_A, FLEET_READY_B }) |fleet_id| {
        const reported = peekedToken(peeked, fleet_id) orelse return error.FleetNotPeeked;
        const stored = (try storedToken(h, fleet_id)) orelse return error.MarkMissing;
        defer ALLOC.free(stored);
        try std.testing.expectEqualStrings(stored, reported);
    }
    for (peeked) |entry| {
        const stored = (try storedToken(h, entry.fleet_id)) orelse return error.MarkMissing;
        defer ALLOC.free(stored);
        try std.testing.expectEqualStrings(stored, entry.token);
    }
}

test "integration: peek never returns more entries than the bound it was given" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    try clearWholeIndex(h);

    // Marked directly rather than via seeded fleets: this asserts the SERVER-side
    // bound on the read, which is what keeps the per-fleet cost off the client.
    //
    // These synthetic ids go into the SHARED index, so they are removed again
    // before the test returns. Left behind they would sit in every later suite's
    // peek, and since the peek is bounded and randomized they could crowd out the
    // one fleet a sibling test had just marked — a lease failure with no visible
    // cause in the suite that suffered it.
    var synthetic: [SYNTHETIC_FLEETS][id_format.UUID_TEXT_LEN]u8 = undefined;
    for (&synthetic) |*slot| slot.* = try id_format.generateUuidV7();
    defer {
        for (&synthetic) |*slot| {
            var cleanup = h.queue.command(&.{ CMD_HDEL, queue_consts.ready_index_key, slot }) catch continue;
            cleanup.deinit(h.queue.alloc);
        }
    }
    for (&synthetic) |*slot| {
        var resp = try h.queue.command(&.{ CMD_HSET, queue_consts.ready_index_key, slot, slot });
        resp.deinit(h.queue.alloc);
    }

    const peeked = try fleet_ready.peek(&h.queue, ALLOC, PEEK_BOUND);
    defer fleet_ready.freePeeked(ALLOC, peeked);
    try std.testing.expectEqual(@as(usize, PEEK_BOUND), peeked.len);

    try std.testing.expectEqual(@as(u64, SYNTHETIC_FLEETS), try fleet_ready.depth(&h.queue));
}

test "integration: an empty index peeks as no entries" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    try clearWholeIndex(h);

    const peeked = try fleet_ready.peek(&h.queue, ALLOC, 64);
    defer fleet_ready.freePeeked(ALLOC, peeked);
    try std.testing.expectEqual(@as(usize, 0), peeked.len);
    try std.testing.expectEqual(@as(u64, 0), try fleet_ready.depth(&h.queue));
}

// ── The lease path ──────────────────────────────────────────────────────────

test "integration: a poll against an empty readiness index performs zero Postgres round-trips" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    // An active, eligible fleet EXISTS — it simply holds no work. Before this
    // change the poll would have claimed it, probed for a prior lease, read its
    // stream twice and released it. The index being empty is what makes all of
    // that unnecessary.
    try base.seedFleetWithConfig(conn, FLEET_READY_A, "ready-idle", base.CONFIG_PLAIN);
    try clearWholeIndex(h);

    mc.resetLeasePollMetricsForTest();
    try std.testing.expect(!try base.pollLease(h));

    const snap = mc.snapshot();
    try std.testing.expectEqual(@as(u64, 1), snap.lease_polls_total);
    // The invariant, asserted rather than reviewed.
    try std.testing.expectEqual(@as(u64, 0), snap.lease_poll_db_roundtrips_total);
    try std.testing.expectEqual(@as(u64, 0), snap.lease_poll_candidates_scanned_total);
}

test "integration: a poll against a non-empty index does reach Postgres" {
    // The negative of the test above: if the counter read zero in both cases it
    // would be measuring nothing, and the zero-round-trip proof would be vacuous.
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, FLEET_READY_A, "ready-busy", base.CONFIG_PLAIN);
    try clearWholeIndex(h);

    const event_id = try base.publishEvent(h, FLEET_READY_A);
    defer h.queue.alloc.free(event_id);

    mc.resetLeasePollMetricsForTest();
    try std.testing.expect(try base.pollLease(h));

    const snap = mc.snapshot();
    try std.testing.expectEqual(@as(u64, 1), snap.lease_polls_total);
    try std.testing.expect(snap.lease_poll_db_roundtrips_total > 0);
    try std.testing.expect(snap.lease_poll_candidates_scanned_total > 0);
}

test "integration: a ready fleet requiring a tag the runner lacks is never leased" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, FLEET_TAGGED, "ready-tagged", base.CONFIG_PLAIN);
    // Readiness NARROWS the candidate set; it must never widen eligibility. The
    // label gate lives in the candidate query and this proves the membership
    // restriction did not displace it.
    try setRequiredTags(conn, FLEET_TAGGED, &.{"gpu"});
    try clearWholeIndex(h);

    const event_id = try base.publishEvent(h, FLEET_TAGGED);
    defer h.queue.alloc.free(event_id);
    try std.testing.expect(try isMarked(h, FLEET_TAGGED));

    // Ready, holding work, and still not leasable by this untagged runner.
    try std.testing.expect(!try base.pollLease(h));

    // Readiness is NOT cleared for it either: the fleet is genuinely non-empty,
    // so clearing would strand its event until a sweep.
    try std.testing.expect(try isMarked(h, FLEET_TAGGED));

    // Dropping the requirement makes the same fleet leasable, which proves the
    // refusal was the tag and not some unrelated ineligibility.
    try setRequiredTags(conn, FLEET_TAGGED, &.{});
    try std.testing.expect(try base.pollLease(h));
}

/// `XGROUP DESTROY` on a fleet's consumer group, leaving the stream and its
/// entries intact. Reproduces the states the poll path must survive without any
/// in-process claim to consult: a group deleted out of band, a Redis restart
/// without persistence, a failover to an empty replica, or a fleet whose stream
/// predates the create-on-write path.
fn destroyGroup(h: *TestHarness, fleet_id: []const u8) !void {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = try queue_consts.fleetStreamKey(&key_buf, fleet_id);
    var resp = try h.queue.command(&.{
        CMD_XGROUP, CMD_DESTROY, stream_key, queue_consts.fleet_consumer_group,
    });
    defer resp.deinit(h.queue.alloc);
}

test "integration: a consumer group deleted out of band is repaired by the next poll" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, FLEET_REPAIR, "ready-repair", base.CONFIG_PLAIN);
    defer redis_fleet.purgeFleetRedisState(&h.queue, FLEET_REPAIR) catch {};
    try clearWholeIndex(h);

    // Stream, group, and one undelivered event.
    const event_id = try base.publishEvent(h, FLEET_REPAIR);
    defer h.queue.alloc.free(event_id);

    // Take the group away, leaving the event stranded in a stream nothing can
    // read. Nothing in the process knows this happened — which is the point: the
    // poll path is TOLD by Redis rather than predicting it.
    try destroyGroup(h, FLEET_REPAIR);

    // The next poll hits NOGROUP, recreates the group at the stream's END, reads
    // again, and answers "no work" from a read that genuinely succeeded. The
    // stranded event is NOT delivered — that is the deliberate cost: a repair at
    // the beginning would also re-deliver every already-executed entry still
    // resident in the stream, with real provider spend. Skipped work is
    // re-submittable; re-executed work cannot be un-spent.
    //
    // Reporting a fault here instead would trip `PollCost.noteRedisFailure`,
    // whose accumulation ends the candidate loop early — one fleet with a
    // missing group would starve every fleet behind it in the same poll. See
    // `redis_fleet.readGroup`.
    const before_repair = try base.xgroupCreateCalls(h);
    try std.testing.expect(before_repair > 0); // vacuous-parse guard
    try std.testing.expect(!try base.pollLease(h));

    // Exactly ONE create: the repair fired once, on the poll that saw NOGROUP.
    try std.testing.expectEqual(before_repair + 1, try base.xgroupCreateCalls(h));

    // An event published after the repair flows through the recreated group —
    // the fleet is live again, not wedged on a group nothing can read.
    const post_repair_event = try base.publishEvent(h, FLEET_REPAIR);
    defer h.queue.alloc.free(post_repair_event);
    try std.testing.expect(try base.pollLease(h));
}

test "integration: repeated leases against one fleet create its consumer group once" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, FLEET_MEMO, "ready-memo", base.CONFIG_PLAIN);
    defer redis_fleet.purgeFleetRedisState(&h.queue, FLEET_MEMO) catch {};
    // Only this fleet may be a candidate, or another fleet's ensure would be
    // counted against this one's budget.
    try clearWholeIndex(h);

    // A fleet this process has never touched, so the memo cannot already hold
    // it and the one real create below is genuinely the first.
    const event_id = try base.publishEvent(h, FLEET_MEMO);
    defer h.queue.alloc.free(event_id);

    const after_first = try base.xgroupCreateCalls(h);
    // Guards the vacuous pass: if the INFO parse ever returned 0 for both reads
    // the equality below would hold while measuring nothing at all.
    try std.testing.expect(after_first > 0);
    var i: usize = 0;
    while (i < LEASE_POLLS) : (i += 1) _ = try base.pollLease(h);

    // Zero further creates across ten polls, because the group is created on the
    // fleet's WRITE path and the poll path never asserts it. This once cost one
    // Redis round-trip per candidate per poll — using the BUSYGROUP error reply as
    // its steady state — which a per-process memo then existed to hide; both the
    // round-trip and the memo are gone, and this is what proves the first one is.
    try std.testing.expectEqual(after_first, try base.xgroupCreateCalls(h));
}

test "integration: readiness is cleared once a claim-won poll finds nothing deliverable" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, FLEET_READY_B, "ready-drain", base.CONFIG_PLAIN);
    try clearWholeIndex(h);

    // Mark a fleet whose stream holds nothing. The poll wins the claim, both
    // reads return null, and only then is the mark removed.
    fleet_ready.mark(&h.queue, FLEET_READY_B);
    try std.testing.expect(try isMarked(h, FLEET_READY_B));

    try std.testing.expect(!try base.pollLease(h));
    try std.testing.expect(!try isMarked(h, FLEET_READY_B));

    // Guards the regression the clear site was moved to avoid: a
    // stream-emptiness condition would essentially never fire, because ingress
    // trims at MAXLEN rather than deleting, so delivered entries persist forever
    // and every fleet that ever received an event would stay in the index.
}
