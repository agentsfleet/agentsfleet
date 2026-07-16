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

fn seed(conn: *pg.Conn) !void {
    const ts = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'FleetSetCacheTest', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ TENANT_ID, ts });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
    , .{ WS_A, TENANT_ID, ts });
    inline for (.{ FLEET_ONE, FLEET_TWO }, .{ "cache-one", "cache-two" }) |zid, name| {
        _ = try conn.exec(
            \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
            \\VALUES ($1, $2, $3, '---\nname: zz\n---\ntest', '{"name":"zz"}', 'active', 0, 0)
            \\ON CONFLICT DO NOTHING
        , .{ zid, WS_A, name });
    }
}

/// Best-effort teardown delete; a failure is logged, not swallowed silently
/// (a bare empty catch trips zlint's suppressed-errors).
const CLEANUP_IGNORED = "cleanup ignored: {s}";

fn cleanup(conn: *pg.Conn) void {
    inline for (.{ FLEET_ONE, FLEET_TWO }) |zid| {
        _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{zid}) catch |err| std.log.warn(CLEANUP_IGNORED, .{@errorName(err)});
    }
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
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'FleetSetCacheTest', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
    , .{ TENANT_ID, ts });
    _ = try db.conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
    , .{ WS_B, TENANT_ID, ts });
    defer _ = db.conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{WS_B}) catch |err| std.log.warn(CLEANUP_IGNORED, .{@errorName(err)});

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
