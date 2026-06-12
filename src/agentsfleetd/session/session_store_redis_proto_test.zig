//! Pure-parser + outcome-mapping unit tests for `session_store_redis_proto.zig`.
//!
//! Extracted from the module so the protocol surface stays under the
//! file-length cap — sibling to `session_store_redis_ttl_integration_test.zig`,
//! which was split off the integration suite for the same reason. Discovered
//! via `src/auth/tests.zig`. These are pure tests: they synthesize RESP values
//! and never touch Redis.

const std = @import("std");
const redis_protocol = @import("../queue/redis_protocol.zig");
const proto = @import("session_store_redis_proto.zig");

const testing = std.testing;

fn bulk(alloc: std.mem.Allocator, s: []const u8) !redis_protocol.RespValue {
    return .{ .bulk = try alloc.dupe(u8, s) };
}

fn arrayOf(alloc: std.mem.Allocator, items: []const []const u8) !redis_protocol.RespValue {
    var arr = try alloc.alloc(redis_protocol.RespValue, items.len);
    errdefer alloc.free(arr);
    for (items, 0..) |s, i| arr[i] = try bulk(alloc, s);
    return .{ .array = arr };
}

test "firstTag returns the leading bulk on a string-only array" {
    var resp = try arrayOf(testing.allocator, &.{ "ok", "ignored" });
    defer resp.deinit(testing.allocator);
    try testing.expectEqualStrings("ok", proto.firstTag(resp).?);
}

test "firstTag returns null on non-array RESP" {
    const resp = redis_protocol.RespValue{ .integer = 7 };
    try testing.expect(proto.firstTag(resp) == null);
}

test "mapApproveOutcome ok maps to void success" {
    var resp = try arrayOf(testing.allocator, &.{"ok"});
    defer resp.deinit(testing.allocator);
    try proto.mapApproveOutcome(resp);
}

test "mapApproveOutcome conflict maps to AlreadyApproved" {
    var resp = try arrayOf(testing.allocator, &.{ "conflict", "verification_pending" });
    defer resp.deinit(testing.allocator);
    try testing.expectError(proto.Error.AlreadyApproved, proto.mapApproveOutcome(resp));
}

test "mapDeleteOutcome not_owner maps to NotOwner" {
    var resp = try arrayOf(testing.allocator, &.{"not_owner"});
    defer resp.deinit(testing.allocator);
    try testing.expectError(proto.Error.NotOwner, proto.mapDeleteOutcome(resp));
}

test "parseVerifyOutcome missing → .missing" {
    var resp = try arrayOf(testing.allocator, &.{"missing"});
    defer resp.deinit(testing.allocator);
    const outcome = try proto.parseVerifyOutcome(resp);
    try testing.expect(outcome == .missing);
}

test "parseVerifyOutcome rate_limited → .rate_limited (distinct from aborted)" {
    var resp = try arrayOf(testing.allocator, &.{"rate_limited"});
    defer resp.deinit(testing.allocator);
    const outcome = try proto.parseVerifyOutcome(resp);
    try testing.expect(outcome == .rate_limited);
}

test "parseVerifyOutcome aborted carries reason string" {
    var resp = try arrayOf(testing.allocator, &.{ "aborted", "rate_limit_exceeded" });
    defer resp.deinit(testing.allocator);
    const outcome = try proto.parseVerifyOutcome(resp);
    try testing.expectEqualStrings("rate_limit_exceeded", outcome.aborted);
}

test "parseVerifyOutcome invalid_code parses attempts" {
    var resp = try arrayOf(testing.allocator, &.{ "invalid_code", "3" });
    defer resp.deinit(testing.allocator);
    const outcome = try proto.parseVerifyOutcome(resp);
    try testing.expectEqual(@as(u8, 3), outcome.invalid_code);
}

test "parseVerifyOutcome success carries payload triple" {
    var resp = try arrayOf(testing.allocator, &.{ "success", "DASH", "CIPHER", "NONCE" });
    defer resp.deinit(testing.allocator);
    const outcome = try proto.parseVerifyOutcome(resp);
    try testing.expectEqualStrings("DASH", outcome.success.dashboard_public_key);
    try testing.expectEqualStrings("CIPHER", outcome.success.ciphertext);
    try testing.expectEqualStrings("NONCE", outcome.success.nonce);
}

test "parseVerifyOutcome replay shares the success payload shape" {
    var resp = try arrayOf(testing.allocator, &.{ "replay", "D", "C", "N" });
    defer resp.deinit(testing.allocator);
    const outcome = try proto.parseVerifyOutcome(resp);
    try testing.expectEqualStrings("D", outcome.replay.dashboard_public_key);
}

test "VerifyOutcome.dupe survives RespValue free — pins the borrowed-slice fix" {
    // The hazard: parseVerifyOutcome returns slices borrowed from `resp`.
    // If the caller frees `resp` before reading the outcome, those slices
    // dangle. testing.allocator scribbles 0xaa on free, so this test will
    // observe corrupted bytes if dupe is missing or wrong.
    var resp = try arrayOf(testing.allocator, &.{ "success", "DASH-KEY", "CIPHER-PAYLOAD", "NONCE-BYTES" });
    const borrowed = try proto.parseVerifyOutcome(resp);
    var owned = try borrowed.dupe(testing.allocator);
    defer owned.deinit(testing.allocator);

    // Free the RespValue first — this is the production lifecycle.
    resp.deinit(testing.allocator);

    // The owned outcome must still read the original bytes, not the
    // allocator scribble pattern.
    try testing.expectEqualStrings("DASH-KEY", owned.success.dashboard_public_key);
    try testing.expectEqualStrings("CIPHER-PAYLOAD", owned.success.ciphertext);
    try testing.expectEqualStrings("NONCE-BYTES", owned.success.nonce);
}

test "VerifyOutcome.dupe survives RespValue free — aborted reason variant" {
    var resp = try arrayOf(testing.allocator, &.{ "aborted", "rate_limit_exceeded" });
    const borrowed = try proto.parseVerifyOutcome(resp);
    var owned = try borrowed.dupe(testing.allocator);
    defer owned.deinit(testing.allocator);

    resp.deinit(testing.allocator);
    try testing.expectEqualStrings("rate_limit_exceeded", owned.aborted);
}

test "VerifyOutcome.dupe is identity for payload-less variants" {
    inline for (.{ "consumed", "expired", "missing", "not_approved", "rate_limited" }) |tag| {
        var resp = try arrayOf(testing.allocator, &.{tag});
        defer resp.deinit(testing.allocator);
        const borrowed = try proto.parseVerifyOutcome(resp);
        var owned = try borrowed.dupe(testing.allocator);
        defer owned.deinit(testing.allocator); // no-op; pin it doesn't crash.
        try testing.expect(std.meta.activeTag(owned) == std.meta.activeTag(borrowed));
    }
}

test "VerifyOutcome.dupe preserves invalid_code u8 attempts" {
    var resp = try arrayOf(testing.allocator, &.{ "invalid_code", "4" });
    defer resp.deinit(testing.allocator);
    const borrowed = try proto.parseVerifyOutcome(resp);
    var owned = try borrowed.dupe(testing.allocator);
    defer owned.deinit(testing.allocator);
    try testing.expectEqual(@as(u8, 4), owned.invalid_code);
}

test "VERIFY_AND_CONSUME_LUA embed is non-empty and references attempts" {
    try testing.expect(proto.VERIFY_AND_CONSUME_LUA.len > 100);
    try testing.expect(std.mem.indexOf(u8, proto.VERIFY_AND_CONSUME_LUA, "verification_attempts") != null);
}

test "APPROVE_LUA decodes JSON and writes verification_pending" {
    try testing.expect(std.mem.indexOf(u8, proto.APPROVE_LUA, "cjson.decode") != null);
    try testing.expect(std.mem.indexOf(u8, proto.APPROVE_LUA, "verification_pending") != null);
}
