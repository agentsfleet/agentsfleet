//! The single durable fleet-memory write path + the hydration read, shared by
//! the runner-plane memory handlers (POST capture, GET hydrate). Pure DB access
//! — no HTTP coupling: callers `SET ROLE memory_runtime` first (see
//! `http/handlers/memory/helpers.zig`) and map errors to their own response.
//!
//! This module owns the ONLY `INSERT INTO memory.memory_entries` in the system
//! (RULE: single write path). The tenant memory API is read-only; every durable
//! write flows through `storeEntry`. Memory is scoped by `fleet_id` (UUID) —
//! our identifier end to end; the legacy NullClaw `instance_id` prefix form is
//! gone with the in-child Postgres path.
//!
//! RULE NSQ: schema-qualified SQL. Allocator: caller-provided (the handler's
//! per-request arena in production); `listAll` returns an arena-owned slice.

const std = @import("std");
const logging = @import("log");
const pg = @import("pg");
const protocol = @import("contract").protocol;
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const sql = @import("sql.zig");

const log = logging.scoped(.fleet_memory);

/// The wire shape of one memory item, reused verbatim for the hydrate read so the
/// handler serializes it directly into `MemoryHydrateResponse` (RULE UFS).
pub const MemoryDelta = protocol.MemoryDelta;

/// Stored category strings — NullClaw's `MemoryCategory.toString` vocabulary;
/// anything else on a row is a custom category. Single-sourced here for the
/// tier map, the eviction ordering, and every test fixture (RULE UFS/TFX).
pub const CATEGORY_CORE: []const u8 = "core";
pub const CATEGORY_DAILY: []const u8 = "daily";
pub const CATEGORY_CONVERSATION: []const u8 = "conversation";

/// Selection tier: `pinned` entries hydrate before any recency windowing and
/// are evicted only when no `windowed` row remains; `windowed` entries live
/// under the classic newest-first byte budget.
pub const Tier = enum { pinned, windowed };

// LOCKSTEP INVARIANT: the pinned set here must equal the set enforceCap's
// eviction ordering protects (its `category = CATEGORY_CORE` bind). The
// comptime assertion pins the pinned-entry count to exactly one, so adding a
// second pinned category fails compilation until that SQL widens in the same
// diff — hydration can never pin what eviction still deletes first.
const TIER_ENTRIES = .{
    .{ CATEGORY_CORE, Tier.pinned },
    .{ CATEGORY_DAILY, Tier.windowed },
    .{ CATEGORY_CONVERSATION, Tier.windowed },
};

const TIER_MAP = std.StaticStringMap(Tier).initComptime(TIER_ENTRIES);

comptime {
    var pinned_count: usize = 0;
    for (TIER_ENTRIES) |entry| {
        if (entry[1] == .pinned) pinned_count += 1;
    }
    std.debug.assert(pinned_count == 1);
}

/// Category → tier. Unknown and custom categories default to `windowed` —
/// an unrecognised string must never be accidentally pinned.
pub fn tierOf(category: []const u8) Tier {
    return TIER_MAP.get(category) orelse .windowed;
}

/// Strategy for bounding a fleet's hydration payload. `selective` pins the
/// `core` tier — every `core` entry that fits the byte budget hydrates before
/// any non-core entry is considered; the remaining budget fills with the
/// newest non-core entries, and the cold tail stays durable in Postgres.
/// `passthrough` returns everything (the tenant read). Deterministic, no
/// allocation, no LLM: summarisation belongs on the executor plane.
pub const Compactor = union(enum) {
    const Self = @This();

    /// Hand the rows over unchanged — the seam's identity arm (no production
    /// caller today; retained per spec as the no-compaction option).
    passthrough,
    /// Category-pinned byte window: `core` first, then newest non-core.
    selective: usize,

    pub fn compact(self: Self, rows: []MemoryDelta) []const MemoryDelta {
        return switch (self) {
            .passthrough => rows,
            .selective => |budget| selectByTier(rows, budget),
        };
    }
};

/// The bytes one entry charges against any memory byte budget — the single
/// formula shared by the hydration window, the capture push cap, and the
/// dropped-bytes loss counter, so the three can never silently diverge.
pub fn entryBytes(d: MemoryDelta) usize {
    return d.key.len + d.content.len + d.category.len;
}

/// Cumulative entryBytes over a slice — the loss-accounting helper the
/// hydrate handler diffs before/after compaction.
pub fn sumBytes(rows: []const MemoryDelta) usize {
    var total: usize = 0;
    for (rows) |d| total += entryBytes(d);
    return total;
}

/// One tier's running byte-budget admission, shared by the sizing pass and
/// the selection pass so the two can never diverge. Newest-first PREFIX
/// semantics: the first entry that overflows the budget closes the tier — an
/// older, smaller entry is never admitted past a rejected newer one (matching
/// the pre-tier window's cut). The tier's head bypasses the budget once when
/// `head_taken` starts false (the never-empty rule).
const TierRun = struct {
    const Self = @This();

    used: usize = 0,
    head_taken: bool = false,
    closed: bool = false,

    fn admit(self: *Self, sz: usize, budget: usize) bool {
        if (!self.head_taken) {
            self.head_taken = true;
            self.used = sz;
            return true;
        }
        if (self.closed) return false;
        if (self.used + sz <= budget) {
            self.used += sz;
            return true;
        }
        self.closed = true;
        return false;
    }
};

/// Stable in-place tier selection over rows arriving `updated_at DESC`: pass
/// one sizes what the pinned tier consumes; pass two replays those admissions
/// (same `TierRun` rule, so the passes are in lockstep by construction) plus
/// the windowed tier against the remaining budget, swapping survivors left.
/// The kept prefix preserves the original recency order, the tail holds the
/// dropped entries (permuted) — the slice stays a permutation of the input,
/// so per-entry ownership survives compaction for callers that free
/// individual entries. A non-empty input always hydrates at least one entry:
/// the pinned head when any `core` exists, the overall head otherwise.
fn selectByTier(rows: []MemoryDelta, budget: usize) []const MemoryDelta {
    var sizing: TierRun = .{};
    var pinned_any = false;
    for (rows) |d| {
        if (tierOf(d.category) != .pinned) continue;
        pinned_any = true;
        _ = sizing.admit(entryBytes(d), budget);
    }
    const windowed_budget = budget -| sizing.used;
    var pinned: TierRun = .{};
    // Any pinned entry satisfies the never-empty rule via the pinned head, so
    // the windowed tier gets head privilege only when no pinned entry exists.
    var windowed: TierRun = .{ .head_taken = pinned_any };
    var kept: usize = 0;
    for (rows, 0..) |d, i| {
        const admitted = switch (tierOf(d.category)) {
            .pinned => pinned.admit(entryBytes(d), budget),
            .windowed => windowed.admit(entryBytes(d), windowed_budget),
        };
        if (admitted) {
            std.mem.swap(MemoryDelta, &rows[kept], &rows[i]);
            kept += 1;
        }
    }
    return rows[0..kept];
}

/// Upsert one memory entry under `fleet_id` — the ONLY `INSERT` into
/// `memory.memory_entries`. Caller has already `SET ROLE memory_runtime`.
/// `ts_ms` is epoch milliseconds from the project clock (one read per push).
/// Idempotent via `ON CONFLICT (key, fleet_id) DO UPDATE`, so a retried or
/// duplicate push never creates a second row for the same key.
pub fn storeEntry(
    conn: *pg.Conn,
    id: []const u8,
    fleet_id: []const u8,
    key: []const u8,
    content: []const u8,
    category: []const u8,
    ts_ms: i64,
) !void {
    const uid_value = try id_format.generateUuidV7();
    const uid: []const u8 = &uid_value;
    _ = try conn.exec(
        sql.UPSERT_ENTRY, .{ uid, id, key, content, category, fleet_id, ts_ms });
}

/// Evict the entries beyond `max` for `fleet_id` (the per-fleet durable-set cap),
/// tier-ordered: the kept set prefers every `core` row (newest-first within the
/// tier), then the newest non-core rows — so victims are the coldest non-core rows
/// first, and a `core` row is evicted only when no non-core row remains. The
/// protected set is the `CATEGORY_CORE` bind below — in lockstep with TIER_MAP's
/// pinned set (see the invariant comment there). Call once
/// after a push loop, in the same `memory_runtime` transaction. A backstop against
/// unbounded growth — the fleet's own stable-key overwrite + `memory_forget` are the
/// primary bound. No-op under the cap. Returns the evicted-row count (the driver's
/// rows-affected); a result carrying no count reports 0 with a warn — eviction is
/// never blocked on telemetry.
pub fn enforceCap(conn: *pg.Conn, fleet_id: []const u8, max: usize) !u64 {
    const affected = try conn.exec(
        sql.EVICT_PAST_CAP, .{ fleet_id, @as(i64, @intCast(max)), CATEGORY_CORE });
    const n = affected orelse {
        log.warn("memory_cap_evict_count_unavailable", .{ .fleet_id = fleet_id });
        return 0;
    };
    return if (n < 0) 0 else @intCast(n);
}

/// Retention window for `daily` entries: scratch notes older than this many
/// milliseconds (by `updated_at`) expire on the next capture push. Every other
/// category is expiry-exempt by construction — the sweep binds CATEGORY_DAILY
/// as a parameter, never a pattern.
pub const DAILY_RETENTION_MS: i64 = 72 * std.time.ms_per_hour;

/// Delete the fleet's `daily` rows with `updated_at` older than `cutoff_ms` —
/// `enforceCap`'s sibling in shape, call site, and posture: per-fleet scoped
/// DELETE, called once after a capture push in the same `memory_runtime` role
/// window, and a failure warns at the call site without failing the capture.
/// Returns the deleted-row count; a result carrying no count reports 0 with a
/// warn — expiry is never blocked on telemetry.
pub fn sweepExpiredDaily(conn: *pg.Conn, fleet_id: []const u8, cutoff_ms: i64) !u64 {
    const affected = try conn.exec(
        sql.DELETE_AGED_IN_CATEGORY, .{ fleet_id, CATEGORY_DAILY, cutoff_ms });
    const n = affected orelse {
        log.warn("memory_daily_sweep_count_unavailable", .{ .fleet_id = fleet_id });
        return 0;
    };
    return if (n < 0) 0 else @intCast(n);
}

/// Forget one entry by key — the operator's correction path when a fleet
/// learned something wrong. Keyed on `(fleet_id, key)`, the same pair the
/// upsert's unique index owns, so no fleet can forget another's memory.
/// Caller has already `SET ROLE memory_runtime`. Returns false when the key
/// was not there (the handler answers 404 — a mistyped key is surfaced, not
/// swallowed).
pub fn deleteEntry(conn: *pg.Conn, fleet_id: []const u8, key: []const u8) !bool {
    var q = PgQuery.from(try conn.query(
        sql.DELETE_ENTRY_BY_KEY, .{ fleet_id, key }));
    defer q.deinit();
    return (try q.next()) != null;
}

/// Every memory entry for `fleet_id`, newest first — the hydration read the
/// runner parent seeds into the child's in-run store. Caller has already
/// `SET ROLE memory_runtime`. Returns an arena-owned `[]MemoryDelta`; each field
/// is duped before the query result is drained.
pub fn listAll(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    fleet_id: []const u8,
) ![]MemoryDelta {
    var out: std.ArrayList(MemoryDelta) = .empty;
    errdefer out.deinit(alloc);
    var q = PgQuery.from(try conn.query(
        sql.SELECT_ALL_FOR_FLEET, .{fleet_id}));
    defer q.deinit();
    while (try q.next()) |row| {
        // Copy each row-backed slice before the next next()/drain invalidates it.
        const key = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(key);
        const content = try alloc.dupe(u8, try row.get([]const u8, 1));
        errdefer alloc.free(content);
        const category = try alloc.dupe(u8, try row.get([]const u8, 2));
        errdefer alloc.free(category);
        try out.append(alloc, .{ .key = key, .content = content, .category = category });
    }
    return out.toOwnedSlice(alloc);
}

// Pure selection-policy tests live in the sibling `fleet_memory_test.zig`
// (registered in `tests.zig`); storeEntry/enforceCap/listAll need a live
// Postgres and are exercised by `fleet_memory_integration_test.zig`.
