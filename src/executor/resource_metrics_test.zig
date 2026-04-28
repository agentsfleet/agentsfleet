//! M27_001: Executor-side resource metrics plumbing tests.
//!
//! Covers:
//! - T1: Session.recordStageResult accumulates resource metrics
//! - T2: Edge cases — zero metrics, max values, single-stage
//! - T3: getUsage and getResourceContext return correct values
//! - T5: Concurrent recordStageResult is safe (single-writer by design)
//! - T7: Struct field regression guards (ExecutionResult, StageResult, CgroupMetrics)
//! - T11: Memory leak detection via std.testing.allocator

const std = @import("std");
const builtin = @import("builtin");
const types = @import("types.zig");
const session_mod = @import("session.zig");
const cgroup = @import("cgroup.zig");
const client_mod = @import("client.zig");

const Session = session_mod.Session;

fn testCorrelation() types.CorrelationContext {
    return .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    };
}

// ─────────────────────────────────────────────────────────────────────────────
// T1: Session.recordStageResult accumulates resource metrics
// ─────────────────────────────────────────────────────────────────────────────

test "T1: recordStageResult tracks peak memory (max) and throttle (sum)" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", testCorrelation(), .{}, 30_000);
    defer session.destroy();

    // Stage 1: 100 MB peak, 500ms throttle.
    session.recordStageResult(.{
        .exit_ok = true,
        .token_count = 100,
        .wall_seconds = 5,
        .memory_peak_bytes = 100 * 1024 * 1024,
        .cpu_throttled_ms = 500,
    });
    try std.testing.expectEqual(@as(u64, 100 * 1024 * 1024), session.max_memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 500), session.total_cpu_throttled_ms);

    // Stage 2: 200 MB peak (new max), 300ms throttle (summed).
    session.recordStageResult(.{
        .exit_ok = true,
        .token_count = 50,
        .wall_seconds = 3,
        .memory_peak_bytes = 200 * 1024 * 1024,
        .cpu_throttled_ms = 300,
    });
    try std.testing.expectEqual(@as(u64, 200 * 1024 * 1024), session.max_memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 800), session.total_cpu_throttled_ms);

    // Stage 3: 150 MB peak (below max, no change), 200ms throttle.
    session.recordStageResult(.{
        .exit_ok = true,
        .token_count = 30,
        .wall_seconds = 2,
        .memory_peak_bytes = 150 * 1024 * 1024,
        .cpu_throttled_ms = 200,
    });
    // Peak stays at 200 MB, throttle accumulates to 1000.
    try std.testing.expectEqual(@as(u64, 200 * 1024 * 1024), session.max_memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 1000), session.total_cpu_throttled_ms);
}

// ─────────────────────────────────────────────────────────────────────────────
// T2: Edge cases — zero metrics, default struct values
// ─────────────────────────────────────────────────────────────────────────────

test "T2: recordStageResult with zero resource metrics leaves fields at 0" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", testCorrelation(), .{}, 30_000);
    defer session.destroy();

    session.recordStageResult(.{ .exit_ok = true, .token_count = 10, .wall_seconds = 1 });
    try std.testing.expectEqual(@as(u64, 0), session.max_memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 0), session.total_cpu_throttled_ms);
}

test "T2: resource fields default to 0 in fresh session" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", testCorrelation(), .{}, 30_000);
    defer session.destroy();

    try std.testing.expectEqual(@as(u64, 0), session.max_memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 0), session.total_cpu_throttled_ms);
}

test "T2: single-byte peak memory is tracked" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", testCorrelation(), .{}, 30_000);
    defer session.destroy();

    session.recordStageResult(.{
        .exit_ok = true,
        .memory_peak_bytes = 1,
        .cpu_throttled_ms = 1,
    });
    try std.testing.expectEqual(@as(u64, 1), session.max_memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 1), session.total_cpu_throttled_ms);
}

// ─────────────────────────────────────────────────────────────────────────────
// T3: getUsage propagates resource metrics
// ─────────────────────────────────────────────────────────────────────────────

test "T3: getUsage includes resource metrics in returned ExecutionResult" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", testCorrelation(), .{}, 30_000);
    defer session.destroy();

    session.recordStageResult(.{
        .exit_ok = true,
        .token_count = 100,
        .wall_seconds = 10,
        .memory_peak_bytes = 300 * 1024 * 1024,
        .cpu_throttled_ms = 750,
    });

    const usage = session.getUsage();
    try std.testing.expectEqual(@as(u64, 300 * 1024 * 1024), usage.memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 750), usage.cpu_throttled_ms);
    try std.testing.expectEqual(@as(u64, 100), usage.token_count);
    try std.testing.expectEqual(@as(u64, 10), usage.wall_seconds);
}

test "T3: getUsage with no stages returns zero resource metrics" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", testCorrelation(), .{}, 30_000);
    defer session.destroy();

    const usage = session.getUsage();
    try std.testing.expectEqual(@as(u64, 0), usage.memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 0), usage.cpu_throttled_ms);
}

// ─────────────────────────────────────────────────────────────────────────────
// T3: getResourceContext returns correct memory limit
// ─────────────────────────────────────────────────────────────────────────────

test "T3: getResourceContext converts MB to bytes" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", testCorrelation(), .{
        .memory_limit_mb = 256,
    }, 30_000);
    defer session.destroy();

    const ctx = session.getResourceContext();
    try std.testing.expectEqual(@as(u64, 256 * 1024 * 1024), ctx.memory_limit_bytes);
}

test "T3: getResourceContext uses default 512 MB limit" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", testCorrelation(), .{}, 30_000);
    defer session.destroy();

    const ctx = session.getResourceContext();
    try std.testing.expectEqual(@as(u64, 512 * 1024 * 1024), ctx.memory_limit_bytes);
}

// ─────────────────────────────────────────────────────────────────────────────
// T7: Struct field regression guards
// ─────────────────────────────────────────────────────────────────────────────

test "T7: ExecutionResult has memory_peak_bytes and cpu_throttled_ms fields" {
    comptime std.debug.assert(@hasField(types.ExecutionResult, "memory_peak_bytes"));
    comptime std.debug.assert(@hasField(types.ExecutionResult, "cpu_throttled_ms"));
}

test "T7: ExecutionResult resource fields default to 0" {
    const r = types.ExecutionResult{};
    try std.testing.expectEqual(@as(u64, 0), r.memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 0), r.cpu_throttled_ms);
}

test "T7: CgroupMetrics has 3 fields (peak, limit, throttle)" {
    comptime std.debug.assert(@typeInfo(cgroup.CgroupScope.CgroupMetrics).@"struct".fields.len == 3);
}

test "T7: Session has resource accumulation fields" {
    comptime std.debug.assert(@hasField(Session, "max_memory_peak_bytes"));
    comptime std.debug.assert(@hasField(Session, "total_cpu_throttled_ms"));
}

test "T7: StageResult has resource metric fields" {
    comptime std.debug.assert(@hasField(client_mod.ExecutorClient.StageResult, "memory_peak_bytes"));
    comptime std.debug.assert(@hasField(client_mod.ExecutorClient.StageResult, "cpu_throttled_ms"));
    comptime std.debug.assert(@hasField(client_mod.ExecutorClient.StageResult, "memory_limit_bytes"));
}

test "T7: StageResult resource fields default to 0" {
    const sr = client_mod.ExecutorClient.StageResult{
        .content = "",
        .token_count = 0,
        .wall_seconds = 0,
        .exit_ok = false,
        .failure = null,
    };
    try std.testing.expectEqual(@as(u64, 0), sr.memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 0), sr.cpu_throttled_ms);
    try std.testing.expectEqual(@as(u64, 0), sr.memory_limit_bytes);
}

// ─────────────────────────────────────────────────────────────────────────────
// T2: cgroup.readCpuThrottledUs returns 0 on non-Linux
// ─────────────────────────────────────────────────────────────────────────────

test "T2: CgroupScope.readCpuThrottledUs returns 0 on non-Linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const scope = cgroup.CgroupScope{ .path = "/fake", .alloc = alloc };
    try std.testing.expectEqual(@as(u64, 0), scope.readCpuThrottledUs());
}

test "T2: CgroupScope.destroy returns zero metrics on non-Linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const path = try alloc.dupe(u8, "/fake/cgroup/path");
    var scope = cgroup.CgroupScope{ .path = path, .alloc = alloc };
    const m = scope.destroy(.{ .memory_limit_mb = 256 });
    // On non-Linux, destroy() returns early without freeing path — free it here.
    alloc.free(path);
    try std.testing.expectEqual(@as(u64, 0), m.memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 256 * 1024 * 1024), m.memory_limit_bytes);
    try std.testing.expectEqual(@as(u64, 0), m.cpu_throttled_ms);
}

// ─────────────────────────────────────────────────────────────────────────────
// T11: Memory leak detection — session lifecycle with resource metrics
// ─────────────────────────────────────────────────────────────────────────────

test "T11: session create/record/destroy cycle has no leaks" {
    const alloc = std.testing.allocator;
    for (0..50) |i| {
        var session = try Session.create(alloc, "/tmp/ws", testCorrelation(), .{}, 30_000);
        session.recordStageResult(.{
            .exit_ok = true,
            .token_count = i,
            .wall_seconds = 1,
            .memory_peak_bytes = i * 1024,
            .cpu_throttled_ms = i,
        });
        _ = session.getUsage();
        _ = session.getResourceContext();
        session.destroy();
    }
}

test "T11: SessionStore lifecycle with resource-bearing sessions has no leaks" {
    const alloc = std.testing.allocator;
    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();

    for (0..10) |i| {
        const sess = try alloc.create(Session);
        sess.* = try Session.create(alloc, "/tmp/ws", testCorrelation(), .{
            .memory_limit_mb = 256,
        }, 30_000);
        sess.recordStageResult(.{
            .exit_ok = true,
            .memory_peak_bytes = (i + 1) * 1024 * 1024,
            .cpu_throttled_ms = (i + 1) * 100,
        });
        try store.put(sess);
    }
    // store.deinit() via defer will destroy all sessions — allocator detects leaks.
}

// ─────────────────────────────────────────────────────────────────────────────
// T5: Concurrent recordStageResult (single session, sequential by design)
// ─────────────────────────────────────────────────────────────────────────────

test "T5: rapid sequential recordStageResult accumulates correctly" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", testCorrelation(), .{}, 30_000);
    defer session.destroy();

    for (0..1000) |i| {
        session.recordStageResult(.{
            .exit_ok = true,
            .token_count = 1,
            .wall_seconds = 1,
            .memory_peak_bytes = (i % 10) * 1024 * 1024,
            .cpu_throttled_ms = 1,
        });
    }
    try std.testing.expectEqual(@as(u32, 1000), session.stages_executed);
    try std.testing.expectEqual(@as(u64, 1000), session.total_cpu_throttled_ms);
    // Peak should be 9 MB (max of i%10 = 0..9).
    try std.testing.expectEqual(@as(u64, 9 * 1024 * 1024), session.max_memory_peak_bytes);
}
