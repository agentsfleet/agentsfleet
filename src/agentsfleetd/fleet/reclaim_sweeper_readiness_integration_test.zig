//! Integration proofs for the readiness backstop inside the reclaim sweeper
//! (M141 §3, Dimensions 3.2–3.5) and for the shared-index depth sample (5.2).
//!
//! The readiness index is a hint and the streams are the system of record. That
//! sentence is only true if something re-derives readiness FROM the streams —
//! and that something is `reclaim_sweeper.sweepOnce`. These are the proofs that
//! losing a mark costs latency rather than the event: a lost mark on an
//! undelivered entry, a strand sitting in another replica's pending list, and a
//! fleet parked beyond the per-pass batch bound all recover here.
//!
//! Rides `event_lifecycle_integration_test.zig`'s harness (real schema, real
//! Redis) so every fixture is production-shaped per RULE ITF. Self-skips when
//! the test datastores are absent.

const std = @import("std");
const pg = @import("pg");
const shared = @import("common");
const protocol = @import("contract").protocol;
const base = @import("event_lifecycle_integration_test.zig");
const reclaim_sweeper = @import("reclaim_sweeper.zig");
const fleet_config = @import("../fleet_runtime/config.zig");
const fleet_ready = @import("../queue/fleet_ready.zig");
const queue_consts = @import("../queue/constants.zig");
const queue_redis = @import("../queue/redis.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const redis_protocol = @import("../queue/redis_protocol.zig");
const mc = @import("../observability/metrics_counters.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const TestHarness = @import("../http/test_harness.zig").TestHarness;

const ALLOC = std.testing.allocator;

// Fleets owned by this suite. The `…7f…` node suffix belongs to no sibling
// fleet suite, so a shared test database never crosses them.
const FLEET_PENDING = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7f01";
const FLEET_UNDELIVERED = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7f02";
const FLEET_STRAY = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7f03";
// Session-id suffixes `base.seedFleetWithConfig` appends; 1–b are taken by the
// harness module's own fixtures.
const SESSION_PENDING = "c";
const SESSION_UNDELIVERED = "d";
const SESSION_STRAY = "e";

/// The scan proof needs strictly more active fleets than one pass reaches, so a
/// sweep that never advanced its cursor could not possibly reach them all.
const SCAN_FLEET_COUNT: usize = @as(usize, @intCast(reclaim_sweeper.SWEEP_BATCH_LIMIT)) + 10;
/// 33 characters; the remaining 3 are this fleet's index in hex. Version nibble
/// 7 and variant 8 keep every generated id a canonical UUIDv7, which the
/// `core.fleets` id CHECK requires.
const SCAN_ID_PREFIX = "0195c9da-1e2a-7f13-8abc-2b3e1e0d8";
const SCAN_ID_LEN = 36;
/// Extra passes allowed beyond the arithmetic minimum, covering the pass that
/// rewinds the cursor at the end of a cycle.
const SCAN_PASS_SLACK: usize = 2;

/// Synthetic index fields for the depth proof. They name no fleet row, so the
/// sweep can neither re-mark nor clear them — the gauge counts them only if it
/// truly reads the shared hash.
const FOREIGN_MARKS = [_][]const u8{
    "0195c9da-1e2a-7f13-8abc-2b3e1e0d9f01",
    "0195c9da-1e2a-7f13-8abc-2b3e1e0d9f02",
    "0195c9da-1e2a-7f13-8abc-2b3e1e0d9f03",
};

const CMD_DEL = "DEL";
const CMD_HGET = "HGET";
const CMD_HSET = "HSET";
const CMD_HKEYS = "HKEYS";
const FOREIGN_MARK_TOKEN = "0195c9da-1e2a-7f13-8abc-2b3e1e0dff01";

// ── Index helpers ───────────────────────────────────────────────────────────

fn isMarked(h: *TestHarness, fleet_id: []const u8) !bool {
    var resp = try h.queue.command(&.{ CMD_HGET, queue_consts.ready_index_key, fleet_id });
    defer resp.deinit(h.queue.alloc);
    return switch (resp) {
        .bulk => |v| v != null,
        else => false,
    };
}

/// Start from an index this test owns. `fleet:ready` is ONE key shared by every
/// suite in the binary, and the assertions below are about what a sweep PUT
/// there — a sibling's leftover mark would answer the question for us. Safe
/// because tests run sequentially and every suite marks its own fleet
/// immediately before it polls.
fn clearWholeIndex(h: *TestHarness) !void {
    var resp = try h.queue.command(&.{ CMD_DEL, queue_consts.ready_index_key });
    resp.deinit(h.queue.alloc);
}

/// How many of the scan fleets currently hold a field, in one command rather
/// than one `HGET` per fleet.
fn scanMarkedCount(h: *TestHarness) !usize {
    var resp = try h.queue.command(&.{ CMD_HKEYS, queue_consts.ready_index_key });
    defer resp.deinit(h.queue.alloc);
    const fields = resp.array orelse return 0;
    var marked: usize = 0;
    for (fields) |field| {
        const name = redis_protocol.valueAsString(field) orelse continue;
        if (std.mem.startsWith(u8, name, SCAN_ID_PREFIX)) marked += 1;
    }
    return marked;
}

fn indexDepth(h: *TestHarness) !u64 {
    return fleet_ready.depth(&h.queue);
}

// ── Postgres helpers ────────────────────────────────────────────────────────

/// The event id on `fleet_id`'s live lease, or null when it holds none. Scoping
/// every lease assertion to one fleet keeps this suite honest about WHICH fleet
/// a poll served, which a bare "did any lease issue" check cannot.
fn activeLeaseEventId(conn: *pg.Conn, fleet_id: []const u8) !?[]u8 {
    var q = PgQuery.from(try conn.query(
        \\SELECT event_id FROM fleet.runner_leases
        \\WHERE fleet_id = $1::uuid AND status = $2
        \\ORDER BY fencing_token DESC LIMIT 1
    , .{ fleet_id, protocol.RUNNER_LEASE_STATUS_ACTIVE }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    return try ALLOC.dupe(u8, try row.get([]const u8, 0));
}

fn expectLeasedEvent(conn: *pg.Conn, fleet_id: []const u8, event_id: []const u8) !void {
    const leased = try activeLeaseEventId(conn, fleet_id) orelse return error.NoActiveLease;
    defer ALLOC.free(leased);
    try std.testing.expectEqualStrings(event_id, leased);
}

/// Drop the holder's lease row and expire its claim — the state a runner leaves
/// behind when it dies mid-work, and the one a re-delivery has to recover from.
fn abandonHolder(conn: *pg.Conn, fleet_id: []const u8) !void {
    _ = try conn.exec("DELETE FROM fleet.runner_leases WHERE fleet_id = $1::uuid", .{fleet_id});
    _ = try conn.exec("UPDATE fleet.runner_affinity SET leased_until = 0 WHERE fleet_id = $1::uuid", .{fleet_id});
}

fn activeFleetCount(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT count(*) FROM core.fleets WHERE status = $1",
        .{fleet_config.FleetStatus.active.toSlice()},
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.CountRowMissing;
    return row.get(i64, 0);
}

// ── Scan-fleet fixtures ─────────────────────────────────────────────────────

fn scanFleetId(buf: *[SCAN_ID_LEN]u8, index: usize) ![]const u8 {
    return std.fmt.bufPrint(buf, "{s}{x:0>3}", .{ SCAN_ID_PREFIX, index });
}

/// Insert every scan fleet in one statement. Sessions are omitted deliberately:
/// these fleets exist to be WALKED by the sweep, never leased, and seeding 110
/// sessions would buy nothing but wall-clock.
fn seedScanFleets(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\SELECT ($2 || lpad(to_hex(g), 3, '0'))::uuid, $1::uuid, 'sweep-scan-' || g,
        \\       '', '{}', $4, 0, 0
        \\FROM generate_series(0, $3::int - 1) AS g
        \\ON CONFLICT DO NOTHING
    , .{
        base.WORKSPACE_ID,
        SCAN_ID_PREFIX,
        @as(i32, @intCast(SCAN_FLEET_COUNT)),
        fleet_config.FleetStatus.active.toSlice(),
    });
}

/// Give every scan fleet a stream carrying one undelivered entry, so the sweep's
/// probe answers true for each and a re-mark is the only reason a field appears.
fn publishToScanFleets(h: *TestHarness) !void {
    var id_buf: [SCAN_ID_LEN]u8 = undefined;
    var i: usize = 0;
    while (i < SCAN_FLEET_COUNT) : (i += 1) {
        const fleet_id = try scanFleetId(&id_buf, i);
        const event_id = try base.publishEvent(h, fleet_id);
        h.queue.alloc.free(event_id);
    }
}

fn forgetScanFleets(h: *TestHarness) void {
    var id_buf: [SCAN_ID_LEN]u8 = undefined;
    var i: usize = 0;
    while (i < SCAN_FLEET_COUNT) : (i += 1) {
        const fleet_id = scanFleetId(&id_buf, i) catch continue;
        redis_fleet.purgeFleetRedisState(&h.queue, fleet_id) catch |err| {
            std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
        };
    }
}

// ── Dimension 3.2 ───────────────────────────────────────────────────────────

test "integration: a fleet holding a pending entry stays ready and the entry is re-delivered" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer redis_fleet.purgeFleetRedisState(&h.queue, FLEET_PENDING) catch {};
    try base.seedFleetWithConfig(conn, FLEET_PENDING, "sweep-pending", base.CONFIG_PLAIN, SESSION_PENDING);
    try clearWholeIndex(h);

    const event_id = try base.publishEvent(h, FLEET_PENDING);
    defer h.queue.alloc.free(event_id);
    try std.testing.expect(try isMarked(h, FLEET_PENDING));

    // A poll that FINDS work must not clear readiness: the clear is reachable
    // only where both the own-PEL read and the undelivered read returned null.
    try std.testing.expect(try base.pollLease(h));
    try expectLeasedEvent(conn, FLEET_PENDING, event_id);
    try std.testing.expectEqual(@as(i64, 1), try base.pendingCount(h, FLEET_PENDING));
    try std.testing.expect(try isMarked(h, FLEET_PENDING));

    // The holder dies with the entry still in the stable consumer's PEL. The
    // next poll re-delivers THAT entry — own-PEL-first — rather than skipping to
    // a fresh read, and readiness survives the round trip.
    try abandonHolder(conn, FLEET_PENDING);
    try std.testing.expect(try base.pollLease(h));
    try expectLeasedEvent(conn, FLEET_PENDING, event_id);
    try std.testing.expectEqual(@as(i64, 1), try base.pendingCount(h, FLEET_PENDING));
    try std.testing.expect(try isMarked(h, FLEET_PENDING));
}

// ── Dimension 3.3 ───────────────────────────────────────────────────────────

test "integration: the sweeper recovers an undelivered entry that sits in no pending list" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer redis_fleet.purgeFleetRedisState(&h.queue, FLEET_UNDELIVERED) catch {};
    try base.seedFleetWithConfig(conn, FLEET_UNDELIVERED, "sweep-undelivered", base.CONFIG_PLAIN, SESSION_UNDELIVERED);

    // A successful append whose readiness mark then failed: the entry is in the
    // stream and in NOBODY's pending list, so XAUTOCLAIM can never see it. The
    // consumer group exists, which is what stops the probe taking its
    // no-group-yet shortcut — this exercises the last-delivered comparison.
    const event_id = try base.publishEvent(h, FLEET_UNDELIVERED);
    defer h.queue.alloc.free(event_id);
    fleet_ready.forceClear(&h.queue, FLEET_UNDELIVERED);
    try std.testing.expect(!try isMarked(h, FLEET_UNDELIVERED));
    try std.testing.expectEqual(@as(i64, 0), try base.pendingCount(h, FLEET_UNDELIVERED));

    // Stranded: discovery goes through the index, and the index no longer knows.
    _ = try base.pollLease(h);
    try std.testing.expect(try activeLeaseEventId(conn, FLEET_UNDELIVERED) == null);

    var cursor = reclaim_sweeper.Cursor{};
    const stats = try reclaim_sweeper.sweepOnce(h.pool, &h.queue, ALLOC, &cursor);
    try std.testing.expectEqual(@as(i64, 0), stats.reclaimed_entries);
    try std.testing.expect(stats.remarked_fleets >= 1);
    try std.testing.expect(try isMarked(h, FLEET_UNDELIVERED));

    try std.testing.expect(try base.pollLease(h));
    try expectLeasedEvent(conn, FLEET_UNDELIVERED, event_id);
}

// ── Dimension 3.4 ───────────────────────────────────────────────────────────

test "integration: a stray the sweeper reclaims leaves its fleet ready and leasable" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer redis_fleet.purgeFleetRedisState(&h.queue, FLEET_STRAY) catch {};
    try base.seedFleetWithConfig(conn, FLEET_STRAY, "sweep-stray", base.CONFIG_PLAIN, SESSION_STRAY);

    // Another replica took delivery and never acked, then went away. Its
    // readiness mark went with it — the cross-replica strand.
    const event_id = try base.publishEvent(h, FLEET_STRAY);
    defer h.queue.alloc.free(event_id);
    try base.deliverToDeadConsumer(h, FLEET_STRAY);
    try base.forceIdle(h, FLEET_STRAY, event_id, base.FORCED_IDLE_MS);
    fleet_ready.forceClear(&h.queue, FLEET_STRAY);
    try std.testing.expect(!try isMarked(h, FLEET_STRAY));

    var cursor = reclaim_sweeper.Cursor{};
    const stats = try reclaim_sweeper.sweepOnce(h.pool, &h.queue, ALLOC, &cursor);
    try std.testing.expect(stats.reclaimed_entries >= 1);
    try std.testing.expect(try isMarked(h, FLEET_STRAY));

    // Leasable with no new ingress: re-marking on reclaim is what closes the
    // hole where a reclaimed stray re-enters "on the next poll" that never came.
    try std.testing.expect(try base.pollLease(h));
    try expectLeasedEvent(conn, FLEET_STRAY, event_id);
}

// ── Dimension 3.5 ───────────────────────────────────────────────────────────

test "integration: the sweeper's scan advances past its batch bound across passes" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer forgetScanFleets(h);
    try seedScanFleets(conn);
    try publishToScanFleets(h);
    try clearWholeIndex(h);

    var cursor = reclaim_sweeper.Cursor{};
    const first = try reclaim_sweeper.sweepOnce(h.pool, &h.queue, ALLOC, &cursor);
    try std.testing.expectEqual(reclaim_sweeper.SWEEP_BATCH_LIMIT, first.scanned_agents);
    // One pass CANNOT reach them all. Without the keyset cursor the remainder is
    // not merely reached late — it is never reached at all, which is the defect
    // this dimension exists to pin.
    const after_first = try scanMarkedCount(h);
    try std.testing.expect(after_first < SCAN_FLEET_COUNT);

    // Every scan fleet is reached within the published recovery bound:
    // ceil(active_fleets / SWEEP_BATCH_LIMIT) passes, plus the pass that rewinds
    // the cursor at the end of a cycle.
    const active = try activeFleetCount(conn);
    const batch: i64 = reclaim_sweeper.SWEEP_BATCH_LIMIT;
    const max_passes: usize = @as(usize, @intCast(@divTrunc(active + batch - 1, batch))) + SCAN_PASS_SLACK;
    var pass: usize = 1;
    while (pass < max_passes and try scanMarkedCount(h) < SCAN_FLEET_COUNT) : (pass += 1) {
        _ = try reclaim_sweeper.sweepOnce(h.pool, &h.queue, ALLOC, &cursor);
    }
    try std.testing.expectEqual(SCAN_FLEET_COUNT, try scanMarkedCount(h));
}

// ── Dimension 5.2 ───────────────────────────────────────────────────────────

test "integration: readiness depth samples the shared index rather than this process's marks" {
    var env = base.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    try clearWholeIndex(h);

    // A SECOND client, standing in for another replica: nothing in this process
    // called `fleet_ready.mark` for these fields, so a gauge built from local
    // mark/clear bookkeeping could not possibly count them.
    var env_map = try shared.env.testLiveSnapshot(ALLOC);
    defer env_map.deinit();
    var other = try queue_redis.Client.connectFromEnv(shared.globalIo(), &env_map, ALLOC, .api);
    defer other.deinit();
    for (FOREIGN_MARKS) |fleet_id| {
        var resp = try other.command(&.{ CMD_HSET, queue_consts.ready_index_key, fleet_id, FOREIGN_MARK_TOKEN });
        resp.deinit(other.alloc);
    }
    defer for (FOREIGN_MARKS) |fleet_id| fleet_ready.forceClear(&h.queue, fleet_id);

    // The pass may also re-mark fleets a sibling fixture left deliverable, so the
    // expected depth is the foreign marks plus whatever this pass itself marked
    // — an exact figure either way, because the index started empty.
    var cursor = reclaim_sweeper.Cursor{};
    const first = try reclaim_sweeper.sweepOnce(h.pool, &h.queue, ALLOC, &cursor);
    const expected: u64 = FOREIGN_MARKS.len + @as(u64, @intCast(first.remarked_fleets));
    try std.testing.expectEqual(expected, mc.snapshot().fleet_ready_depth);
    try std.testing.expectEqual(expected, try indexDepth(h));

    // Sampling again re-reads the same hash; it never accumulates.
    cursor = reclaim_sweeper.Cursor{};
    _ = try reclaim_sweeper.sweepOnce(h.pool, &h.queue, ALLOC, &cursor);
    try std.testing.expectEqual(expected, mc.snapshot().fleet_ready_depth);
}
