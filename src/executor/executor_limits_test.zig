//! cgroup platform guard tests + executor/protocol constant stability (T2, T3, T5, T7, T10).
//!
//! Extracted from sandbox_edge_test.zig to keep each file under 500 lines.

const std = @import("std");
const builtin = @import("builtin");
const cgroup = @import("cgroup.zig");
const types = @import("types.zig");
const runner = @import("runner.zig");
const executor_metrics = @import("executor_metrics.zig");
const protocol = @import("protocol.zig");

// ─────────────────────────────────────────────────────────────────────────────
// cgroup platform guards — T3 non-Linux
// ─────────────────────────────────────────────────────────────────────────────

test "T3: CgroupScope.create returns UnsupportedPlatform on non-Linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const result = cgroup.CgroupScope.create(alloc, types.generateExecutionId(), .{});
    try std.testing.expectError(cgroup.CgroupError.UnsupportedPlatform, result);
}

test "T3: CgroupScope.addProcess returns UnsupportedPlatform on non-Linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    var scope = cgroup.CgroupScope{ .path = "/fake", .alloc = alloc };
    const result = scope.addProcess(1);
    try std.testing.expectError(cgroup.CgroupError.UnsupportedPlatform, result);
}

test "T2: CgroupScope.readMemoryPeak returns 0 on non-Linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const scope = cgroup.CgroupScope{ .path = "/fake", .alloc = alloc };
    try std.testing.expectEqual(@as(u64, 0), scope.readMemoryPeak());
}

test "T2: CgroupScope.readMemoryCurrent returns 0 on non-Linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const scope = cgroup.CgroupScope{ .path = "/fake", .alloc = alloc };
    try std.testing.expectEqual(@as(u64, 0), scope.readMemoryCurrent());
}

test "T2: CgroupScope.wasOomKilled returns false on non-Linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const scope = cgroup.CgroupScope{ .path = "/fake", .alloc = alloc };
    try std.testing.expect(!scope.wasOomKilled());
}

// ─────────────────────────────────────────────────────────────────────────────
// setExecutorMemoryPeakBytes monotonicity under concurrent updates — T5
// ─────────────────────────────────────────────────────────────────────────────

test "T5: setExecutorMemoryPeakBytes is monotone under concurrent updates from 8 threads" {
    // Seed at 500 KB. Threads write 200 KB–1 600 KB (thread_id * 200_000).
    // Threads 1–2 write values below the seed (200 KB, 400 KB) — must not regress peak.
    // Threads 3–8 write values above the seed (600 KB–1 600 KB) — must raise peak.
    // Asserting final peak >= 1_600_000 proves both the CAS fires for higher values
    // and lower-valued threads cannot regress it.
    executor_metrics.setExecutorMemoryPeakBytes(500_000);

    const Worker = struct {
        fn run(thread_id: usize) void {
            executor_metrics.setExecutorMemoryPeakBytes(thread_id * 200_000);
        }
    };

    var threads: [8]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = std.Thread.spawn(.{}, Worker.run, .{i + 1}) catch unreachable;
    }
    for (&threads) |*t| t.join();

    const snap = executor_metrics.executorSnapshot();
    // Thread 8 writes 1_600_000; peak must have risen to at least that.
    try std.testing.expect(snap.memory_peak_bytes >= 1_600_000);
}

// ─────────────────────────────────────────────────────────────────────────────
// errorCodeForFailure — T10 constants stability
// ─────────────────────────────────────────────────────────────────────────────

test "T10: errorCodeForFailure returns correct UZ-EXEC codes" {
    try std.testing.expectEqualStrings("UZ-EXEC-012", runner.errorCodeForFailure(.startup_posture));
    try std.testing.expectEqualStrings("UZ-EXEC-003", runner.errorCodeForFailure(.timeout_kill));
    try std.testing.expectEqualStrings("UZ-EXEC-004", runner.errorCodeForFailure(.oom_kill));
    try std.testing.expectEqualStrings("UZ-EXEC-013", runner.errorCodeForFailure(.executor_crash));
}

test "T10: errorCodeForFailure for non-primary variants returns fallback runner code" {
    const fallback = "UZ-EXEC-013";
    try std.testing.expectEqualStrings(fallback, runner.errorCodeForFailure(.policy_deny));
    try std.testing.expectEqualStrings(fallback, runner.errorCodeForFailure(.resource_kill));
    try std.testing.expectEqualStrings(fallback, runner.errorCodeForFailure(.transport_loss));
    try std.testing.expectEqualStrings(fallback, runner.errorCodeForFailure(.landlock_deny));
    try std.testing.expectEqualStrings(fallback, runner.errorCodeForFailure(.lease_expired));
}

// ─────────────────────────────────────────────────────────────────────────────
// Protocol constants — T10
// ─────────────────────────────────────────────────────────────────────────────

test "T10: protocol error codes match JSON-RPC spec values" {
    try std.testing.expectEqual(@as(i32, -32700), protocol.ErrorCode.parse_error);
    try std.testing.expectEqual(@as(i32, -32600), protocol.ErrorCode.invalid_request);
    try std.testing.expectEqual(@as(i32, -32601), protocol.ErrorCode.method_not_found);
    try std.testing.expectEqual(@as(i32, -32602), protocol.ErrorCode.invalid_params);
    try std.testing.expectEqual(@as(i32, -32603), protocol.ErrorCode.internal_error);
}

test "T10: protocol execution-specific error codes are negative and distinct" {
    const codes = [_]i32{
        protocol.ErrorCode.execution_failed,
        protocol.ErrorCode.timeout_killed,
        protocol.ErrorCode.oom_killed,
        protocol.ErrorCode.policy_denied,
        protocol.ErrorCode.lease_expired,
        protocol.ErrorCode.landlock_denied,
        protocol.ErrorCode.resource_killed,
    };
    for (codes) |c| {
        try std.testing.expect(c < 0);
    }
    for (codes, 0..) |a, i| {
        for (codes, 0..) |b, j| {
            if (i != j) try std.testing.expect(a != b);
        }
    }
}

test "T10: MAX_FRAME_SIZE is 16 MiB" {
    try std.testing.expectEqual(@as(u32, 16 * 1024 * 1024), protocol.MAX_FRAME_SIZE);
}
