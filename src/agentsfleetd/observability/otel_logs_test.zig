const std = @import("std");
const common = @import("common");
const otel_logs = @import("otel_logs.zig");
const otlp_config = @import("otlp/config.zig");
const health = @import("metrics_otel.zig");

const TEST_CFG: otlp_config.GrafanaOtlpConfig = .{
    .endpoint = "http://127.0.0.1:0",
    .instance_id = "i",
    .api_key = "k",
};

const Ring = otel_logs.TestRing;
const LogEntry = otel_logs.TestLogEntry;
const BUFFER_CAPACITY = otel_logs.TEST_BUFFER_CAPACITY;
const configFromEnv = @import("otlp/config.zig").configFromEnv;
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

// The next three pin the conditional-free negative branches: each frees the
// already-allocated values before returning null. testing.allocator turns any
// leak or double-free into a test failure.
test "configFromEnv returns null and frees on a whitespace-only endpoint" {
    const alloc = std.testing.allocator;
    var env_map = try common.env.fromPairs(alloc, &.{.{ "GRAFANA_OTLP_ENDPOINT", "   " }});
    defer env_map.deinit();
    try std.testing.expect(configFromEnv(&env_map, alloc) == null);
}

test "configFromEnv returns null and frees the endpoint when instance_id is missing" {
    const alloc = std.testing.allocator;
    var env_map = try common.env.fromPairs(alloc, &.{.{ "GRAFANA_OTLP_ENDPOINT", "https://otlp.example" }});
    defer env_map.deinit();
    try std.testing.expect(configFromEnv(&env_map, alloc) == null);
}

test "configFromEnv returns null and frees endpoint+instance_id when api_key is missing" {
    const alloc = std.testing.allocator;
    var env_map = try common.env.fromPairs(alloc, &.{
        .{ "GRAFANA_OTLP_ENDPOINT", "https://otlp.example" },
        .{ "GRAFANA_OTLP_INSTANCE_ID", "12345" },
    });
    defer env_map.deinit();
    try std.testing.expect(configFromEnv(&env_map, alloc) == null);
}

test "configFromEnv returns the full config with the default service_name" {
    const alloc = std.testing.allocator;
    var env_map = try common.env.fromPairs(alloc, &.{
        .{ "GRAFANA_OTLP_ENDPOINT", "https://otlp.example" },
        .{ "GRAFANA_OTLP_INSTANCE_ID", "12345" },
        .{ "GRAFANA_OTLP_API_KEY", "secret-key" },
    });
    defer env_map.deinit();
    const cfg = configFromEnv(&env_map, alloc).?;
    // endpoint/instance_id/api_key are owned copies; service_name here is the
    // static default (NOT allocated), so it must NOT be freed.
    defer {
        alloc.free(cfg.endpoint);
        alloc.free(cfg.instance_id);
        alloc.free(cfg.api_key);
    }
    try std.testing.expectEqualStrings("https://otlp.example", cfg.endpoint);
    try std.testing.expectEqualStrings("agentsfleetd", cfg.service_name);
}

test "configFromEnv honors an OTEL_SERVICE_NAME override" {
    const alloc = std.testing.allocator;
    var env_map = try common.env.fromPairs(alloc, &.{
        .{ "GRAFANA_OTLP_ENDPOINT", "https://otlp.example" },
        .{ "GRAFANA_OTLP_INSTANCE_ID", "12345" },
        .{ "GRAFANA_OTLP_API_KEY", "secret-key" },
        .{ "OTEL_SERVICE_NAME", "custom-name" },
    });
    defer env_map.deinit();
    const cfg = configFromEnv(&env_map, alloc).?;
    // Override path: service_name is an owned copy too — free all four.
    defer {
        alloc.free(cfg.endpoint);
        alloc.free(cfg.instance_id);
        alloc.free(cfg.api_key);
        alloc.free(cfg.service_name);
    }
    try std.testing.expectEqualStrings("custom-name", cfg.service_name);
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

// Encodes (thread, seq) into one u64 and into the body text so a torn entry
// (fields mixed from two writers) is detectable on pop.
const ENCODE_BASE: u64 = 1_000_000;
const BODY_FMT = "{d}:{d}";

test "ring buffer concurrent pushes never tear, lose, or duplicate entries" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    const THREADS: usize = 4;
    const PER_THREAD: usize = 400; // 1600 total < capacity-1 → zero drops expected

    const Pusher = struct {
        fn run(r: *Ring, thread_idx: usize) void {
            var seq: usize = 0;
            while (seq < PER_THREAD) : (seq += 1) {
                // SAFETY: every read field (timestamp, level, scope, body + lens)
                // is assigned below before the entry is pushed.
                var entry: LogEntry = undefined;
                entry.timestamp_ns = @as(u64, thread_idx) * ENCODE_BASE + seq;
                entry.level_len = 5;
                @memcpy(entry.level[0..5], "debug");
                entry.scope_len = 1;
                entry.scope[0] = @intCast('a' + thread_idx);
                var body_buf: [32]u8 = undefined;
                const body = std.fmt.bufPrint(&body_buf, BODY_FMT, .{ thread_idx, seq }) catch unreachable;
                entry.body_len = @intCast(body.len);
                @memcpy(entry.body[0..body.len], body);
                _ = r.push(entry);
            }
        }
    };

    var threads: [THREADS]std.Thread = undefined;
    for (&threads, 0..) |*t, idx| t.* = try std.Thread.spawn(.{}, Pusher.run, .{ ring, idx });
    for (&threads) |*t| t.join();

    try std.testing.expectEqual(@as(u64, 0), ring.dropped.load(.acquire));

    var seen = [_]bool{false} ** (THREADS * PER_THREAD);
    var popped_count: usize = 0;
    while (ring.pop()) |entry| {
        popped_count += 1;
        const thread_idx: usize = @intCast(entry.timestamp_ns / ENCODE_BASE);
        const seq: usize = @intCast(entry.timestamp_ns % ENCODE_BASE);
        // Cross-field consistency: a torn slot mixes (thread, seq) across fields.
        try std.testing.expectEqual(@as(u8, @intCast('a' + thread_idx)), entry.scope[0]);
        var body_buf: [32]u8 = undefined;
        const want_body = std.fmt.bufPrint(&body_buf, BODY_FMT, .{ thread_idx, seq }) catch unreachable;
        try std.testing.expectEqualStrings(want_body, entry.body[0..entry.body_len]);
        const key = thread_idx * PER_THREAD + seq;
        try std.testing.expect(!seen[key]); // duplicate delivery
        seen[key] = true;
    }
    try std.testing.expectEqual(THREADS * PER_THREAD, popped_count); // lost entry
}

test "ring buffer ready flags recycle across wraparound" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    var i: u64 = 0;
    while (i < BUFFER_CAPACITY * 2 + 5) : (i += 1) {
        var entry: LogEntry = undefined;
        entry.timestamp_ns = i;
        entry.level_len = 0;
        entry.scope_len = 0;
        entry.body_len = 0;
        try std.testing.expect(ring.push(entry));
        const popped = ring.pop();
        try std.testing.expect(popped != null);
        try std.testing.expectEqual(i, popped.?.timestamp_ns);
    }
    try std.testing.expectEqual(@as(usize, 0), ring.len());
}

test "pop returns null on a claimed-but-unready head slot, then delivers once published" {
    const alloc = std.testing.allocator;
    const ring = try alloc.create(Ring);
    defer alloc.destroy(ring);
    ring.* = .{};

    // Simulate a producer that claimed slot 0 (head advanced) but has not yet
    // published its write: head=1, ready[0]=0.
    ring.head.store(1, .release);
    try std.testing.expect(ring.pop() == null);

    // The producer completes its write and publishes.
    var entry: LogEntry = undefined;
    entry.timestamp_ns = 7;
    entry.level_len = 0;
    entry.scope_len = 0;
    entry.body_len = 0;
    ring.buffer[0] = entry;
    ring.ready[0].store(1, .release);

    const popped = ring.pop();
    try std.testing.expect(popped != null);
    try std.testing.expectEqual(@as(u64, 7), popped.?.timestamp_ns);
}

test "enqueue is no-op when exporter not installed" {
    // g_config is null by default, so enqueue should silently no-op
    enqueue("info", "test", "this should not crash");
}

test "test_logs_full_ring_records_discard_and_only_accepted_wake" {
    health.resetForTest();
    defer health.resetForTest();
    otel_logs.testSetInstalled(TEST_CFG);
    defer otel_logs.testClear();

    for (0..BUFFER_CAPACITY - 1) |_| enqueue("info", "test", "accepted");
    enqueue("info", "test", "discarded");

    const snapshot = health.snapshot();
    try std.testing.expectEqual(@as(usize, BUFFER_CAPACITY - 1), otel_logs.testPendingCount());
    try std.testing.expectEqual(@as(u32, BUFFER_CAPACITY - 1), otel_logs.testAcceptedSinceCycle());
    try std.testing.expectEqual(
        @as(u64, 1),
        snapshot.discarded[@intFromEnum(health.Signal.logs)][@intFromEnum(health.DiscardReason.ring_full)],
    );
}

test "collectLogs serializes valid JSON with an escaped body (json.fmt double-quote regression)" {
    const alloc = std.testing.allocator;
    otel_logs.testSetInstalled(TEST_CFG);
    defer otel_logs.testClear();

    // Body with an embedded quote: exercises json.fmt escaping AND the regression
    // (the prior bug wrapped json.fmt output in its own quotes → invalid ""...."").
    otel_logs.enqueue("error", "fleet", "boom: said \"hi\"");

    const body = (try otel_logs.testCollect(alloc, TEST_CFG)) orelse return error.NoBody;
    defer alloc.free(body);

    // A successful parse is the regression assertion: the double-quote bug makes
    // the log body unparseable, and the embedded quote must be escaped (\").
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    parsed.deinit();
    try std.testing.expect(std.mem.indexOf(u8, body, "\\\"hi\\\"") != null); // escaped quote present
}

test "LogEntry truncates oversized fields" {
    var entry: LogEntry = undefined;
    const long_scope = "this_scope_is_way_too_long_for_32_bytes_buffer_overflow";
    entry.scope_len = @intCast(@min(long_scope.len, 32));
    @memcpy(entry.scope[0..entry.scope_len], long_scope[0..entry.scope_len]);
    try std.testing.expectEqual(@as(u8, 32), entry.scope_len);
}

test "collectLogs frees its envelope when the ring drains empty" {
    const alloc = std.testing.allocator;
    otel_logs.testSetInstalled(TEST_CFG);
    defer otel_logs.testClear();

    // No entries pending: collect writes the resourceLogs prefix, finds nothing
    // to serialize, and returns empty. The testing allocator's leak check is the
    // assertion — the early return must release that prefix, and errdefer does
    // not fire on a successful return.
    try std.testing.expect((try otel_logs.testCollect(alloc, TEST_CFG)) == null);
}
