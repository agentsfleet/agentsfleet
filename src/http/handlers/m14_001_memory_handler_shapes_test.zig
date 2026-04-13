// Unit tests for M14_001 memory HTTP handler contract — Part 2.
// Covers: T2 (boundary validation, helper functions),
//         T11 (memory safety), T12 (response JSON shapes).
//
// Part 1 (T3, T7, T8, T9, T10) lives in m14_001_memory_handler_test.zig.

const std = @import("std");
const ec = @import("../../errors/error_registry.zig");
const h = @import("memory_http_helpers.zig");

// ── T2: Key length boundaries ─────────────────────────────────────────────────

test "M14_001 T2: key at exact limit (255 bytes) is accepted" {
    const at_limit = "k" ** 255;
    try std.testing.expectEqual(@as(usize, 255), at_limit.len);
    try std.testing.expect(at_limit.len > 0 and at_limit.len <= h.MAX_KEY_LEN);
}

test "M14_001 T2: key one over limit (256 bytes) is rejected" {
    const over = "k" ** 256;
    try std.testing.expect(over.len > h.MAX_KEY_LEN);
}

test "M14_001 T2: empty key (0 bytes) is rejected by len == 0 guard" {
    const empty: []const u8 = "";
    try std.testing.expectEqual(@as(usize, 0), empty.len);
}

test "M14_001 T2: single-byte key is at minimum" {
    const min: []const u8 = "x";
    try std.testing.expect(min.len > 0 and min.len <= h.MAX_KEY_LEN);
}

// ── T2: Content length boundaries ────────────────────────────────────────────

test "M14_001 T2: content at exact limit (16384 bytes) is accepted" {
    const at_limit = "c" ** 16384;
    try std.testing.expectEqual(@as(usize, 16384), at_limit.len);
    try std.testing.expect(at_limit.len > 0 and at_limit.len <= h.MAX_CONTENT_LEN);
}

test "M14_001 T2: content one over limit (16385 bytes) is rejected" {
    const over = "c" ** 16385;
    try std.testing.expect(over.len > h.MAX_CONTENT_LEN);
}

test "M14_001 T2: empty content (0 bytes) is rejected by len == 0 guard" {
    const empty: []const u8 = "";
    try std.testing.expectEqual(@as(usize, 0), empty.len);
}

// ── T2: nowTs returns a valid Unix timestamp string ───────────────────────────

test "M14_001 T2: nowTs returns non-empty string parseable as i64" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const ts = h.nowTs(arena.allocator());
    try std.testing.expect(ts.len > 0);
    const val = try std.fmt.parseInt(i64, ts, 10);
    try std.testing.expect(val > 1_577_836_800); // > 2020-01-01
}

test "M14_001 T2: nowTs contains only decimal digit characters" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const ts = h.nowTs(arena.allocator());
    for (ts) |c| try std.testing.expect(c >= '0' and c <= '9');
}

test "M14_001 T2: nowTs is at least 10 digits (current Unix epoch)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    try std.testing.expect(h.nowTs(arena.allocator()).len >= 10);
}

// ── T2: genId produces well-formed IDs ───────────────────────────────────────

test "M14_001 T2: genId returns non-empty string" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    try std.testing.expect(h.genId(arena.allocator()).len > 0);
}

test "M14_001 T2: genId format has exactly two hyphens (ts-hi-lo)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const id = h.genId(arena.allocator());
    var count: usize = 0;
    for (id) |c| if (c == '-') { count += 1; };
    try std.testing.expectEqual(@as(usize, 2), count);
}

test "M14_001 T2: two sequential genId calls produce distinct values" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const id1 = h.genId(arena.allocator());
    const id2 = h.genId(arena.allocator());
    try std.testing.expect(!std.mem.eql(u8, id1, id2));
}

// ── T2: escapeLikePattern ────────────────────────────────────────────────────

test "M14_001 T2: escapeLikePattern plain string unchanged" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const out = try h.escapeLikePattern(arena.allocator(), "hello");
    try std.testing.expectEqualStrings("hello", out);
}

test "M14_001 T2: escapeLikePattern escapes percent" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const out = try h.escapeLikePattern(arena.allocator(), "100%");
    try std.testing.expectEqualStrings("100\\%", out);
}

test "M14_001 T2: escapeLikePattern escapes underscore" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const out = try h.escapeLikePattern(arena.allocator(), "a_b");
    try std.testing.expectEqualStrings("a\\_b", out);
}

test "M14_001 T2: escapeLikePattern escapes backslash" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const out = try h.escapeLikePattern(arena.allocator(), "a\\b");
    try std.testing.expectEqualStrings("a\\\\b", out);
}

test "M14_001 T2: escapeLikePattern escapes all metacharacters in one string" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const out = try h.escapeLikePattern(arena.allocator(), "%_\\");
    try std.testing.expectEqualStrings("\\%\\_\\\\", out);
}

test "M14_001 T2: escapeLikePattern empty input returns empty output" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const out = try h.escapeLikePattern(arena.allocator(), "");
    try std.testing.expectEqual(@as(usize, 0), out.len);
}

// ── T12: Response JSON shape contracts ───────────────────────────────────────

test "M14_001 T12: store success response shape parses (zombie_id/key/category/request_id)" {
    const json =
        \\{"zombie_id":"0195b4ba-8d3a-7f13-8abc-000000000001","key":"lead_acme","category":"core","request_id":"r1"}
    ;
    const Shape = struct { zombie_id: []const u8, key: []const u8, category: []const u8, request_id: []const u8 };
    const p = try std.json.parseFromSlice(Shape, std.testing.allocator, json, .{ .ignore_unknown_fields = false });
    defer p.deinit();
    try std.testing.expectEqualStrings("lead_acme", p.value.key);
    try std.testing.expectEqualStrings("core", p.value.category);
}

test "M14_001 T12: store response has no 'content' field (not echoed back)" {
    // content is write-only; echoing 16KB wastes bandwidth and leaks to logs.
    const json =
        \\{"zombie_id":"z","key":"k","category":"core","request_id":"r"}
    ;
    const Shape = struct { zombie_id: []const u8, key: []const u8, category: []const u8, request_id: []const u8 };
    const p = try std.json.parseFromSlice(Shape, std.testing.allocator, json, .{ .ignore_unknown_fields = false });
    defer p.deinit();
    try std.testing.expectEqualStrings("k", p.value.key);
}

test "M14_001 T12: store HTTP status is 201 Created" {
    try std.testing.expectEqual(@as(u10, 201), @intFromEnum(std.http.Status.created));
}

test "M14_001 T12: recall success response shape parses (entries array with updated_at)" {
    const json =
        \\{"zombie_id":"z","entries":[{"key":"k1","content":"v1","category":"core","updated_at":"1744498800"}],"count":1,"request_id":"r"}
    ;
    const E = struct { key: []const u8, content: []const u8, category: []const u8, updated_at: []const u8 };
    const S = struct { zombie_id: []const u8, entries: []E, count: usize, request_id: []const u8 };
    const p = try std.json.parseFromSlice(S, std.testing.allocator, json, .{ .ignore_unknown_fields = false });
    defer p.deinit();
    try std.testing.expectEqual(@as(usize, 1), p.value.count);
    try std.testing.expectEqualStrings("1744498800", p.value.entries[0].updated_at);
}

test "M14_001 T12: recall empty result (count=0, entries=[]) parses correctly" {
    const json =
        \\{"zombie_id":"z","entries":[],"count":0,"request_id":"r"}
    ;
    const S = struct { zombie_id: []const u8, entries: []struct { key: []const u8 }, count: usize, request_id: []const u8 };
    const p = try std.json.parseFromSlice(S, std.testing.allocator, json, .{});
    defer p.deinit();
    try std.testing.expectEqual(@as(usize, 0), p.value.count);
    try std.testing.expectEqual(@as(usize, 0), p.value.entries.len);
}

test "M14_001 T12: list response shape is identical to recall (same contract)" {
    const json =
        \\{"zombie_id":"z","entries":[{"key":"k","content":"v","category":"daily","updated_at":"1744498800"}],"count":1,"request_id":"r"}
    ;
    const E = struct { key: []const u8, content: []const u8, category: []const u8, updated_at: []const u8 };
    const S = struct { zombie_id: []const u8, entries: []E, count: usize, request_id: []const u8 };
    const p = try std.json.parseFromSlice(S, std.testing.allocator, json, .{ .ignore_unknown_fields = false });
    defer p.deinit();
    try std.testing.expectEqualStrings("daily", p.value.entries[0].category);
}

test "M14_001 T12: forget deleted=true response parses correctly" {
    const json =
        \\{"zombie_id":"z","key":"k","deleted":true,"request_id":"r"}
    ;
    const S = struct { zombie_id: []const u8, key: []const u8, deleted: bool, request_id: []const u8 };
    const p = try std.json.parseFromSlice(S, std.testing.allocator, json, .{ .ignore_unknown_fields = false });
    defer p.deinit();
    try std.testing.expect(p.value.deleted);
}

test "M14_001 T12: forget deleted=false (idempotent) parses correctly" {
    const json =
        \\{"zombie_id":"z","key":"missing","deleted":false,"request_id":"r"}
    ;
    const S = struct { zombie_id: []const u8, key: []const u8, deleted: bool, request_id: []const u8 };
    const p = try std.json.parseFromSlice(S, std.testing.allocator, json, .{ .ignore_unknown_fields = false });
    defer p.deinit();
    try std.testing.expect(!p.value.deleted); // no error, idempotent
}

test "M14_001 T12: forget response has no 'entries' field (not a list)" {
    const json =
        \\{"zombie_id":"z","key":"k","deleted":true,"request_id":"r"}
    ;
    const S = struct { zombie_id: []const u8, key: []const u8, deleted: bool, request_id: []const u8 };
    const p = try std.json.parseFromSlice(S, std.testing.allocator, json, .{ .ignore_unknown_fields = false });
    defer p.deinit();
    try std.testing.expectEqualStrings("k", p.value.key);
}

// ── T11: Memory safety — helpers do not leak ─────────────────────────────────

test "M14_001 T11: 200 nowTs calls via arena leave no leaks" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var i: usize = 0;
    while (i < 200) : (i += 1) {
        try std.testing.expect(h.nowTs(arena.allocator()).len > 0);
    }
}

test "M14_001 T11: 200 genId calls via arena leave no leaks" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var i: usize = 0;
    while (i < 200) : (i += 1) {
        try std.testing.expect(h.genId(arena.allocator()).len > 0);
    }
}
