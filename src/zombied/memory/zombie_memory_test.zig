//! Unit tier for the pure selection policy in `zombie_memory.zig`: the
//! category→tier map and the `selective` Compactor arm (category-pinned byte
//! window — `core` hydrates before any recency windowing). Extracted to a
//! sibling so the adapter module stays under the file-length cap. DB-backed
//! paths (storeEntry / enforceCap / listAll) live in
//! `zombie_memory_integration_test.zig`.
//!
//! Fixtures are built newest-first, mirroring `listAll`'s `updated_at DESC`
//! order — the order `compact()` is contracted to receive. Byte budgets are
//! computed from `entryBytes` sums rather than hand-added literals, so the
//! category strings' differing lengths can never skew the arithmetic.

const std = @import("std");
const adapter = @import("zombie_memory.zig");

const MemoryDelta = adapter.MemoryDelta;

fn delta(key: []const u8, content: []const u8, category: []const u8) MemoryDelta {
    return .{ .key = key, .content = content, .category = category };
}

fn budgetFor(deltas: []const MemoryDelta) usize {
    return adapter.sumBytes(deltas);
}

// ── category → tier ─────────────────────────────────────────────────────────

test "tier map pins core; daily and conversation stay windowed" {
    try std.testing.expectEqual(adapter.Tier.pinned, adapter.tierOf(adapter.CATEGORY_CORE));
    try std.testing.expectEqual(adapter.Tier.windowed, adapter.tierOf(adapter.CATEGORY_DAILY));
    try std.testing.expectEqual(adapter.Tier.windowed, adapter.tierOf(adapter.CATEGORY_CONVERSATION));
}

test "tier map defaults custom and unknown categories to windowed — never accidentally pinned" {
    try std.testing.expectEqual(adapter.Tier.windowed, adapter.tierOf("incident-notes"));
    try std.testing.expectEqual(adapter.Tier.windowed, adapter.tierOf("CORE")); // exact-match only
    try std.testing.expectEqual(adapter.Tier.windowed, adapter.tierOf(""));
}

// ── selective arm ───────────────────────────────────────────────────────────

test "selective pins every fitting core before any non-core is considered" {
    // Five newest daily entries ahead of two old core facts; the budget fits
    // exactly both cores plus the two newest dailies.
    var rows = [_]MemoryDelta{
        delta("d1", "new1", adapter.CATEGORY_DAILY),
        delta("d2", "new2", adapter.CATEGORY_DAILY),
        delta("d3", "new3", adapter.CATEGORY_DAILY),
        delta("d4", "new4", adapter.CATEGORY_DAILY),
        delta("d5", "new5", adapter.CATEGORY_DAILY),
        delta("c1", "old1", adapter.CATEGORY_CORE),
        delta("c2", "old2", adapter.CATEGORY_CORE),
    };
    const kept = [_]MemoryDelta{ rows[0], rows[1], rows[5], rows[6] };
    const c: adapter.Compactor = .{ .selective = budgetFor(&kept) };
    const out = c.compact(&rows);
    try std.testing.expectEqual(@as(usize, 4), out.len);
    // Original recency order preserved among the kept entries.
    try std.testing.expectEqualStrings("d1", out[0].key);
    try std.testing.expectEqualStrings("d2", out[1].key);
    try std.testing.expectEqualStrings("c1", out[2].key);
    try std.testing.expectEqualStrings("c2", out[3].key);
}

test "selective keeps the newest core prefix when core alone overflows the budget" {
    var rows = [_]MemoryDelta{
        delta("c1", "vvvv", adapter.CATEGORY_CORE),
        delta("c2", "vvvv", adapter.CATEGORY_CORE),
        delta("c3", "vvvv", adapter.CATEGORY_CORE),
    };
    const kept = [_]MemoryDelta{ rows[0], rows[1] };
    const c: adapter.Compactor = .{ .selective = budgetFor(&kept) };
    const out = c.compact(&rows);
    try std.testing.expectEqual(@as(usize, 2), out.len);
    try std.testing.expectEqualStrings("c1", out[0].key);
    try std.testing.expectEqualStrings("c2", out[1].key);
}

test "selective starves the windowed tier before dropping any fitting core" {
    // The cores consume the whole budget; the newer daily entry is the loss.
    var rows = [_]MemoryDelta{
        delta("d1", "newest", adapter.CATEGORY_DAILY),
        delta("c1", "values", adapter.CATEGORY_CORE),
        delta("c2", "values", adapter.CATEGORY_CORE),
    };
    const kept = [_]MemoryDelta{ rows[1], rows[2] };
    const c: adapter.Compactor = .{ .selective = budgetFor(&kept) };
    const out = c.compact(&rows);
    try std.testing.expectEqual(@as(usize, 2), out.len);
    try std.testing.expectEqualStrings("c1", out[0].key);
    try std.testing.expectEqualStrings("c2", out[1].key);
}

test "selective is a no-op preserving original order when the set fits" {
    var rows = [_]MemoryDelta{
        delta("d1", "a", adapter.CATEGORY_DAILY),
        delta("c1", "b", adapter.CATEGORY_CORE),
        delta("x1", "c", "scratch"),
    };
    const c: adapter.Compactor = .{ .selective = budgetFor(&rows) };
    const out = c.compact(&rows);
    try std.testing.expectEqual(@as(usize, 3), out.len);
    try std.testing.expectEqualStrings("d1", out[0].key);
    try std.testing.expectEqualStrings("c1", out[1].key);
    try std.testing.expectEqualStrings("x1", out[2].key);
}

test "selective is deterministic — same rows and budget yield identical output" {
    const build = struct {
        fn rows() [4]MemoryDelta {
            return .{
                delta("d1", "aaaa", adapter.CATEGORY_DAILY),
                delta("c1", "bbbb", adapter.CATEGORY_CORE),
                delta("d2", "cccc", adapter.CATEGORY_DAILY),
                delta("c2", "dddd", adapter.CATEGORY_CORE),
            };
        }
    };
    var first = build.rows();
    var second = build.rows();
    const budget = adapter.entryBytes(first[1]) + adapter.entryBytes(first[3]) + adapter.entryBytes(first[0]);
    const ca: adapter.Compactor = .{ .selective = budget };
    const a = ca.compact(&first);
    const b = ca.compact(&second);
    try std.testing.expectEqual(a.len, b.len);
    for (a, b) |ea, eb| {
        try std.testing.expectEqualStrings(ea.key, eb.key);
        try std.testing.expectEqualStrings(ea.content, eb.content);
        try std.testing.expectEqualStrings(ea.category, eb.category);
    }
}

test "selective always hydrates at least one entry — oversized core head" {
    var rows = [_]MemoryDelta{
        delta("big-core", "xxxxxxxxxxxxxxxx", adapter.CATEGORY_CORE),
    };
    const c: adapter.Compactor = .{ .selective = 4 };
    try std.testing.expectEqual(@as(usize, 1), c.compact(&rows).len);
}

test "selective always hydrates at least one entry — oversized windowed head, no core" {
    var rows = [_]MemoryDelta{
        delta("big-daily", "xxxxxxxxxxxxxxxx", adapter.CATEGORY_DAILY),
        delta("older", "yy", adapter.CATEGORY_DAILY),
    };
    const c: adapter.Compactor = .{ .selective = 4 };
    const out = c.compact(&rows);
    try std.testing.expectEqual(@as(usize, 1), out.len);
    try std.testing.expectEqualStrings("big-daily", out[0].key);
}

test "selective without any core matches plain newest-first windowing" {
    var rows = [_]MemoryDelta{
        delta("k0", "aaaa", "x"),
        delta("k1", "bbbb", "x"),
        delta("k2", "cccc", "x"),
    };
    const kept = [_]MemoryDelta{ rows[0], rows[1] };
    const c: adapter.Compactor = .{ .selective = budgetFor(&kept) };
    const out = c.compact(&rows);
    try std.testing.expectEqual(@as(usize, 2), out.len);
    try std.testing.expectEqualStrings("k0", out[0].key);
    try std.testing.expectEqualStrings("k1", out[1].key);
}

test "passthrough returns the rows unchanged" {
    var rows = [_]MemoryDelta{delta("a", "b", "c")};
    const c: adapter.Compactor = .passthrough;
    try std.testing.expectEqual(@as(usize, 1), c.compact(&rows).len);
}
