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

/// Strategy for bounding a zombie's hydration payload. `recency_window` returns the
/// newest entries (rows arrive `updated_at DESC` from `listAll`) whose cumulative
/// bytes fit a budget, dropping the cold tail from the payload — the tail stays
/// durable in Postgres, so nothing is lost. `passthrough` returns everything (the
/// tenant read). Deterministic, no allocation, no LLM: summarisation belongs on the
/// executor plane, not the control plane. The seam holds for a future selective arm.
pub const Compactor = union(enum) {
    /// Hand the rows over unchanged (the tenant GET path).
    passthrough,
    /// Keep the newest entries within this many bytes; evict the cold tail.
    recency_window: usize,

    pub fn compact(self: Compactor, rows: []const MemoryDelta) []const MemoryDelta {
        return switch (self) {
            .passthrough => rows,
            .recency_window => |budget| windowByBytes(rows, budget),
        };
    }
};

/// The bytes one entry charges against any memory byte budget — the single
/// formula shared by the hydration window, the capture push cap, and the
/// dropped-bytes loss counter, so the three can never silently diverge.
pub fn entryBytes(d: MemoryDelta) usize {
    return d.key.len + d.content.len + d.category.len;
}

/// Newest-first prefix of `rows` whose cumulative entryBytes stay within
/// `budget`. Always includes at least the newest entry (so an oversized head
/// still hydrates something); the remainder is the cold tail, left in the database.
fn windowByBytes(rows: []const MemoryDelta, budget: usize) []const MemoryDelta {
    var used: usize = 0;
    for (rows, 0..) |d, i| {
        const sz = entryBytes(d);
        if (i > 0 and used + sz > budget) return rows[0..i];
        used += sz;
    }
    return rows;
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

/// Evict the coldest entries beyond `max` for `zombie_id` (the per-zombie durable-set
/// cap), keeping the newest by `updated_at DESC`. Call once after a push loop, in the
/// same `memory_runtime` transaction. A backstop against unbounded growth — the agent's
/// own stable-key overwrite + `memory_forget` are the primary bound. No-op under the cap.
/// Returns the evicted-row count (the driver's rows-affected); a driver result that
/// carries no count reports 0 with a warn — eviction is never blocked on telemetry.
pub fn enforceCap(conn: *pg.Conn, zombie_id: []const u8, max: usize) !u64 {
    const affected = try conn.exec(
        \\DELETE FROM memory.memory_entries
        \\WHERE zombie_id = $1::uuid
        \\  AND id IN (
        \\    SELECT id FROM memory.memory_entries
        \\    WHERE zombie_id = $1::uuid
        \\    ORDER BY updated_at DESC, id DESC
        \\    OFFSET $2
        \\  )
    , .{ zombie_id, @as(i64, @intCast(max)) });
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

// ── Tests ───────────────────────────────────────────────────────────────────
// Compactor windowing is pure (no DB). storeEntry/enforceCap/listAll need a live
// Postgres and are exercised by the integration tier.

test "recency_window keeps every entry when the set fits the budget" {
    const rows = [_]MemoryDelta{
        .{ .key = "a", .content = "11", .category = "c" },
        .{ .key = "b", .content = "22", .category = "c" },
    };
    const c: Compactor = .{ .recency_window = 1024 };
    try std.testing.expectEqual(@as(usize, 2), c.compact(&rows).len);
}

test "recency_window drops the cold tail past the byte budget, newest kept" {
    const rows = [_]MemoryDelta{
        .{ .key = "k0", .content = "aaaa", .category = "x" }, // 2+4+1 = 7
        .{ .key = "k1", .content = "bbbb", .category = "x" }, // cumulative 14
        .{ .key = "k2", .content = "cccc", .category = "x" }, // would be 21
    };
    const c: Compactor = .{ .recency_window = 14 };
    const out = c.compact(&rows);
    try std.testing.expectEqual(@as(usize, 2), out.len);
    try std.testing.expectEqualStrings("k0", out[0].key);
    try std.testing.expectEqualStrings("k1", out[1].key);
}

test "recency_window always hydrates at least the newest entry" {
    const rows = [_]MemoryDelta{
        .{ .key = "big", .content = "xxxxxxxxxx", .category = "c" }, // 14 bytes > budget
    };
    const c: Compactor = .{ .recency_window = 4 };
    try std.testing.expectEqual(@as(usize, 1), c.compact(&rows).len);
}

test "passthrough returns the rows unchanged" {
    const rows = [_]MemoryDelta{.{ .key = "a", .content = "b", .category = "c" }};
    const c: Compactor = .passthrough;
    try std.testing.expectEqual(@as(usize, 1), c.compact(&rows).len);
}
