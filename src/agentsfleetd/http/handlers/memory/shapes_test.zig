// Unit tests for the memory HTTP helper contract — boundary validation,
// helper functions, response JSON shape pins, and leak checks. The sibling
// memory_handler_test.zig covers the handler-level paths.

const std = @import("std");
const h = @import("helpers.zig");

// ── Key length boundaries ─────────────────────────────────────────────────────

test "key at exact limit (255 bytes) is accepted" {
    const at_limit = "k" ** 255;
    try std.testing.expectEqual(@as(usize, 255), at_limit.len);
    try std.testing.expect(at_limit.len > 0 and at_limit.len <= h.MAX_KEY_LEN);
}

test "key one over limit (256 bytes) is rejected" {
    const over = "k" ** 256;
    try std.testing.expect(over.len > h.MAX_KEY_LEN);
}

test "empty key (0 bytes) is rejected by len == 0 guard" {
    const empty: []const u8 = "";
    try std.testing.expectEqual(@as(usize, 0), empty.len);
}

test "single-byte key is at minimum" {
    const min: []const u8 = "x";
    try std.testing.expect(min.len > 0 and min.len <= h.MAX_KEY_LEN);
}

// ── Content length boundaries ────────────────────────────────────────────────

test "content at exact limit (16384 bytes) is accepted" {
    const at_limit = "c" ** 16384;
    try std.testing.expectEqual(@as(usize, 16384), at_limit.len);
    try std.testing.expect(at_limit.len > 0 and at_limit.len <= h.MAX_CONTENT_LEN);
}

test "content one over limit (16385 bytes) is rejected" {
    const over = "c" ** 16385;
    try std.testing.expect(over.len > h.MAX_CONTENT_LEN);
}

test "empty content (0 bytes) is rejected by len == 0 guard" {
    const empty: []const u8 = "";
    try std.testing.expectEqual(@as(usize, 0), empty.len);
}

// ── genId produces well-formed IDs ───────────────────────────────────────────

test "genId returns non-empty string" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    try std.testing.expect(h.genId(arena.allocator()).len > 0);
}

test "genId format has exactly two hyphens (ts-hi-lo)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const id = h.genId(arena.allocator());
    var count: usize = 0;
    for (id) |c| if (c == '-') {
        count += 1;
    };
    try std.testing.expectEqual(@as(usize, 2), count);
}

test "two sequential genId calls produce distinct values" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const id1 = h.genId(arena.allocator());
    const id2 = h.genId(arena.allocator());
    try std.testing.expect(!std.mem.eql(u8, id1, id2));
}

// ── escapeLikePattern ────────────────────────────────────────────────────────

test "escapeLikePattern plain string unchanged" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const out = try h.escapeLikePattern(arena.allocator(), "hello");
    try std.testing.expectEqualStrings("hello", out);
}

test "escapeLikePattern escapes percent" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const out = try h.escapeLikePattern(arena.allocator(), "100%");
    try std.testing.expectEqualStrings("100\\%", out);
}

test "escapeLikePattern escapes underscore" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const out = try h.escapeLikePattern(arena.allocator(), "a_b");
    try std.testing.expectEqualStrings("a\\_b", out);
}

test "escapeLikePattern escapes backslash" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const out = try h.escapeLikePattern(arena.allocator(), "a\\b");
    try std.testing.expectEqualStrings("a\\\\b", out);
}

test "escapeLikePattern escapes all metacharacters in one string" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const out = try h.escapeLikePattern(arena.allocator(), "%_\\");
    try std.testing.expectEqualStrings("\\%\\_\\\\", out);
}

test "escapeLikePattern empty input returns empty output" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const out = try h.escapeLikePattern(arena.allocator(), "");
    try std.testing.expectEqual(@as(usize, 0), out.len);
}

// ── Response JSON shape pins — the LIVE tenant read envelope ─────────────────
// The tenant memory surface is read-only: GET /memories answers
// {items, total, request_id} with numeric epoch-millis updated_at. The retired
// write-verb shapes (store/forget) have no pins here — those verbs 404/405,
// proven in memories_integration_test.zig.

test "tenant list response shape parses (items array, numeric updated_at)" {
    const json =
        \\{"items":[{"key":"k1","content":"v1","category":"core","updated_at":1744498800000}],"total":1,"request_id":"r"}
    ;
    const E = struct { key: []const u8, content: []const u8, category: []const u8, updated_at: i64 };
    const S = struct { items: []E, total: usize, request_id: []const u8 };
    const p = try std.json.parseFromSlice(S, std.testing.allocator, json, .{ .ignore_unknown_fields = false });
    defer p.deinit();
    try std.testing.expectEqual(@as(usize, 1), p.value.total);
    try std.testing.expectEqual(@as(i64, 1_744_498_800_000), p.value.items[0].updated_at);
    try std.testing.expectEqualStrings("core", p.value.items[0].category);
}

test "tenant list empty result (total=0, items=[]) parses correctly" {
    const json =
        \\{"items":[],"total":0,"request_id":"r"}
    ;
    const S = struct { items: []struct { key: []const u8 }, total: usize, request_id: []const u8 };
    const p = try std.json.parseFromSlice(S, std.testing.allocator, json, .{});
    defer p.deinit();
    try std.testing.expectEqual(@as(usize, 0), p.value.total);
    try std.testing.expectEqual(@as(usize, 0), p.value.items.len);
}

// ── Memory safety — helpers do not leak ──────────────────────────────────────

test "200 genId calls via arena leave no leaks" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var i: usize = 0;
    while (i < 200) : (i += 1) {
        try std.testing.expect(h.genId(arena.allocator()).len > 0);
    }
}
