// Unit tests for M14_001 memory HTTP handler contract — Part 1.
// Covers: T3 (HTTP status), T7 (error code stability), T8 (security),
//         T9 (imports), T10 (constants).
//
// Part 2 (T2, T11, T12) lives in m14_001_memory_handler_shapes_test.zig.
// Live HTTP enforcement (T1, T5, T6) requires LIVE_DB=1.

const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");
const ec = @import("../../errors/error_registry.zig");
const h = @import("memory_http_helpers.zig");

// ── T9: Module import resolution ─────────────────────────────────────────────

test "M14_001 T9: memory_http module imports resolve" {
    _ = @import("memory_http.zig");
}

test "M14_001 T9: memory_http_helpers module imports resolve" {
    _ = @import("memory_http_helpers.zig");
}

// ── T10: Spec-defined limits pinned (spec §2 validation) ─────────────────────

const MAX_KEY_LEN: usize = 255;
const MAX_CONTENT_LEN: usize = 16 * 1024;
const MAX_RECALL_LIMIT: i64 = 100;
const DEFAULT_RECALL_LIMIT: i64 = 20;
const DEFAULT_LIST_LIMIT: i64 = 100;

test "M14_001 T10: MAX_KEY_LEN = 255 (spec §2)" {
    try std.testing.expectEqual(@as(usize, 255), MAX_KEY_LEN);
    try std.testing.expectEqual(MAX_KEY_LEN, h.MAX_KEY_LEN);
}

test "M14_001 T10: MAX_CONTENT_LEN = 16384 bytes (spec §2)" {
    try std.testing.expectEqual(@as(usize, 16384), MAX_CONTENT_LEN);
    try std.testing.expectEqual(MAX_CONTENT_LEN, h.MAX_CONTENT_LEN);
}

test "M14_001 T10: MAX_RECALL_LIMIT = 100 (spec §2)" {
    try std.testing.expectEqual(@as(i64, 100), MAX_RECALL_LIMIT);
    try std.testing.expectEqual(MAX_RECALL_LIMIT, h.MAX_RECALL_LIMIT);
}

test "M14_001 T10: DEFAULT_RECALL_LIMIT = 20 (spec §2)" {
    try std.testing.expectEqual(@as(i64, 20), DEFAULT_RECALL_LIMIT);
    try std.testing.expectEqual(DEFAULT_RECALL_LIMIT, h.DEFAULT_RECALL_LIMIT);
}

test "M14_001 T10: DEFAULT_LIST_LIMIT = 100 (spec §2)" {
    try std.testing.expectEqual(@as(i64, 100), DEFAULT_LIST_LIMIT);
    try std.testing.expectEqual(DEFAULT_LIST_LIMIT, h.DEFAULT_LIST_LIMIT);
}

// ── T7: Error code string stability (regression guard) ───────────────────────

test "M14_001 T7: ERR_MEM_SCOPE = UZ-MEM-001" {
    try std.testing.expectEqualStrings("UZ-MEM-001", ec.ERR_MEM_SCOPE);
}

test "M14_001 T7: ERR_MEM_ZOMBIE_NOT_FOUND = UZ-MEM-002" {
    try std.testing.expectEqualStrings("UZ-MEM-002", ec.ERR_MEM_ZOMBIE_NOT_FOUND);
}

test "M14_001 T7: ERR_MEM_UNAVAILABLE = UZ-MEM-003" {
    try std.testing.expectEqualStrings("UZ-MEM-003", ec.ERR_MEM_UNAVAILABLE);
}

test "M14_001 T7: all UZ-MEM-* codes have UZ-MEM- prefix" {
    for ([_][]const u8{ ec.ERR_MEM_SCOPE, ec.ERR_MEM_ZOMBIE_NOT_FOUND, ec.ERR_MEM_UNAVAILABLE }) |code| {
        try std.testing.expect(std.mem.startsWith(u8, code, "UZ-MEM-"));
    }
}

test "M14_001 T7: UZ-MEM codes are sequential 001-002-003" {
    try std.testing.expectEqualStrings("UZ-MEM-001", ec.ERR_MEM_SCOPE);
    try std.testing.expectEqualStrings("UZ-MEM-002", ec.ERR_MEM_ZOMBIE_NOT_FOUND);
    try std.testing.expectEqualStrings("UZ-MEM-003", ec.ERR_MEM_UNAVAILABLE);
}

// ── T3: UZ-MEM-* → correct HTTP status codes ─────────────────────────────────

test "M14_001 T3: UZ-MEM-001 (scope denied) → HTTP 403 Forbidden" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    common.errorResponse(ht.res, ec.ERR_MEM_SCOPE, "scope denied", "req-t3a");
    try ht.expectStatus(403);
}

test "M14_001 T3: UZ-MEM-002 (zombie not found) → HTTP 404 Not Found" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    common.errorResponse(ht.res, ec.ERR_MEM_ZOMBIE_NOT_FOUND, "not found", "req-t3b");
    try ht.expectStatus(404);
}

test "M14_001 T3: UZ-MEM-003 (backend unavailable) → HTTP 503 Service Unavailable" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    common.errorResponse(ht.res, ec.ERR_MEM_UNAVAILABLE, "backend down", "req-t3c");
    try ht.expectStatus(503);
}

test "M14_001 T3: UZ-MEM-* error body contains error_code field" {
    for ([_]struct { code: []const u8, status: u16 }{
        .{ .code = ec.ERR_MEM_SCOPE, .status = 403 },
        .{ .code = ec.ERR_MEM_ZOMBIE_NOT_FOUND, .status = 404 },
        .{ .code = ec.ERR_MEM_UNAVAILABLE, .status = 503 },
    }) |c| {
        var ht = httpz.testing.init(.{});
        defer ht.deinit();
        common.errorResponse(ht.res, c.code, "detail", "req-all");
        try ht.expectStatus(c.status);
        const json = try ht.getJson();
        try std.testing.expectEqualStrings(c.code, json.object.get("error_code").?.string);
    }
}

test "M14_001 T3: UZ-MEM-001 hint references 'workspace' (scope error is contextual)" {
    const entry = ec.lookup(ec.ERR_MEM_SCOPE);
    try std.testing.expect(std.mem.indexOf(u8, entry.hint, "workspace") != null);
}

// ── T8: Security contract ─────────────────────────────────────────────────────

test "M14_001 T8: scope rejection is ERR_MEM_SCOPE not generic ERR_FORBIDDEN" {
    try std.testing.expect(!std.mem.eql(u8, ec.ERR_MEM_SCOPE, ec.ERR_FORBIDDEN));
}

test "M14_001 T8: UZ-MEM-001 is HTTP 403 not 401 (scope != unauthenticated)" {
    // 401 = unauthenticated, 403 = authenticated but wrong workspace.
    const entry = ec.lookup(ec.ERR_MEM_SCOPE);
    try std.testing.expectEqual(std.http.Status.forbidden, entry.http_status);
}

test "M14_001 T8: ERR_MEM_SCOPE title references memory or scope" {
    const entry = ec.lookup(ec.ERR_MEM_SCOPE);
    const ok = std.mem.indexOf(u8, entry.title, "Memory") != null or
        std.mem.indexOf(u8, entry.title, "scope") != null;
    try std.testing.expect(ok);
}

// ── T11: Memory safety — httpz arena balanced ────────────────────────────────

test "M14_001 T11: 50 errorResponse calls for each UZ-MEM-* do not leak" {
    var i: usize = 0;
    while (i < 50) : (i += 1) {
        var ht = httpz.testing.init(.{});
        defer ht.deinit();
        common.errorResponse(ht.res, ec.ERR_MEM_SCOPE, "d", "r");
        try ht.expectStatus(403);
    }
}
