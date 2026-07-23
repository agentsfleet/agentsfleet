const std = @import("std");
const common = @import("common");
const exporter = @import("exporter.zig");
const config = @import("config.zig");

// A throwaway Exporter instantiation with a no-op collect — distinct module
// state from the real signal exporters (one static set per comptime hooks).
fn testCollect(alloc: std.mem.Allocator, cfg: config.GrafanaOtlpConfig) !?[]const u8 {
    _ = alloc;
    _ = cfg;
    return null; // nothing to send → flush never POSTs
}
fn testPending() bool {
    return false;
}

const TestExporter = exporter.Exporter(.{
    .path = "/v1/test",
    .scope = .otel_metrics,
    .collect = testCollect,
    .pending = testPending,
    .flush_interval_ms = 100,
});

const TEST_CFG: config.GrafanaOtlpConfig = .{
    .endpoint = "http://127.0.0.1:0",
    .instance_id = "i",
    .api_key = "k",
};

test "test_exporter_lifecycle: install spawns the flush thread, uninstall joins within a tick" {
    try std.testing.expect(!TestExporter.isInstalled());
    try std.testing.expectEqual(TestExporter.InstallOutcome.installed, TestExporter.install(common.globalIo(), TEST_CFG));
    try std.testing.expect(TestExporter.isInstalled());
    // Empty buffer (collect→null) → no POST; uninstall wakes the tick sleep and
    // joins cleanly, no hang.
    TestExporter.uninstall();
    try std.testing.expect(!TestExporter.isInstalled());
}

test "double install loses the claim — single-consumer guard (no orphaned second thread)" {
    try std.testing.expectEqual(TestExporter.InstallOutcome.installed, TestExporter.install(common.globalIo(), TEST_CFG));
    // Second install while running must NOT spawn a second flush thread (two
    // consumers on one ring would corrupt it, and the overwritten handle
    // would never be joined).
    try std.testing.expectEqual(TestExporter.InstallOutcome.already_running, TestExporter.install(common.globalIo(), TEST_CFG));
    try std.testing.expect(TestExporter.isInstalled());
    // uninstall joins the single thread and completes (would hang/leak if a
    // second orphaned thread existed).
    TestExporter.uninstall();
    try std.testing.expect(!TestExporter.isInstalled());
}

/// Racing installer for the atomic-claim test: records its outcome.
const RacingInstall = struct {
    fn run(outcome: *TestExporter.InstallOutcome) void {
        outcome.* = TestExporter.install(common.globalIo(), TEST_CFG);
    }
};

test "test_exporter_double_install_one_thread: racing installs — exactly one claim wins" {
    var a = TestExporter.InstallOutcome.spawn_failed;
    var b = TestExporter.InstallOutcome.spawn_failed;
    const ta = try std.Thread.spawn(.{}, RacingInstall.run, .{&a});
    const tb = try std.Thread.spawn(.{}, RacingInstall.run, .{&b});
    ta.join();
    tb.join();
    // Exactly one .installed; the loser observed .already_running — the
    // pre-fix check-then-act let BOTH through, orphaning a thread handle.
    const a_won = a == .installed and b == .already_running;
    const b_won = b == .installed and a == .already_running;
    try std.testing.expect(a_won or b_won);
    TestExporter.uninstall();
    try std.testing.expect(!TestExporter.isInstalled());
}

test "test_exporter_testhooks: testSetInstalled/testClear toggle state without a thread" {
    TestExporter.testSetInstalled(TEST_CFG);
    try std.testing.expect(TestExporter.isInstalled());
    TestExporter.testClear();
    try std.testing.expect(!TestExporter.isInstalled());
}

// ── Drain-on-shutdown: uninstall's join runs the flush thread's drain loop
//    until the ring is empty. This is the one lifecycle leg the tests above did
//    not pin (they buffer nothing); the other legs — spawn/join, single-thread
//    claim, racing install — are covered above, so this extends that suite
//    rather than duplicating it. ──────────────────────────────────────────────
var g_drain_ring = std.atomic.Value(usize).init(0);

fn drainCollect(alloc: std.mem.Allocator, cfg: config.GrafanaOtlpConfig) !?[]const u8 {
    _ = alloc;
    _ = cfg;
    // A real collect consumes the pending buffer into the body; here we drain
    // the fake ring and return null so the flush never touches the network.
    g_drain_ring.store(0, .release);
    return null;
}
fn drainPending() bool {
    return g_drain_ring.load(.acquire) > 0;
}

const DrainExporter = exporter.Exporter(.{
    .path = "/v1/drain",
    .scope = .otel_traces,
    .collect = drainCollect,
    .pending = drainPending,
    .flush_interval_ms = 100,
});

const DRAIN_PENDING_SEED: usize = 5;

test "exporter uninstall drains the ring before the flush thread joins" {
    g_drain_ring.store(DRAIN_PENDING_SEED, .release); // buffer pending entries
    try std.testing.expectEqual(DrainExporter.InstallOutcome.installed, DrainExporter.install(common.globalIo(), TEST_CFG));
    // uninstall clears g_running, wakes the tick sleep, and joins the flush
    // thread — whose shutdown-drain loop runs flushOnce while pending() is true,
    // so the ring is empty by the time join returns. The join is the barrier, so
    // the post-join read needs no polling.
    DrainExporter.uninstall();
    try std.testing.expectEqual(@as(usize, 0), g_drain_ring.load(.acquire));
    try std.testing.expect(!DrainExporter.isInstalled());
}
