const std = @import("std");
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
    TestExporter.install(TEST_CFG);
    try std.testing.expect(TestExporter.isInstalled());
    // Empty buffer (collect→null) → no POST; uninstall wakes the tick sleep and
    // joins cleanly, no hang.
    TestExporter.uninstall();
    try std.testing.expect(!TestExporter.isInstalled());
}

test "double install is a no-op — single-consumer guard (no orphaned second thread)" {
    TestExporter.install(TEST_CFG);
    // Second install while running must NOT spawn a second flush thread (two
    // consumers on one ring would corrupt it). The guard makes it a no-op.
    TestExporter.install(TEST_CFG);
    try std.testing.expect(TestExporter.isInstalled());
    // uninstall joins the single thread and completes (would hang/leak if a
    // second orphaned thread existed).
    TestExporter.uninstall();
    try std.testing.expect(!TestExporter.isInstalled());
}

test "test_exporter_testhooks: testSetInstalled/testClear toggle state without a thread" {
    TestExporter.testSetInstalled(TEST_CFG);
    try std.testing.expect(TestExporter.isInstalled());
    TestExporter.testClear();
    try std.testing.expect(!TestExporter.isInstalled());
}
