//! Per-workspace + per-zombie Prometheus token counter.
//!
//! Fixed-capacity hash slot table keyed on (workspace_id, zombie_id).
//! No allocator at runtime — compile-time capacity. Overflow routes to `_other`.
//!
//! Thread-safe: slot claim uses CAS on first write, atomic loads afterwards.
//! Counter increments are lock-free atomic fetchAdd.

const std = @import("std");

/// Max distinct (workspace_id, zombie_id) pairs tracked. Overflow → `_other`.
const MAX_SLOTS: usize = 4096;

/// Truncated id length stored per slot (enough for Prometheus labels).
const ID_LEN: usize = 48;

const Counters = struct {
    tokens_total: std.atomic.Value(u64) = std.atomic.Value(u64).init(0),
};

const Slot = struct {
    occupied: std.atomic.Value(u8) = std.atomic.Value(u8).init(0),
    ready: std.atomic.Value(u8) = std.atomic.Value(u8).init(0),
    ws_id: [ID_LEN]u8 = [_]u8{0} ** ID_LEN,
    ws_id_len: u8 = 0,
    zombie_id: [ID_LEN]u8 = [_]u8{0} ** ID_LEN,
    zombie_id_len: u8 = 0,
    hash: u64 = 0,
    counters: Counters = .{},
};

var g_slots: [MAX_SLOTS]Slot = [_]Slot{.{}} ** MAX_SLOTS;
var g_overflow: Counters = .{};
var g_overflow_total = std.atomic.Value(u64).init(0);
var g_slot_count = std.atomic.Value(u32).init(0);

fn compositeHash(ws_id: []const u8, zombie_id: []const u8) u64 {
    var h = std.hash.Wyhash.init(0);
    h.update(ws_id);
    h.update("\x00");
    h.update(zombie_id);
    return h.final();
}

fn slotMatches(slot: *const Slot, h: u64, ws_id: []const u8, zombie_id: []const u8) bool {
    if (slot.hash != h) return false;
    const ws_cmp = @min(ws_id.len, ID_LEN);
    const z_cmp = @min(zombie_id.len, ID_LEN);
    if (slot.ws_id_len != ws_cmp or slot.zombie_id_len != z_cmp) return false;
    return std.mem.eql(u8, slot.ws_id[0..slot.ws_id_len], ws_id[0..ws_cmp]) and
        std.mem.eql(u8, slot.zombie_id[0..slot.zombie_id_len], zombie_id[0..z_cmp]);
}

fn initSlot(slot: *Slot, h: u64, ws_id: []const u8, zombie_id: []const u8) void {
    const ws_len: u8 = @intCast(@min(ws_id.len, ID_LEN));
    const z_len: u8 = @intCast(@min(zombie_id.len, ID_LEN));
    @memcpy(slot.ws_id[0..ws_len], ws_id[0..ws_len]);
    slot.ws_id_len = ws_len;
    @memcpy(slot.zombie_id[0..z_len], zombie_id[0..z_len]);
    slot.zombie_id_len = z_len;
    slot.hash = h;
    slot.ready.store(1, .release);
}

fn resolveSlot(ws_id: []const u8, zombie_id: []const u8) ?*Slot {
    const h = compositeHash(ws_id, zombie_id);
    const start = h % MAX_SLOTS;
    var i: usize = 0;
    while (i < MAX_SLOTS) : (i += 1) {
        const idx = (start + i) % MAX_SLOTS;
        const slot = &g_slots[idx];

        const occ = slot.occupied.load(.acquire);
        if (occ == 1) {
            if (slot.ready.load(.acquire) != 1) continue;
            if (slotMatches(slot, h, ws_id, zombie_id)) return slot;
            continue;
        }

        if (slot.occupied.cmpxchgStrong(0, 1, .acq_rel, .acquire)) |_| {
            continue;
        }

        initSlot(slot, h, ws_id, zombie_id);
        _ = g_slot_count.fetchAdd(1, .monotonic);
        return slot;
    }
    return null;
}

// ── Public increment API ──────────────────────────────────────────────────

pub fn addTokens(ws_id: []const u8, zombie_id: []const u8, tokens: u64) void {
    if (resolveSlot(ws_id, zombie_id)) |slot| {
        _ = slot.counters.tokens_total.fetchAdd(tokens, .monotonic);
    } else {
        _ = g_overflow.tokens_total.fetchAdd(tokens, .monotonic);
        _ = g_overflow_total.fetchAdd(1, .monotonic);
    }
}

// ── Prometheus rendering ──────────────────────────────────────────────────

pub fn renderPrometheus(writer: anytype) !void {
    const count = g_slot_count.load(.acquire);
    if (count == 0 and g_overflow_total.load(.acquire) == 0) return;

    try renderTokensFamily(writer);

    const overflow = g_overflow_total.load(.acquire);
    try writer.print("# HELP zombie_workspace_metrics_overflow_total Increments routed to _other due to (workspace,zombie) cardinality overflow.\n", .{});
    try writer.print("# TYPE zombie_workspace_metrics_overflow_total counter\n", .{});
    try writer.print("zombie_workspace_metrics_overflow_total {d}\n", .{overflow});
}

fn renderTokensFamily(writer: anytype) !void {
    const name = "zombie_agent_tokens_by_workspace_total";
    try writer.print("# HELP {s} Tokens consumed per (workspace, zombie).\n", .{name});
    try writer.print("# TYPE {s} counter\n", .{name});

    for (&g_slots) |*slot| {
        if (slot.occupied.load(.acquire) != 1 or slot.ready.load(.acquire) != 1) continue;
        const val = slot.counters.tokens_total.load(.acquire);
        if (val == 0) continue;
        try writer.print("{s}{{workspace_id=\"{s}\",zombie_id=\"{s}\"}} {d}\n", .{
            name,
            slot.ws_id[0..slot.ws_id_len],
            slot.zombie_id[0..slot.zombie_id_len],
            val,
        });
    }

    const ov = g_overflow.tokens_total.load(.acquire);
    if (ov > 0) {
        try writer.print("{s}{{workspace_id=\"_other\",zombie_id=\"_other\"}} {d}\n", .{ name, ov });
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

fn resetForTest() void {
    g_slots = [_]Slot{.{}} ** MAX_SLOTS;
    g_overflow = .{};
    g_overflow_total.store(0, .release);
    g_slot_count.store(0, .release);
}

test "addTokens tracks per (workspace, zombie)" {
    resetForTest();

    addTokens("ws-aaa", "z-1", 100);
    addTokens("ws-aaa", "z-1", 50);
    addTokens("ws-aaa", "z-2", 25);
    addTokens("ws-bbb", "z-1", 200);

    const slot_a_1 = resolveSlot("ws-aaa", "z-1").?;
    try std.testing.expectEqual(@as(u64, 150), slot_a_1.counters.tokens_total.load(.acquire));

    const slot_a_2 = resolveSlot("ws-aaa", "z-2").?;
    try std.testing.expectEqual(@as(u64, 25), slot_a_2.counters.tokens_total.load(.acquire));

    const slot_b_1 = resolveSlot("ws-bbb", "z-1").?;
    try std.testing.expectEqual(@as(u64, 200), slot_b_1.counters.tokens_total.load(.acquire));

    try std.testing.expectEqual(@as(u32, 3), g_slot_count.load(.acquire));
}

test "renderPrometheus outputs both labels" {
    resetForTest();

    addTokens("ws-render-test", "zombie-42", 42);

    var buf: [4096]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try renderPrometheus(fbs.writer());
    const output = fbs.getWritten();

    try std.testing.expect(std.mem.containsAtLeast(u8, output, 1, "zombie_agent_tokens_by_workspace_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, output, 1, "workspace_id=\"ws-render-test\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, output, 1, "zombie_id=\"zombie-42\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, output, 1, " 42\n"));
}

test "overflow counter starts at zero" {
    resetForTest();
    try std.testing.expectEqual(@as(u64, 0), g_overflow_total.load(.acquire));
}

test "resolveSlot returns same slot for identical (ws, zombie)" {
    resetForTest();

    const s1 = resolveSlot("ws-dedup", "z-dedup").?;
    _ = s1.counters.tokens_total.fetchAdd(10, .monotonic);
    const s2 = resolveSlot("ws-dedup", "z-dedup").?;

    try std.testing.expectEqual(s1, s2);
    try std.testing.expectEqual(@as(u64, 10), s2.counters.tokens_total.load(.acquire));
    try std.testing.expectEqual(@as(u32, 1), g_slot_count.load(.acquire));
}

test "slot with occupied=1 but ready=0 is skipped" {
    resetForTest();

    const h = compositeHash("ws-half", "z-half");
    const idx = h % MAX_SLOTS;
    g_slots[idx].occupied.store(1, .release);
    g_slots[idx].hash = h;
    g_slots[idx].ws_id_len = 7;
    @memcpy(g_slots[idx].ws_id[0..7], "ws-half");
    g_slots[idx].zombie_id_len = 6;
    @memcpy(g_slots[idx].zombie_id[0..6], "z-half");
    // ready=0 — must not be returned.

    if (resolveSlot("ws-half", "z-half")) |s| {
        try std.testing.expectEqual(@as(u8, 1), s.ready.load(.acquire));
    }
}

test "renderPrometheus skips slots with ready=0" {
    resetForTest();

    addTokens("ws-visible", "z-visible", 100);

    const h = compositeHash("ws-ghost", "z-ghost");
    const idx = h % MAX_SLOTS;
    g_slots[idx].occupied.store(1, .release);
    g_slots[idx].ws_id_len = 8;
    @memcpy(g_slots[idx].ws_id[0..8], "ws-ghost");
    g_slots[idx].zombie_id_len = 7;
    @memcpy(g_slots[idx].zombie_id[0..7], "z-ghost");
    g_slots[idx].counters.tokens_total.store(999, .release);
    g_slot_count.store(2, .release);

    var buf: [8192]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try renderPrometheus(fbs.writer());
    const output = fbs.getWritten();

    try std.testing.expect(std.mem.containsAtLeast(u8, output, 1, "ws-visible"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, output, 1, "ws-ghost"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, output, 1, "999"));
}

test "distinct (ws, zombie) pairs get distinct slots" {
    resetForTest();

    addTokens("ws-alpha", "z-1", 10);
    addTokens("ws-beta", "z-1", 20);
    addTokens("ws-alpha", "z-2", 30);

    try std.testing.expectEqual(@as(u32, 3), g_slot_count.load(.acquire));
    try std.testing.expectEqual(@as(u64, 10), resolveSlot("ws-alpha", "z-1").?.counters.tokens_total.load(.acquire));
    try std.testing.expectEqual(@as(u64, 20), resolveSlot("ws-beta", "z-1").?.counters.tokens_total.load(.acquire));
    try std.testing.expectEqual(@as(u64, 30), resolveSlot("ws-alpha", "z-2").?.counters.tokens_total.load(.acquire));
}
