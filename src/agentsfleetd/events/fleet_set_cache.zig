//! The readable fleet set of a workspace, enumerated ONCE per workspace per
//! cadence and shared by every stream watching it.
//!
//! Why: the workspace stream collapses a wall's per-tile connections into one.
//! If each connection then re-enumerated `core.fleets` on its refresh tick, V
//! viewers would cost V full scans — trading a per-viewer connection for a
//! per-viewer query. The fleet set is a property of the WORKSPACE, so it is
//! resolved once and read by all.
//!
//! NOT shared: authorization. Whether a caller may still read the workspace is
//! a property of the CALLER, re-checked per tick (one indexed point lookup) — a
//! revoked member must lose frames even when the set is unchanged, which no
//! shared cache can provide.
//!
//! Concurrency: `mutex` guards the map + every entry field and NOTHING else —
//! the enumeration never runs under it (C3). A stale tick claims the refresh
//! with an in-flight flag, queries lock-released, then swaps in; concurrent
//! ticks serve the current set for one more cadence, not duplicate queries.
//!
//! Versioning: `version` bumps only when the SET changes; a viewer whose
//! version matches does no query, no diff, no allocation.

const FleetSetCache = @This();

alloc: std.mem.Allocator,
io: std.Io,
/// Guards `entries` and every `Entry` field. Never held across the DB query.
mutex: std.Io.Mutex = .init,
entries: std.StringHashMapUnmanaged(*Entry) = .empty,
/// Test seam: counts completed enumerations so a test can prove V viewers cost
/// one query, not V.
enumerations: std.atomic.Value(u64) = .init(0),
/// How long a fleet set is served before a tick refreshes it, and the cadence
/// the stream loop re-authorizes on (one knob so the two never drift). Default
/// = prod's `REFRESH_INTERVAL_MS`; the test harness lowers it so a fleet
/// appearing / a membership revoked surfaces within a test's patience.
refresh_interval_ms: i64 = REFRESH_INTERVAL_MS,

/// How long a fleet set is served before a tick refreshes it. A fleet created
/// now becomes visible to every live stream within this window.
pub const REFRESH_INTERVAL_MS: i64 = 10_000;

/// Upper bound on fleets enumerated for one workspace stream. A workspace past
/// this is fanned in up to the cap and the overflow is logged — a bounded query
/// beats an unbounded one silently pinning the reader (RESOURCE BUDGET).
pub const MAX_FANNED_IN_FLEETS: u32 = 500;

const Entry = struct {
    /// Owned fleet ids, sorted — sorted so a set comparison is a linear walk
    /// rather than an O(n²) membership scan.
    fleet_ids: [][]u8 = &.{},
    /// Bumps only when `fleet_ids` actually changes. Starts at 0, meaning
    /// "never enumerated" — no viewer's observed version can match that.
    version: u64 = 0,
    refreshed_ms: i64 = 0,
    /// One refresher at a time; the others serve the current set for one more
    /// cadence rather than duplicating the query.
    refreshing: bool = false,
    /// Live streams holding this workspace. Evicted at zero, so an idle
    /// workspace costs nothing.
    refs: usize = 0,
};

/// What a viewer reads when the version moved: the fleet ids at that version,
/// owned by the caller.
pub const Snapshot = struct {
    fleet_ids: [][]u8,
    version: u64,

    pub fn deinit(self: Snapshot, alloc: std.mem.Allocator) void {
        for (self.fleet_ids) |id| alloc.free(id);
        alloc.free(self.fleet_ids);
    }
};

pub fn init(alloc: std.mem.Allocator, io: std.Io) FleetSetCache {
    return .{ .alloc = alloc, .io = io };
}

/// Frees every entry. Call after every stream has released its workspace —
/// a live stream still holding a ref must never race this.
pub fn deinit(self: *FleetSetCache) void {
    var it = self.entries.iterator();
    while (it.next()) |kv| {
        freeIds(self.alloc, kv.value_ptr.*.fleet_ids);
        self.alloc.destroy(kv.value_ptr.*);
        self.alloc.free(kv.key_ptr.*);
    }
    self.entries.deinit(self.alloc);
    self.* = undefined;
}

/// A stream takes a reference to its workspace's set. Balanced by `release`.
///
/// Everything fallible is allocated BEFORE the lock; `consumed` routes the
/// spares into the map or back to the allocator on the way out — the same shape
/// the hub's `attach` uses, and for the same reason (no fallible work, and no
/// allocator call that could fail unnoticed, under the mutex).
pub fn retain(self: *FleetSetCache, workspace_id: []const u8) error{OutOfMemory}!void {
    const spare_key = try self.alloc.dupe(u8, workspace_id);
    const spare_entry = self.alloc.create(Entry) catch |err| {
        self.alloc.free(spare_key);
        return err;
    };
    spare_entry.* = .{ .refs = 1 };
    var consumed = false;
    defer if (!consumed) {
        self.alloc.free(spare_key);
        self.alloc.destroy(spare_entry);
    };

    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    const gop = try self.entries.getOrPut(self.alloc, spare_key);
    if (gop.found_existing) {
        gop.value_ptr.*.refs += 1;
        return; // the defer returns the spares
    }
    gop.value_ptr.* = spare_entry;
    consumed = true;
}

/// The last stream off a workspace evicts its set — an idle workspace holds no
/// memory and pays no query.
pub fn release(self: *FleetSetCache, workspace_id: []const u8) void {
    var evicted: ?*Entry = null;
    var evicted_key: ?[]const u8 = null;
    {
        self.mutex.lockUncancelable(self.io);
        defer self.mutex.unlock(self.io);
        const kv = self.entries.getEntry(workspace_id) orelse return;
        const entry = kv.value_ptr.*;
        if (entry.refs > 0) entry.refs -= 1;
        if (entry.refs > 0) return;
        // A refresh in flight still reads this entry — leave it mapped and let
        // the refresher's swap-in find refs == 0 and evict it then.
        if (entry.refreshing) return;
        evicted = entry;
        evicted_key = kv.key_ptr.*;
        self.entries.removeByPtr(kv.key_ptr);
    }
    if (evicted) |entry| {
        freeIds(self.alloc, entry.fleet_ids);
        self.alloc.destroy(entry);
    }
    if (evicted_key) |key| self.alloc.free(key);
}

/// The version a viewer compares against. 0 means "never enumerated", which no
/// viewer's observed version can equal — so the first tick always reads.
pub fn version(self: *FleetSetCache, workspace_id: []const u8) u64 {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    const entry = self.entries.get(workspace_id) orelse return 0;
    return entry.version;
}

/// Copy out the current set. Called ONLY when the version moved, so the copy is
/// paid on change, never on a steady-state tick. Caller owns the result.
pub fn snapshot(self: *FleetSetCache, workspace_id: []const u8) error{OutOfMemory}!?Snapshot {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    const entry = self.entries.get(workspace_id) orelse return null;
    if (entry.version == 0) return null;

    const ids = try self.alloc.alloc([]u8, entry.fleet_ids.len);
    var copied: usize = 0;
    errdefer {
        for (ids[0..copied]) |id| self.alloc.free(id);
        self.alloc.free(ids);
    }
    for (entry.fleet_ids, 0..) |id, i| {
        ids[i] = try self.alloc.dupe(u8, id);
        copied += 1;
    }
    return .{ .fleet_ids = ids, .version = entry.version };
}

/// Refresh the set if it is stale and nobody else is already doing it.
///
/// Exactly one caller runs the query; the rest return immediately and serve the
/// current set for one more cadence. The query runs with the map mutex RELEASED
/// — a stalled Postgres must never block `version`, `snapshot`, `retain`, or
/// `release` (C3).
pub fn refreshIfStale(self: *FleetSetCache, conn: *pg.Conn, workspace_id: []const u8, now_ms: i64) void {
    if (!self.claimRefresh(workspace_id, now_ms)) return;
    // From here we own the refresh flag and MUST clear it on every path.
    const fresh = enumerate(self.alloc, conn, workspace_id) catch |err| {
        log.warn("fleet_set_enumeration_failed", .{
            .error_code = ec.ERR_INTERNAL_DB_QUERY,
            .workspace_id = workspace_id,
            .err = @errorName(err),
        });
        self.finishRefresh(workspace_id, null);
        return;
    };
    _ = self.enumerations.fetchAdd(1, .monotonic); // safe because: a test-only counter, no state depends on its ordering
    self.finishRefresh(workspace_id, fresh);
}

/// True when THIS caller took responsibility for the refresh.
fn claimRefresh(self: *FleetSetCache, workspace_id: []const u8, now_ms: i64) bool {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    const entry = self.entries.get(workspace_id) orelse return false;
    if (entry.refreshing) return false;
    const fresh_enough = entry.version != 0 and (now_ms - entry.refreshed_ms) < self.refresh_interval_ms;
    if (fresh_enough) return false;
    entry.refreshing = true;
    return true;
}

/// Install the refreshed set (or just clear the flag when the query failed —
/// the previous set keeps serving, and the next tick retries).
fn finishRefresh(self: *FleetSetCache, workspace_id: []const u8, fresh: ?[][]u8) void {
    var discard: ?[][]u8 = fresh;
    var evicted: ?*Entry = null;
    var evicted_key: ?[]const u8 = null;
    install: {
        self.mutex.lockUncancelable(self.io);
        defer self.mutex.unlock(self.io);
        // Entry gone (a deinit raced us): the `discard` defer below still frees
        // the list we just built, so a lost race costs a query, never a leak.
        const kv = self.entries.getEntry(workspace_id) orelse break :install;
        const entry = kv.value_ptr.*;
        entry.refreshing = false;
        entry.refreshed_ms = clock.nowMillis();

        if (fresh) |ids| {
            if (sameSet(entry.fleet_ids, ids)) {
                // Unchanged: keep the old list (and its version), free the new
                // one — every viewer's tick stays a no-op.
                discard = ids;
            } else {
                discard = entry.fleet_ids;
                entry.fleet_ids = ids;
                entry.version += 1;
            }
        }
        // The last viewer left while we were querying: evict now that the
        // refresh flag is clear (release() deferred to us).
        if (entry.refs == 0) {
            evicted = entry;
            evicted_key = kv.key_ptr.*;
            self.entries.removeByPtr(kv.key_ptr);
        }
    }
    if (discard) |ids| freeIds(self.alloc, ids);
    if (evicted) |entry| {
        freeIds(self.alloc, entry.fleet_ids);
        self.alloc.destroy(entry);
    }
    if (evicted_key) |key| self.alloc.free(key);
}

/// The RLS-scoped enumeration itself: every fleet that EXISTS in the workspace
/// (not every fleet with events — a fleet must be subscribed BEFORE its first
/// event or that event's frames are systematically missed). Sorted so callers
/// can diff two sets with a linear walk.
///
/// Reads `core.fleets` under the workspace's owning tenant, resolved here: the
/// set is a property of the workspace, so it must not depend on which viewer's
/// tick happened to run the refresh.
fn enumerate(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8) ![][]u8 {
    if (!try setOwningTenantContext(conn, workspace_id)) return error.WorkspaceNotFound;

    // Every fleet in the workspace, ANY status — a paused fleet mid-run still
    // emits and a resumed one must reappear, so NO status filter (the partial
    // `_active` index is deliberately unused). `workspace_id` scans
    // `uq_fleets_workspace_id_name`; `ORDER BY id` is a bounded top-N sort giving
    // a deterministic set + stable truncation, paid ~0.1×/sec/workspace (shared).
    // `AS fleet_id` is load-bearing: without it the output column is named `id`,
    // so `ORDER BY id` binds to the TEXT cast and sorts 36-char strings under the
    // DB collation — slower than the native 16-byte uuid compare AND locale-
    // dependent (which would make the stable-set order collation-sensitive).
    var q = PgQuery.from(try conn.query(
        \\SELECT id::text AS fleet_id FROM core.fleets
        \\WHERE workspace_id = $1::uuid
        \\ORDER BY id
        \\LIMIT $2
    , .{ workspace_id, @as(i64, @intCast(MAX_FANNED_IN_FLEETS)) }));
    defer q.deinit();

    var ids: std.ArrayList([]u8) = .empty;
    errdefer {
        for (ids.items) |id| alloc.free(id);
        ids.deinit(alloc);
    }
    while (try q.next()) |row| {
        const id = try row.get([]const u8, 0);
        try ids.append(alloc, try alloc.dupe(u8, id));
    }
    if (ids.items.len == MAX_FANNED_IN_FLEETS) {
        log.warn("fleet_set_truncated_at_cap", .{
            .workspace_id = workspace_id,
            .cap = MAX_FANNED_IN_FLEETS,
        });
    }
    return ids.toOwnedSlice(alloc);
}

/// Set the RLS tenant context to the workspace's OWNER. False when the
/// workspace does not exist.
fn setOwningTenantContext(conn: *pg.Conn, workspace_id: []const u8) !bool {
    // ONE round-trip: the tenant resolve is folded into the `set_config`
    // subquery, so the workspace lookup + the RLS-context write are a single
    // statement instead of two. `set_config` returns the value it set — a
    // non-empty tenant means the workspace exists and the context is now scoped
    // to its owner; an empty/NULL result means the workspace vanished (a race
    // against a concurrent delete, since authorize already confirmed it existed
    // this tick), and the subsequent fleets query fails closed under the empty
    // context. Runs at most once per workspace per refresh cadence.
    var q = PgQuery.from(try conn.query(
        \\SELECT COALESCE(set_config('app.current_tenant_id',
        \\  (SELECT tenant_id::text FROM core.workspaces WHERE workspace_id = $1), false), '')
    , .{workspace_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return false;
    const applied = try row.get([]const u8, 0);
    return applied.len > 0;
}

/// Both lists are sorted, so equality is a linear walk.
fn sameSet(a: []const []const u8, b: []const []const u8) bool {
    if (a.len != b.len) return false;
    for (a, b) |x, y| {
        if (!std.mem.eql(u8, x, y)) return false;
    }
    return true;
}

fn freeIds(alloc: std.mem.Allocator, ids: [][]u8) void {
    for (ids) |id| alloc.free(id);
    alloc.free(ids);
}

const std = @import("std");
const pg = @import("pg");
const common = @import("common");
const clock = common.clock;
const logging = @import("log");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const ec = @import("../errors/error_registry.zig");
const log = logging.scoped(.fleet_set_cache);

test {
    _ = @import("fleet_set_cache_test.zig");
}
