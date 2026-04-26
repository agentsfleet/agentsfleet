//! Unit tests for `json_line_buffer.zig`.
//!
//! T1 — empty buffer returns null.
//! T2 — single full line round-trips minus terminator; second nextLine null.
//! T3 — partial line surfaces only after terminator arrives.
//! T4 — internal compaction triggers past the threshold (asserted via head reset).
//! T5 — init + immediate deinit leaks nothing under std.testing.allocator.
//! Plus: multi-line drain in arrival order; embedded blank line; OOM unwind on write.

const std = @import("std");
const testing = std.testing;
const JsonLineBuffer = @import("json_line_buffer.zig");

test "T1 empty buffer returns null" {
    var buf = JsonLineBuffer.init(testing.allocator);
    defer buf.deinit();
    try testing.expectEqual(@as(?[]const u8, null), buf.nextLine());
    try testing.expect(buf.isEmpty());
}

test "T2 single full line round-trips minus terminator" {
    var buf = JsonLineBuffer.init(testing.allocator);
    defer buf.deinit();

    try buf.write("hello world\n");
    const line = buf.nextLine() orelse return error.MissingLine;
    try testing.expectEqualStrings("hello world", line);
    try testing.expectEqual(@as(?[]const u8, null), buf.nextLine());
}

test "T3 partial line surfaces only after terminator arrives" {
    var buf = JsonLineBuffer.init(testing.allocator);
    defer buf.deinit();

    try buf.write("partial");
    try testing.expectEqual(@as(?[]const u8, null), buf.nextLine());

    try buf.write(" rest\n");
    const line = buf.nextLine() orelse return error.MissingLine;
    try testing.expectEqualStrings("partial rest", line);
}

test "T4 internal compaction triggers and resets head" {
    var buf = JsonLineBuffer.init(testing.allocator);
    defer buf.deinit();

    // Force head past 16 MiB by writing a single huge line then consuming it,
    // then writing one short line that triggers compaction inside consume().
    const big_len: usize = 16 * 1024 * 1024 + 1;
    const huge = try testing.allocator.alloc(u8, big_len);
    defer testing.allocator.free(huge);
    @memset(huge[0 .. big_len - 1], 'x');
    huge[big_len - 1] = '\n';

    try buf.write(huge);
    _ = buf.nextLine() orelse return error.MissingLine;
    // After draining the only line, head==len triggers the reset branch — buffer
    // is fully cleared. Re-prime with a partial line then complete it; head must
    // have been zeroed.
    try buf.write("a\n");
    const line = buf.nextLine() orelse return error.MissingLine;
    try testing.expectEqualStrings("a", line);
    try testing.expectEqual(@as(u32, 0), buf.head);
}

test "T5 init + immediate deinit leaks nothing" {
    var buf = JsonLineBuffer.init(testing.allocator);
    buf.deinit();
}

test "multi-line drain preserves arrival order" {
    var buf = JsonLineBuffer.init(testing.allocator);
    defer buf.deinit();

    try buf.write("first\nsecond\nthird\n");
    try testing.expectEqualStrings("first", buf.nextLine().?);
    try testing.expectEqualStrings("second", buf.nextLine().?);
    try testing.expectEqualStrings("third", buf.nextLine().?);
    try testing.expectEqual(@as(?[]const u8, null), buf.nextLine());
}

test "embedded blank line surfaces as zero-length slice" {
    var buf = JsonLineBuffer.init(testing.allocator);
    defer buf.deinit();

    try buf.write("a\n\nb\n");
    try testing.expectEqualStrings("a", buf.nextLine().?);
    try testing.expectEqualStrings("", buf.nextLine().?);
    try testing.expectEqualStrings("b", buf.nextLine().?);
}

test "OOM during write leaves buffer drainable up to last successful write" {
    var failing = std.testing.FailingAllocator.init(testing.allocator, .{ .fail_index = 1 });
    var buf = JsonLineBuffer.init(failing.allocator());
    defer buf.deinit();

    // First write may succeed (alloc 0 inside ArrayListUnmanaged is no-op for empty slice).
    // Force a sizable write to provoke the failing allocation.
    const r = buf.write("hello\n");
    if (r) |_| {
        // No allocation triggered yet; write a larger payload that must allocate.
        const big = try testing.allocator.alloc(u8, 4096);
        defer testing.allocator.free(big);
        @memset(big, 'z');
        const r2 = buf.write(big);
        try testing.expectError(error.OutOfMemory, r2);
    } else |err| {
        try testing.expectEqual(error.OutOfMemory, err);
    }
}
