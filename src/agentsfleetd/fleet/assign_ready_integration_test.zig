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
const pg = @import("pg");
const base = @import("event_lifecycle_integration_test.zig");
const fleet_ready = @import("../queue/fleet_ready.zig");
const queue_consts = @import("../queue/constants.zig");
const id_format = @import("../types/id_format.zig");
const mc = @import("../observability/metrics_counters.zig");
const TestHarness = @import("../http/test_harness.zig").TestHarness;

const ALLOC = std.testing.allocator;

/// Fleets owned by this suite. Distinct from the sibling suites' ids so a shared
/// test database never crosses them.
const FLEET_READY_A = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7e01";
const FLEET_READY_B = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7e02";
const FLEET_TAGGED = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7e03";

const CMD_DEL = "DEL";
const CMD_HGET = "HGET";
const CMD_HSET = "HSET";

/// Empty the shared readiness index so a test starts from a known state. Sibling
/// suites publish events (which mark), so this must run inside each test rather
/// than once per file.
fn clearIndex(h: *TestHarness) !void {
    var resp = try h.queue.command(&.{ CMD_DEL, queue_consts.ready_index_key });
    resp.deinit(h.queue.alloc);
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
    try base.seedFleetWithConfig(conn, FLEET_READY_A, "ready-mark", base.CONFIG_PLAIN, "5");
    try clearIndex(h);

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
    try clearIndex(h);

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

    fleet_ready.mark(&h.queue, FLEET_READY_A);
    const second = (try storedToken(h, FLEET_READY_A)) orelse return error.MarkMissing;
    defer ALLOC.free(second);
    try std.testing.expect(id_format.isUuidV7(second));
    try std.testing.expect(!std.mem.eql(u8, first, second));

    // Successive marks also differ without any clear in between.
    fleet_ready.mark(&h.queue, FLEET_READY_A);
    const third = (try storedToken(h, FLEET_READY_A)) orelse return error.MarkMissing;
    defer ALLOC.free(third);
    try std.testing.expect(!std.mem.eql(u8, second, third));
}

test "integration: a clear holding a stale token leaves the newer mark intact" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    try clearIndex(h);

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
    try clearIndex(h);

    fleet_ready.mark(&h.queue, FLEET_READY_A);
    fleet_ready.mark(&h.queue, FLEET_READY_B);

    const peeked = try fleet_ready.peek(&h.queue, ALLOC, 16);
    defer fleet_ready.freePeeked(ALLOC, peeked);
    try std.testing.expectEqual(@as(usize, 2), peeked.len);

    // Pairing matters: `clear` compares against the token peek reported, so a
    // decoder that mis-associated ids and tokens would make every clear a no-op
    // and let the index grow without bound.
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
    try clearIndex(h);

    // Marked directly rather than via seeded fleets: this asserts the SERVER-side
    // bound on the read, which is what keeps the per-fleet cost off the client.
    var i: usize = 0;
    while (i < 20) : (i += 1) {
        const id = try id_format.generateUuidV7();
        var resp = try h.queue.command(&.{ CMD_HSET, queue_consts.ready_index_key, &id, &id });
        resp.deinit(h.queue.alloc);
    }

    const peeked = try fleet_ready.peek(&h.queue, ALLOC, 5);
    defer fleet_ready.freePeeked(ALLOC, peeked);
    try std.testing.expectEqual(@as(usize, 5), peeked.len);

    try std.testing.expectEqual(@as(u64, 20), try fleet_ready.depth(&h.queue));
}

test "integration: an empty index peeks as no entries" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    try clearIndex(h);

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
    try base.seedFleetWithConfig(conn, FLEET_READY_A, "ready-idle", base.CONFIG_PLAIN, "5");
    try clearIndex(h);

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
    try base.seedFleetWithConfig(conn, FLEET_READY_A, "ready-busy", base.CONFIG_PLAIN, "5");
    try clearIndex(h);

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
    try base.seedFleetWithConfig(conn, FLEET_TAGGED, "ready-tagged", base.CONFIG_PLAIN, "5");
    // Readiness NARROWS the candidate set; it must never widen eligibility. The
    // label gate lives in the candidate query and this proves the membership
    // restriction did not displace it.
    try setRequiredTags(conn, FLEET_TAGGED, &.{"gpu"});
    try clearIndex(h);

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

test "integration: readiness is cleared once a claim-won poll finds nothing deliverable" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedFleetWithConfig(conn, FLEET_READY_B, "ready-drain", base.CONFIG_PLAIN, "5");
    try clearIndex(h);

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
