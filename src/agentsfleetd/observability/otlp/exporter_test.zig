const std = @import("std");
const common = @import("common");
const exporter = @import("exporter.zig");
const Client = @import("Client.zig");
const config = @import("config.zig");
const health = @import("../metrics_otel.zig");

const TEST_CFG: config.GrafanaOtlpConfig = .{
    .endpoint = "http://127.0.0.1:0",
    .instance_id = "i",
    .api_key = "k",
};
const TEST_BATCH_SIZE: usize = 2;
/// Mirrors the shipped log/trace wake threshold, so the batching proof runs
/// against the real coalescing shape rather than a toy value.
const TEST_WAKE_THRESHOLD: u32 = 50;
const PRODUCER_THREAD_COUNT: usize = 100;

fn emptyCollect(
    alloc: std.mem.Allocator,
    cfg: config.GrafanaOtlpConfig,
    max_entries: usize,
) exporter.CollectResult {
    _ = alloc;
    _ = cfg;
    _ = max_entries;
    return .empty;
}

fn emptyPending() usize {
    return 0;
}

const EmptyExporter = exporter.Exporter(.{
    .signal = .metrics,
    .path = "/v1/test-empty",
    .scope = .otel_metrics,
    .collect = emptyCollect,
    .pending_count = emptyPending,
    .flush_interval_ms = 100,
    .wake_threshold = 2,
});

test "test_exporter_lifecycle: install spawns the flush thread and uninstall joins" {
    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();
    const io = threaded.io();

    try std.testing.expect(!EmptyExporter.isInstalled());
    try std.testing.expectEqual(EmptyExporter.InstallOutcome.installed, EmptyExporter.install(io, TEST_CFG));
    try std.testing.expect(EmptyExporter.isInstalled());
    EmptyExporter.uninstall();
    try std.testing.expect(!EmptyExporter.isInstalled());
}

test "double install keeps exactly one consumer" {
    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();
    const io = threaded.io();

    try std.testing.expectEqual(EmptyExporter.InstallOutcome.installed, EmptyExporter.install(io, TEST_CFG));
    try std.testing.expectEqual(EmptyExporter.InstallOutcome.already_running, EmptyExporter.install(io, TEST_CFG));
    EmptyExporter.uninstall();
}

const RacingInstall = struct {
    io: std.Io,
    outcome: EmptyExporter.InstallOutcome = .spawn_failed,

    fn run(self: *RacingInstall) void {
        self.outcome = EmptyExporter.install(self.io, TEST_CFG);
    }
};

test "test_exporter_double_install_one_thread: racing installs have one winner" {
    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();
    const io = threaded.io();
    var a = RacingInstall{ .io = io };
    var b = RacingInstall{ .io = io };
    const thread_a = try std.Thread.spawn(.{}, RacingInstall.run, .{&a});
    const thread_b = try std.Thread.spawn(.{}, RacingInstall.run, .{&b});
    thread_a.join();
    thread_b.join();

    const a_won = a.outcome == .installed and b.outcome == .already_running;
    const b_won = b.outcome == .installed and a.outcome == .already_running;
    try std.testing.expect(a_won or b_won);
    EmptyExporter.uninstall();
}

const BatchingExporter = exporter.Exporter(.{
    .signal = .logs,
    .path = "/v1/test-batching",
    .scope = .otel_logs,
    .collect = emptyCollect,
    .pending_count = emptyPending,
    .flush_interval_ms = 100,
    .wake_threshold = TEST_WAKE_THRESHOLD,
});

test "test_exporter_preserves_batching_and_coalesces_wakes" {
    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();
    const io = threaded.io();

    BatchingExporter.testSetInstalled(io, TEST_CFG);
    defer BatchingExporter.testClear();

    // Below the threshold nothing wakes the consumer: low-rate traffic keeps
    // batching instead of degrading into one HTTP request per entry.
    for (0..TEST_WAKE_THRESHOLD - 1) |_| BatchingExporter.notifyAccepted();
    try std.testing.expectEqual(TEST_WAKE_THRESHOLD - 1, BatchingExporter.testAcceptedSinceCycle());
    try std.testing.expect(!BatchingExporter.testTakeWakeSignal());

    // Crossing the threshold arms the wake exactly once ...
    BatchingExporter.notifyAccepted();
    try std.testing.expect(BatchingExporter.testTakeWakeSignal());

    // ... and every push after it folds into that pending wake rather than
    // re-arming, so a burst costs one wake, not one per entry.
    BatchingExporter.notifyAccepted();
    BatchingExporter.notifyAccepted();
    try std.testing.expect(!BatchingExporter.testTakeWakeSignal());

    // A cycle clears the accepted-push tally so the next threshold is counted
    // from zero rather than from the drained backlog.
    BatchingExporter.testFlushCycle();
    try std.testing.expectEqual(@as(u32, 0), BatchingExporter.testAcceptedSinceCycle());

    // 100 concurrent producers every one of which returns: the accepted-push
    // path is an atomic add plus an idempotent event set, so no producer waits
    // on the consumer and no increment is lost.
    var threads: [PRODUCER_THREAD_COUNT]std.Thread = undefined;
    for (&threads) |*t| t.* = try std.Thread.spawn(.{}, BatchingExporter.notifyAccepted, .{});
    for (threads) |t| t.join();
    try std.testing.expectEqual(
        @as(u32, PRODUCER_THREAD_COUNT),
        BatchingExporter.testAcceptedSinceCycle(),
    );
}

var g_pending = std.atomic.Value(usize).init(0);
var g_collect_calls = std.atomic.Value(usize).init(0);
var g_post_calls = std.atomic.Value(usize).init(0);
var g_add_during_first_post = std.atomic.Value(usize).init(0);

fn batchPending() usize {
    return g_pending.load(.acquire);
}

fn batchCollect(
    alloc: std.mem.Allocator,
    cfg: config.GrafanaOtlpConfig,
    max_entries: usize,
) exporter.CollectResult {
    _ = alloc;
    _ = cfg;
    const available = g_pending.load(.acquire);
    const removed = @min(available, @min(max_entries, TEST_BATCH_SIZE));
    if (removed == 0) return .empty;
    g_pending.store(available - removed, .release);
    _ = g_collect_calls.fetchAdd(1, .monotonic);
    return .{ .ready = .{
        .body = "{}",
        .removed_count = removed,
        .export_count = removed,
    } };
}

fn acceptedPost(
    client: *Client,
    alloc: std.mem.Allocator,
    cfg: config.GrafanaOtlpConfig,
    path: []const u8,
    body: []const u8,
    deadline_ns: i96,
) anyerror!Client.ExportResult {
    _ = client;
    _ = alloc;
    _ = cfg;
    _ = path;
    _ = body;
    _ = deadline_ns;
    const prior = g_post_calls.fetchAdd(1, .monotonic);
    if (prior == 0) {
        _ = g_pending.fetchAdd(g_add_during_first_post.swap(0, .acq_rel), .acq_rel);
    }
    return .accepted;
}

const DrainExporter = exporter.Exporter(.{
    .signal = .traces,
    .path = "/v1/test-drain",
    .scope = .otel_traces,
    .collect = batchCollect,
    .pending_count = batchPending,
    .post = acceptedPost,
    .flush_interval_ms = 100,
});

fn resetBatchState(pending: usize) void {
    g_pending.store(pending, .release);
    g_collect_calls.store(0, .release);
    g_post_calls.store(0, .release);
    g_add_during_first_post.store(0, .release);
}

test "test_exporter_drains_cycle_start_backlog" {
    resetBatchState(5);
    g_add_during_first_post.store(3, .release);
    DrainExporter.testSetInstalled(common.globalIo(), TEST_CFG);
    defer DrainExporter.testClear();

    DrainExporter.testFlushCycle();

    try std.testing.expectEqual(@as(usize, 3), g_pending.load(.acquire));
    try std.testing.expectEqual(@as(usize, 3), g_collect_calls.load(.acquire));
    try std.testing.expectEqual(@as(usize, 3), g_post_calls.load(.acquire));
}

test "test_exporter_stop_wakes_drains_and_joins" {
    resetBatchState(5);
    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();

    try std.testing.expectEqual(
        DrainExporter.InstallOutcome.installed,
        DrainExporter.install(threaded.io(), TEST_CFG),
    );
    DrainExporter.uninstall();

    try std.testing.expectEqual(@as(usize, 0), g_pending.load(.acquire));
    try std.testing.expectEqual(@as(usize, 3), g_post_calls.load(.acquire));
}

fn rejectedPost(
    client: *Client,
    alloc: std.mem.Allocator,
    cfg: config.GrafanaOtlpConfig,
    path: []const u8,
    body: []const u8,
    deadline_ns: i96,
) anyerror!Client.ExportResult {
    _ = client;
    _ = alloc;
    _ = cfg;
    _ = path;
    _ = body;
    _ = deadline_ns;
    _ = g_post_calls.fetchAdd(1, .monotonic);
    return error.OtlpExportRejected;
}

const RejectingExporter = exporter.Exporter(.{
    .signal = .logs,
    .path = "/v1/test-rejected",
    .scope = .otel_logs,
    .collect = batchCollect,
    .pending_count = batchPending,
    .post = rejectedPost,
});

test "test_exporter_never_retries_destructive_batch" {
    health.resetForTest();
    defer health.resetForTest();
    resetBatchState(TEST_BATCH_SIZE);
    RejectingExporter.testSetInstalled(common.globalIo(), TEST_CFG);
    defer RejectingExporter.testClear();

    RejectingExporter.testFlushCycle();

    const snapshot = health.snapshot();
    try std.testing.expectEqual(@as(usize, 1), g_post_calls.load(.acquire));
    try std.testing.expectEqual(@as(usize, 0), g_pending.load(.acquire));
    try std.testing.expectEqual(
        @as(u64, TEST_BATCH_SIZE),
        snapshot.discarded[@intFromEnum(health.Signal.logs)][@intFromEnum(health.DiscardReason.export_rejected)],
    );
}

fn serializeFailureCollect(
    alloc: std.mem.Allocator,
    cfg: config.GrafanaOtlpConfig,
    max_entries: usize,
) exporter.CollectResult {
    _ = alloc;
    _ = cfg;
    const available = g_pending.load(.acquire);
    const removed = @min(available, max_entries);
    g_pending.store(available - removed, .release);
    return .{ .serialize_failed = removed };
}

const SerializeFailureExporter = exporter.Exporter(.{
    .signal = .metrics,
    .path = "/v1/test-serialize",
    .scope = .otel_metrics,
    .collect = serializeFailureCollect,
    .pending_count = batchPending,
    .post = acceptedPost,
});

test "test_exporter_serialization_failure_counts_local_discards" {
    health.resetForTest();
    defer health.resetForTest();
    resetBatchState(7);
    SerializeFailureExporter.testSetInstalled(common.globalIo(), TEST_CFG);
    defer SerializeFailureExporter.testClear();

    SerializeFailureExporter.testFlushCycle();

    const snapshot = health.snapshot();
    try std.testing.expectEqual(@as(usize, 0), g_pending.load(.acquire));
    try std.testing.expectEqual(@as(usize, 0), g_post_calls.load(.acquire));
    try std.testing.expectEqual(
        @as(u64, 7),
        snapshot.discarded[@intFromEnum(health.Signal.metrics)][@intFromEnum(health.DiscardReason.serialize_failed)],
    );
}
