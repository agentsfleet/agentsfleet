//! Unit tests for `zombie_events_store` (filter parsers + cursor).
//! DB-backed listing tests live in `tests/integration/zombie_events_test.zig`.

const std = @import("std");
const filter_mod = @import("zombie_events_filter.zig");
const store = @import("zombie_events_store.zig");
const base64 = std.base64.url_safe_no_pad;

const testing = std.testing;

test "parseSince: durations relative to now" {
    const now: i64 = 1_745_568_000_000;
    try testing.expectEqual(now - 2 * 3_600_000, try filter_mod.parseSince("2h", now));
    try testing.expectEqual(now - 30 * 60_000, try filter_mod.parseSince("30m", now));
    try testing.expectEqual(now - 7 * 86_400_000, try filter_mod.parseSince("7d", now));
    try testing.expectEqual(now - 15 * 1_000, try filter_mod.parseSince("15s", now));
}

test "parseSince: rfc3339Z" {
    // 2026-04-25T08:00:00Z → 1777104000 seconds → 1777104000000 ms
    try testing.expectEqual(@as(i64, 1_777_104_000_000), try filter_mod.parseSince("2026-04-25T08:00:00Z", 0));
}

test "parseSince: rejects malformed input" {
    try testing.expectError(filter_mod.SinceError.InvalidSince, filter_mod.parseSince("", 0));
    try testing.expectError(filter_mod.SinceError.InvalidSince, filter_mod.parseSince("bogus", 0));
    try testing.expectError(filter_mod.SinceError.InvalidSince, filter_mod.parseSince("2h30m", 0));
    try testing.expectError(filter_mod.SinceError.InvalidSince, filter_mod.parseSince("-5h", 0));
    try testing.expectError(filter_mod.SinceError.InvalidSince, filter_mod.parseSince("2026-04-25T08:00:00", 0));
}

test "globToLike: star becomes percent" {
    const got = try filter_mod.globToLike(testing.allocator, "steer:*");
    defer testing.allocator.free(got);
    try testing.expectEqualStrings("steer:%", got);
}

test "globToLike: literal percent and underscore are escaped" {
    const got = try filter_mod.globToLike(testing.allocator, "100%_done");
    defer testing.allocator.free(got);
    try testing.expectEqualStrings("100\\%\\_done", got);
}

test "globToLike: exact match passes through" {
    const got = try filter_mod.globToLike(testing.allocator, "webhook:github");
    defer testing.allocator.free(got);
    try testing.expectEqualStrings("webhook:github", got);
}

test "makeCursor + parseCursor: round-trip" {
    const alloc = testing.allocator;
    const cursor = try filter_mod.makeCursor(alloc, 1_745_568_008_210, "1729874000000-0");
    defer alloc.free(cursor);
    const parsed = try filter_mod.parseCursor(alloc, cursor);
    defer alloc.free(parsed.event_id);
    try testing.expectEqual(@as(i64, 1_745_568_008_210), parsed.created_at);
    try testing.expectEqualStrings("1729874000000-0", parsed.event_id);
}

test "parseCursor: rejects empty event_id" {
    const alloc = testing.allocator;
    var buf: [base64.Encoder.calcSize("123456:".len)]u8 = undefined;
    _ = base64.Encoder.encode(&buf, "123456:");
    try testing.expectError(error.InvalidCursor, filter_mod.parseCursor(alloc, &buf));
}

test "parseCursor: rejects oversized event_id" {
    const alloc = testing.allocator;
    const big = "x" ** 129;
    const plain = "1:" ++ big;
    var buf: [base64.Encoder.calcSize(plain.len)]u8 = undefined;
    _ = base64.Encoder.encode(&buf, plain);
    try testing.expectError(error.InvalidCursor, filter_mod.parseCursor(alloc, &buf));
}

test "parseCursor: rejects non-base64" {
    try testing.expectError(error.InvalidCursor, filter_mod.parseCursor(testing.allocator, "!!not-valid!!"));
}

test "store re-exports resolve" {
    // Smoke: every public symbol the handlers will reach for compiles.
    _ = store.EventRow;
    _ = store.Filter;
    _ = store.makeCursor;
    _ = store.parseSince;
    _ = store.SinceError;
    _ = store.globToLike;
    _ = store.listForZombie;
    _ = store.listForWorkspace;
}
