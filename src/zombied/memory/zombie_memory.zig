//! The single durable agent-memory write path + the hydration read, shared by
//! the runner-plane memory handlers (POST capture, GET hydrate). Pure DB access
//! — no HTTP coupling: callers `SET ROLE memory_runtime` first (see
//! `http/handlers/memory/helpers.zig`) and map errors to their own response.
//!
//! This module owns the ONLY `INSERT INTO memory.memory_entries` in the system
//! (RULE: single write path). The tenant memory API is read-only; every durable
//! write flows through `storeEntry`. Memory is scoped by `zombie_id` (UUID) —
//! our identifier end to end; the legacy NullClaw `instance_id`/`"zmb:"` form is
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

const log = logging.scoped(.zombie_memory);

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

const TIER_MAP = std.StaticStringMap(Tier).initComptime(.{
    .{ CATEGORY_CORE, .pinned },
    .{ CATEGORY_DAILY, .windowed },
    .{ CATEGORY_CONVERSATION, .windowed },
});

/// Category → tier. Unknown and custom categories default to `windowed` —
/// an unrecognised string must never be accidentally pinned.
pub fn tierOf(category: []const u8) Tier {
    return TIER_MAP.get(category) orelse .windowed;
}

/// Strategy for bounding a zombie's hydration payload. `selective` pins the
/// `core` tier — every `core` entry that fits the byte budget hydrates before
/// any non-core entry is considered; the remaining budget fills with the
/// newest non-core entries, and the cold tail stays durable in Postgres.
/// `passthrough` returns everything (the tenant read). Deterministic, no
/// allocation, no LLM: summarisation belongs on the executor plane.
pub const Compactor = union(enum) {
    /// Hand the rows over unchanged (the tenant GET path).
    passthrough,
    /// Category-pinned byte window: `core` first, then newest non-core.
    selective: usize,

    pub fn compact(self: Compactor, rows: []MemoryDelta) []const MemoryDelta {
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

/// Stable in-place tier selection over rows arriving `updated_at DESC`:
/// pass one sizes what the pinned tier consumes (newest-first, its head
/// admitted even when oversized — mirroring the old window's head rule);
/// pass two replays those admissions plus the windowed tier against the
/// remaining budget, swapping survivors left. The kept prefix preserves the
/// original recency order, the tail holds the dropped entries (permuted) —
/// the slice stays a permutation of the input, so per-entry ownership
/// survives compaction for callers that free individual entries. A non-empty
/// input always hydrates at least one entry: the pinned head when any `core`
/// exists, the overall head otherwise.
fn selectByTier(rows: []MemoryDelta, budget: usize) []const MemoryDelta {
    var pinned_used: usize = 0;
    var pinned_any = false;
    for (rows) |d| {
        if (tierOf(d.category) != .pinned) continue;
        const sz = entryBytes(d);
        if (!pinned_any) {
            pinned_used = sz;
            pinned_any = true;
        } else if (pinned_used + sz <= budget) {
            pinned_used += sz;
        }
    }
    const windowed_budget = budget -| pinned_used;
    var pinned_run: usize = 0;
    var pinned_head = true;
    var windowed_run: usize = 0;
    var kept: usize = 0;
    for (rows, 0..) |d, i| {
        const sz = entryBytes(d);
        const admit = switch (tierOf(d.category)) {
            .pinned => admit: {
                if (pinned_head) {
                    pinned_run = sz;
                    pinned_head = false;
                    break :admit true;
                }
                if (pinned_run + sz <= budget) {
                    pinned_run += sz;
                    break :admit true;
                }
                break :admit false;
            },
            .windowed => admit: {
                if (!pinned_any and kept == 0) {
                    windowed_run = sz;
                    break :admit true;
                }
                if (windowed_run + sz <= windowed_budget) {
                    windowed_run += sz;
                    break :admit true;
                }
                break :admit false;
            },
        };
        if (admit) {
            std.mem.swap(MemoryDelta, &rows[kept], &rows[i]);
            kept += 1;
        }
    }
    return rows[0..kept];
}

/// Upsert one memory entry under `zombie_id` — the ONLY `INSERT` into
/// `memory.memory_entries`. Caller has already `SET ROLE memory_runtime`.
/// Idempotent via `ON CONFLICT (key, zombie_id) DO UPDATE`, so a retried or
/// duplicate push never creates a second row for the same key.
pub fn storeEntry(
    conn: *pg.Conn,
    id: []const u8,
    zombie_id: []const u8,
    key: []const u8,
    content: []const u8,
    category: []const u8,
    ts: []const u8,
) !void {
    var uid_buf: [36]u8 = undefined;
    const uid = try id_format.formatUuidV7(&uid_buf);
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries
        \\  (uid, id, key, content, category, zombie_id, session_id, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6::uuid, NULL, $7, $7)
        \\ON CONFLICT (key, zombie_id) DO UPDATE
        \\  SET content = EXCLUDED.content,
        \\      category = EXCLUDED.category,
        \\      updated_at = EXCLUDED.updated_at
    , .{ uid, id, key, content, category, zombie_id, ts });
}

/// Evict the entries beyond `max` for `zombie_id` (the per-zombie durable-set cap),
/// tier-ordered: the kept set prefers every `core` row (newest-first within the
/// tier), then the newest non-core rows — so victims are the coldest non-core rows
/// first, and a `core` row is evicted only when no non-core row remains. Call once
/// after a push loop, in the same `memory_runtime` transaction. A backstop against
/// unbounded growth — the agent's own stable-key overwrite + `memory_forget` are the
/// primary bound. No-op under the cap. Returns the evicted-row count (the driver's
/// rows-affected); a result carrying no count reports 0 with a warn — eviction is
/// never blocked on telemetry.
pub fn enforceCap(conn: *pg.Conn, zombie_id: []const u8, max: usize) !u64 {
    const affected = try conn.exec(
        \\DELETE FROM memory.memory_entries
        \\WHERE zombie_id = $1::uuid
        \\  AND id IN (
        \\    SELECT id FROM memory.memory_entries
        \\    WHERE zombie_id = $1::uuid
        \\    ORDER BY (category = $3) DESC, updated_at DESC, id DESC
        \\    OFFSET $2
        \\  )
    , .{ zombie_id, @as(i64, @intCast(max)), CATEGORY_CORE });
    const n = affected orelse {
        log.warn("memory_cap_evict_count_unavailable", .{ .zombie_id = zombie_id });
        return 0;
    };
    return if (n < 0) 0 else @intCast(n);
}

/// Every memory entry for `zombie_id`, newest first — the hydration read the
/// runner parent seeds into the child's in-run store. Caller has already
/// `SET ROLE memory_runtime`. Returns an arena-owned `[]MemoryDelta`; each field
/// is duped before the query result is drained.
pub fn listAll(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    zombie_id: []const u8,
) ![]MemoryDelta {
    var out: std.ArrayList(MemoryDelta) = .empty;
    errdefer out.deinit(alloc);
    var q = PgQuery.from(try conn.query(
        \\SELECT key, content, category
        \\FROM memory.memory_entries
        \\WHERE zombie_id = $1::uuid
        \\ORDER BY updated_at DESC, id DESC
    , .{zombie_id}));
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

// Pure selection-policy tests live in the sibling `zombie_memory_test.zig`
// (registered in `tests.zig`); storeEntry/enforceCap/listAll need a live
// Postgres and are exercised by `zombie_memory_integration_test.zig`.
