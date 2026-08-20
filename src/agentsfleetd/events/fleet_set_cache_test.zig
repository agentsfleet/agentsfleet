//! FleetSetCache tests.
//!
//! The map/refcount/version mechanics are pure — they need no database, because
//! the whole point of the type is that the DB query happens at most once per
//! workspace per cadence and every other read is served from memory. The
//! enumeration itself (the RLS-scoped `core.fleets` read) is exercised by the
//! workspace-stream integration suite against the real schema.
//!
//! The live-Postgres tests below prove the property this type exists for: V
//! viewers of one workspace cost ONE enumeration, not V.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const clock = common.clock;
const pg = @import("pg");
const FleetSetCache = @import("fleet_set_cache.zig");
const common_authz = @import("../http/handlers/common_authz.zig");
const base = @import("../db/test_fixtures.zig");

// Pure-mechanics tests use arbitrary ids (no DB). The live-Postgres tests below
// use a DEDICATED workspace + tenant this file owns exclusively, so the fleet
// set is deterministic under the parallel runner (the shared fixture workspace
// has sibling suites seeding fleets into it concurrently).
const WS_A = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d0001";
const WS_B = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d0002";

/// A tick that happens "now" — the cache compares against REFRESH_INTERVAL_MS.
fn now() i64 {
    return clock.nowMillis();
}

// ── Refcount + eviction (pure) ──────────────────────────────────────────────

test "cache: an idle workspace holds nothing — the last release evicts" {
    var cache = FleetSetCache.init(testing.allocator, common.globalIo());
    defer cache.deinit();

    try cache.retain(WS_A);
    try cache.retain(WS_A); // a second viewer of the same workspace
    try cache.retain(WS_B);

    cache.release(WS_A);
    // one viewer left on A — still mapped, still serving
    try testing.expectEqual(@as(u64, 0), cache.version(WS_A));
    cache.release(WS_A);
    cache.release(WS_B);
    // released down to zero: nothing retained, so a quiet instance pays nothing
    try testing.expect(try cache.snapshot(WS_A) == null);
}

test "cache: release of an unknown workspace is a no-op, never an underflow" {
    var cache = FleetSetCache.init(testing.allocator, common.globalIo());
    defer cache.deinit();
    cache.release(WS_A);
    cache.release(WS_A);
}

test "cache: version 0 means never-enumerated, so a viewer's first tick always reads" {
    var cache = FleetSetCache.init(testing.allocator, common.globalIo());
    defer cache.deinit();
    try cache.retain(WS_A);
    defer cache.release(WS_A);

    // No viewer can hold version 0, so the first comparison always differs and
    // the first tick always fetches the set.
    try testing.expectEqual(@as(u64, 0), cache.version(WS_A));
    try testing.expect(try cache.snapshot(WS_A) == null);
}

test "cache: retain unwinds cleanly under allocation failure" {
    try std.testing.checkAllAllocationFailures(testing.allocator, retainReleaseRoundTrip, .{});
}

fn retainReleaseRoundTrip(alloc: std.mem.Allocator) !void {
    var cache = FleetSetCache.init(alloc, common.globalIo());
    defer cache.deinit();
    try cache.retain(WS_A);
    errdefer cache.release(WS_A);
    try cache.retain(WS_A); // found_existing path — spares must go back
    cache.release(WS_A);
    cache.release(WS_A);
}

// ── The shared-enumeration property (live Postgres) ─────────────────────────

const FLEET_ONE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ccc01";
const FLEET_TWO = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ccc02";
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";

/// Viewers of one workspace, each running its refresh tick — the shape the
/// stream threads have.
const VIEWERS: usize = 8;

const TestDb = struct {
    pool: *pg.Pool,
    conn: *pg.Conn,

    fn open() !TestDb {
        const opened = (try common_authz.openHandlerTestConn(testing.allocator)) orelse
            return error.SkipZigTest;
        return .{ .pool = opened.pool, .conn = opened.conn };
    }

    fn close(self: TestDb) void {
        self.pool.release(self.conn);
        self.pool.deinit();
    }
};

/// Seeds through the shared fixtures rather than inline SQL. The inline form
/// addressed `core.tenants(tenant_id)` and `core.workspaces(workspace_id)` —
/// both now `id` — and omitted the `tenant_id` that `core.fleets` requires, so
/// all three statements described a shape the database no longer has.
fn seed(conn: *pg.Conn) !void {
    try base.seedTenantById(conn, TENANT_ID, "FleetSetCacheTest");
    try base.seedWorkspaceWithTenant(conn, WS_A, TENANT_ID);
    inline for (.{ FLEET_ONE, FLEET_TWO }, .{ "cache-one", "cache-two" }) |zid, name| {
        try base.seedFleet(conn, zid, WS_A, name, "{\"name\":\"zz\"}", "---\nname: zz\n---\ntest");
    }
}

/// Best-effort teardown delete; a failure is logged, not swallowed silently
/// (a bare empty catch trips zlint's suppressed-errors).
const CLEANUP_IGNORED = "cleanup ignored: {s}";

fn cleanup(conn: *pg.Conn) void {
    inline for (.{ FLEET_ONE, FLEET_TWO }) |zid| {
        _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{zid}) catch |err| std.log.warn(CLEANUP_IGNORED, .{@errorName(err)});
    }
    // The dedicated workspace is this file's own row — remove it so the suite
    // leaves zero rows behind. The tenant is SHARED with sibling suites
    // running in parallel and must survive.
    _ = conn.exec("DELETE FROM core.workspaces WHERE id = $1", .{WS_A}) catch |err| std.log.warn(CLEANUP_IGNORED, .{@errorName(err)});
}

test "integration: V viewers of one workspace cost ONE enumeration, not V" {
    // The whole reason this type exists. The workspace stream deleted the
    // wall's per-viewer CONNECTION cost; it must not reintroduce a per-viewer
    // QUERY cost.
    const db = TestDb.open() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer db.close();
    try seed(db.conn);
    defer cleanup(db.conn);

    var cache = FleetSetCache.init(testing.allocator, common.globalIo());
    defer cache.deinit();

    var v: usize = 0;
    while (v < VIEWERS) : (v += 1) try cache.retain(WS_A);
    defer {
        var r: usize = 0;
        while (r < VIEWERS) : (r += 1) cache.release(WS_A);
    }

    // Every viewer runs its tick, back to back, inside one refresh window.
    var tick: usize = 0;
    while (tick < VIEWERS) : (tick += 1) {
        cache.refreshIfStale(db.conn, WS_A, now());
    }

    // ONE query served all of them: the first tick enumerated, the rest found
    // the set fresh and did nothing.
    try testing.expectEqual(@as(u64, 1), cache.enumerations.load(.monotonic));

    const snap = (try cache.snapshot(WS_A)).?;
    defer snap.deinit(testing.allocator);
    try testing.expectEqual(@as(usize, 2), snap.fleet_ids.len);
    try testing.expectEqual(@as(u64, 1), snap.version);
}

test "integration: an unchanged set does not bump the version — a steady tick is a no-op" {
    const db = TestDb.open() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer db.close();
    try seed(db.conn);
    defer cleanup(db.conn);

    var cache = FleetSetCache.init(testing.allocator, common.globalIo());
    defer cache.deinit();
    try cache.retain(WS_A);
    defer cache.release(WS_A);

    cache.refreshIfStale(db.conn, WS_A, now());
    const first = cache.version(WS_A);
    try testing.expectEqual(@as(u64, 1), first);

    // Force the staleness check by moving the clock past the window: the query
    // re-runs, but the SET is identical, so the version must NOT move — every
    // viewer's next tick stays a version compare and nothing else.
    cache.refreshIfStale(db.conn, WS_A, now() + FleetSetCache.REFRESH_INTERVAL_MS + 1);
    try testing.expectEqual(@as(u64, 2), cache.enumerations.load(.monotonic));
    try testing.expectEqual(first, cache.version(WS_A));
}

test "integration: a successful empty enumeration initializes the cache version" {
    const db = TestDb.open() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer db.close();
    const ts = clock.nowMillis();
    _ = try db.conn.exec(
        \\INSERT INTO core.tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'FleetSetCacheTest', $2, $2) ON CONFLICT (id) DO NOTHING
    , .{ TENANT_ID, ts });
    _ = try db.conn.exec(
        \\INSERT INTO core.workspaces (id, tenant_id, created_at)
        \\VALUES ($1::uuid, $2, $3) ON CONFLICT (id) DO NOTHING
    , .{ WS_B, TENANT_ID, ts });
    defer _ = db.conn.exec("DELETE FROM core.workspaces WHERE id = $1", .{WS_B}) catch |err| std.log.warn(CLEANUP_IGNORED, .{@errorName(err)});

    var cache = FleetSetCache.init(testing.allocator, common.globalIo());
    defer cache.deinit();
    try cache.retain(WS_B);
    defer cache.release(WS_B);

    cache.refreshIfStale(db.conn, WS_B, now());
    try testing.expectEqual(@as(u64, 1), cache.version(WS_B));
    const snap = (try cache.snapshot(WS_B)).?;
    defer snap.deinit(testing.allocator);
    try testing.expectEqual(@as(usize, 0), snap.fleet_ids.len);
}

test "integration: a fleet appearing bumps the version exactly once" {
    const db = TestDb.open() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer db.close();
    try seed(db.conn);
    defer cleanup(db.conn);

    var cache = FleetSetCache.init(testing.allocator, common.globalIo());
    defer cache.deinit();
    try cache.retain(WS_A);
    defer cache.release(WS_A);

    cache.refreshIfStale(db.conn, WS_A, now());
    const before = cache.version(WS_A);

    _ = try db.conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{FLEET_TWO});
    cache.refreshIfStale(db.conn, WS_A, now() + FleetSetCache.REFRESH_INTERVAL_MS + 1);

    try testing.expectEqual(before + 1, cache.version(WS_A));
    const snap = (try cache.snapshot(WS_A)).?;
    defer snap.deinit(testing.allocator);
    try testing.expectEqual(@as(usize, 1), snap.fleet_ids.len);
    try testing.expectEqualStrings(FLEET_ONE, snap.fleet_ids[0]);
}

test "integration: enumerate frees every id when an allocation fails mid-refresh" {
    // refreshIfStale swallows refresh errors internally (a failed refresh is a
    // retry-next-tick event), so this sweep injects a failure at every cache
    // allocation index and lets the BACKING testing.allocator's leak detector
    // prove each failure path freed everything — including the dupe orphaned
    // by a failed append, which the list-level errdefer alone cannot free.
    const db = TestDb.open() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer db.close();
    try seed(db.conn);
    defer cleanup(db.conn);

    const SWEEP_BOUND = 32; // comfortably past every cache-side allocation
    var fail_index: usize = 0;
    while (fail_index < SWEEP_BOUND) : (fail_index += 1) {
        var failing = std.testing.FailingAllocator.init(testing.allocator, .{ .fail_index = fail_index });
        var cache = FleetSetCache.init(failing.allocator(), common.globalIo());
        defer cache.deinit();
        cache.retain(WS_A) catch continue; // injected failure before any refresh
        defer cache.release(WS_A);
        cache.refreshIfStale(db.conn, WS_A, now());
    }
}

// ── Concurrency + complexity ────────────────────────────────────────────────
//
// The two properties this type exists for that the tests above do not reach:
// that its mutex actually protects the refcount under real contention, and
// that V viewers of one workspace cost ONE entry rather than V.

const CONC_THREADS = 128;
const CONC_ROUNDS = 16;

const ConcWorker = struct {
    /// Retain, read, and release in a tight loop — the interleaving that a
    /// missing or mis-scoped lock corrupts.
    fn hammer(cache: *FleetSetCache, start: *std.atomic.Value(bool), failed: *std.atomic.Value(bool)) void {
        while (!start.load(.acquire)) std.atomic.spinLoopHint();
        var i: usize = 0;
        while (i < CONC_ROUNDS) : (i += 1) {
            cache.retain(WS_A) catch {
                failed.store(true, .release);
                return;
            };
            _ = cache.version(WS_A);
            if (cache.snapshot(WS_A) catch null) |s| s.deinit(cache.alloc);
            cache.release(WS_A);
        }
    }
};

test "cache: 128 concurrent viewers never corrupt the refcount or duplicate the entry" {
    // Catches: a retain/release pair moved outside the locked section (refs
    // drifts, so the workspace either leaks forever or is freed while a viewer
    // still holds it), and a map insert racing another insert for the same key.
    // testing.allocator is the arbiter for the second failure mode.
    var cache = FleetSetCache.init(testing.allocator, common.globalIo());
    defer cache.deinit();

    // One reference held for the whole run, so the entry cannot be evicted and
    // recreated mid-race: the invariant under test is refcount arithmetic, not
    // create/destroy churn.
    try cache.retain(WS_A);

    var start = std.atomic.Value(bool).init(false);
    var failed = std.atomic.Value(bool).init(false);
    var threads: [CONC_THREADS]std.Thread = undefined;
    var spawned: usize = 0;
    for (&threads) |*t| {
        t.* = std.Thread.spawn(.{}, ConcWorker.hammer, .{ &cache, &start, &failed }) catch break;
        spawned += 1;
    }
    // Released even on a partial spawn, so already-started workers never spin
    // forever on a flag nobody will set.
    start.store(true, .release);
    for (threads[0..spawned]) |t| t.join();

    try testing.expectEqual(@as(usize, CONC_THREADS), spawned);
    try testing.expect(!failed.load(.acquire));
    // Exactly one entry despite 128 threads racing to create it.
    try testing.expectEqual(@as(u32, 1), cache.entries.count());
    // Every retain was paired with a release, so only this test's own
    // reference remains — releasing it evicts, exactly once.
    cache.release(WS_A);
    try testing.expectEqual(@as(u32, 0), cache.entries.count());
}

test "cache: V viewers of one workspace hold ONE entry's memory, not V" {
    // The sharing property proven by counter rather than by clock: the memory
    // still HELD after V viewers retain must stay flat as V doubles. A
    // per-viewer entry here is the per-viewer cost this type was built to
    // remove, and no correctness test above would notice it.
    //
    // Held bytes, not allocation calls: `retain` deliberately allocates a spare
    // key and entry BEFORE taking the lock and returns them when the entry
    // already exists, so the CALL count is linear in viewers by design (it is
    // what keeps the critical section non-fallible). What must not grow is what
    // survives the call.
    const ladder = [_]usize{ 32, 64, 128 };
    var held: [ladder.len]usize = undefined;

    for (ladder, 0..) |viewers, idx| {
        var counting = std.testing.FailingAllocator.init(testing.allocator, .{ .fail_index = std.math.maxInt(usize) });
        var cache = FleetSetCache.init(counting.allocator(), common.globalIo());
        defer cache.deinit();

        var i: usize = 0;
        while (i < viewers) : (i += 1) try cache.retain(WS_A);
        held[idx] = counting.allocated_bytes - counting.freed_bytes;
        while (i > 0) : (i -= 1) cache.release(WS_A);
    }

    // Flat across a 4x viewer increase — O(1) in viewers, not O(V).
    try testing.expectEqual(held[0], held[1]);
    try testing.expectEqual(held[1], held[2]);
}

test "cache: the spares a contended retain allocates are all returned" {
    // The other half of the design above: every speculative key/entry that did
    // not win the map slot must come back. A leak on that path would be
    // invisible to the held-bytes ladder, which only samples the total.
    var counting = std.testing.FailingAllocator.init(testing.allocator, .{ .fail_index = std.math.maxInt(usize) });
    {
        var cache = FleetSetCache.init(counting.allocator(), common.globalIo());
        defer cache.deinit();
        var i: usize = 0;
        while (i < 64) : (i += 1) try cache.retain(WS_A);
        while (i > 0) : (i -= 1) cache.release(WS_A);
    }
    // Every byte the 64 retains took is back — the 63 losing spares included.
    try testing.expectEqual(counting.allocated_bytes, counting.freed_bytes);
}
