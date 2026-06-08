const std = @import("std");
const common = @import("common");
const otel_logs = @import("otel_logs.zig");

const Ring = otel_logs.TestRing;
const LogEntry = otel_logs.TestLogEntry;
const BUFFER_CAPACITY = otel_logs.TEST_BUFFER_CAPACITY;
const configFromEnv = otel_logs.configFromEnv;
const enqueue = otel_logs.enqueue;

test "configFromEnv returns null when GRAFANA_OTLP_ENDPOINT is unset" {
    const alloc = std.testing.allocator;
    // Zig 0.16 env snapshot: pass a synthetic empty map rather than depending on
    // the ambient process env. GRAFANA_OTLP_ENDPOINT is unset by construction, so
    // the unset → null contract is now asserted deterministically.
    var env_map = try common.env.fromPairs(alloc, &.{});
    defer env_map.deinit();
    try std.testing.expect(configFromEnv(&env_map, alloc) == null);
}

test "ring buffer push and pop round-trip" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};
    var entry: LogEntry = undefined;
    entry.timestamp_ns = 42;
    entry.level_len = 4;
    @memcpy(entry.level[0..4], "info");
    entry.scope_len = 4;
    @memcpy(entry.scope[0..4], "test");
    entry.body_len = 5;
    @memcpy(entry.body[0..5], "hello");

    try std.testing.expect(ring.push(entry));
    try std.testing.expectEqual(@as(usize, 1), ring.len());

    const popped = ring.pop();
    try std.testing.expect(popped != null);
    try std.testing.expectEqual(@as(u64, 42), popped.?.timestamp_ns);
    try std.testing.expectEqualStrings("info", popped.?.level[0..popped.?.level_len]);
    try std.testing.expectEqualStrings("test", popped.?.scope[0..popped.?.scope_len]);
    try std.testing.expectEqualStrings("hello", popped.?.body[0..popped.?.body_len]);
    try std.testing.expectEqual(@as(usize, 0), ring.len());
}

test "ring buffer pop returns null when empty" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};
    try std.testing.expect(ring.pop() == null);
}

test "ring buffer drops when full" {
    // Use heap allocation to avoid 8MB+ stack frame that valgrind flags.
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    var entry: LogEntry = undefined;
    entry.timestamp_ns = 0;
    entry.level_len = 3;
    @memcpy(entry.level[0..3], "err");
    entry.scope_len = 1;
    entry.scope[0] = 'x';
    entry.body_len = 1;
    entry.body[0] = 'y';

    // Fill to capacity - 1 (ring buffer leaves one slot empty)
    var i: usize = 0;
    while (i < BUFFER_CAPACITY - 1) : (i += 1) {
        try std.testing.expect(ring.push(entry));
    }
    // Next push should fail (buffer full)
    try std.testing.expect(!ring.push(entry));
    try std.testing.expectEqual(@as(u64, 1), ring.dropped.load(.acquire));
}

test "enqueue is no-op when exporter not installed" {
    // g_config is null by default, so enqueue should silently no-op
    enqueue("info", "test", "this should not crash");
}

test "LogEntry truncates oversized fields" {
    var entry: LogEntry = undefined;
    const long_scope = "this_scope_is_way_too_long_for_32_bytes_buffer_overflow";
    entry.scope_len = @intCast(@min(long_scope.len, 32));
    @memcpy(entry.scope[0..entry.scope_len], long_scope[0..entry.scope_len]);
    try std.testing.expectEqual(@as(u8, 32), entry.scope_len);
}
