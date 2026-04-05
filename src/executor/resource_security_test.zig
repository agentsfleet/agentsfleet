//! M27_001: Security & adversarial tests for resource metrics.
//!
//! Perspective: Security Architect + Platform Architect + Systems Engineer.
//!
//! Attack surfaces tested:
//! - Cross-session resource metric contamination (rogue sandbox reads neighbor)
//! - Cgroup path uniqueness (no collision or traversal)
//! - Resource metrics under failure classes (OOM, timeout, lease expired)
//! - Metric integrity under concurrent session operations
//! - Resource limit bypass detection (peak > limit visible in score)
//! - FailureClass propagation preserves resource metric context
//! - Session cancellation zeroes vs preserves metrics appropriately
//!
//! Tiers: T5 (concurrency), T7 (regression), T8 (security), T11 (memleak)

const std = @import("std");
const builtin = @import("builtin");
const types = @import("types.zig");
const session_mod = @import("session.zig");
const cgroup = @import("cgroup.zig");
const executor_metrics = @import("executor_metrics.zig");
const runner = @import("runner.zig");

const Session = session_mod.Session;
const SessionStore = session_mod.SessionStore;

fn testCorrelation() types.CorrelationContext {
    return .{
        .trace_id = "t",
        .run_id = "r",
        .workspace_id = "w",
        .stage_id = "s",
        .role_id = "echo",
        .skill_id = "echo",
    };
}

// ─────────────────────────────────────────────────────────────────────────────
// T8: Cross-session resource metric isolation
// ─────────────────────────────────────────────────────────────────────────────

test "T8: two sessions with different executions do not contaminate resource metrics" {
    const alloc = std.testing.allocator;
    var s1 = Session.create(alloc, "/tmp/ws1", testCorrelation(), .{ .memory_limit_mb = 256 }, 30_000);
    defer s1.destroy();
    var s2 = Session.create(alloc, "/tmp/ws2", testCorrelation(), .{ .memory_limit_mb = 512 }, 30_000);
    defer s2.destroy();

    // Session 1 records heavy resource usage.
    s1.recordStageResult(.{
        .exit_ok = true,
        .memory_peak_bytes = 200 * 1024 * 1024,
        .cpu_throttled_ms = 5000,
        .token_count = 100,
        .wall_seconds = 10,
    });

    // Session 2 records light usage.
    s2.recordStageResult(.{
        .exit_ok = true,
        .memory_peak_bytes = 10 * 1024 * 1024,
        .cpu_throttled_ms = 0,
        .token_count = 50,
        .wall_seconds = 5,
    });

    // Verify: s1 metrics did NOT leak into s2.
    const usage1 = s1.getUsage();
    const usage2 = s2.getUsage();
    try std.testing.expectEqual(@as(u64, 200 * 1024 * 1024), usage1.memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 5000), usage1.cpu_throttled_ms);
    try std.testing.expectEqual(@as(u64, 10 * 1024 * 1024), usage2.memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 0), usage2.cpu_throttled_ms);

    // Resource contexts are also independent.
    const ctx1 = s1.getResourceContext();
    const ctx2 = s2.getResourceContext();
    try std.testing.expectEqual(@as(u64, 256 * 1024 * 1024), ctx1.memory_limit_bytes);
    try std.testing.expectEqual(@as(u64, 512 * 1024 * 1024), ctx2.memory_limit_bytes);
}

test "T8: destroying one session in store does not affect neighbor's metrics" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();

    const s1 = try alloc.create(Session);
    s1.* = Session.create(alloc, "/tmp/ws1", testCorrelation(), .{}, 30_000);
    s1.recordStageResult(.{
        .exit_ok = true,
        .memory_peak_bytes = 300 * 1024 * 1024,
        .cpu_throttled_ms = 2000,
    });
    try store.put(s1);

    const s2 = try alloc.create(Session);
    s2.* = Session.create(alloc, "/tmp/ws2", testCorrelation(), .{}, 30_000);
    s2.recordStageResult(.{
        .exit_ok = true,
        .memory_peak_bytes = 50 * 1024 * 1024,
        .cpu_throttled_ms = 100,
    });
    try store.put(s2);

    // Destroy s1, verify s2 is intact.
    const removed = store.remove(s1.execution_id).?;
    const s1_usage = removed.getUsage();
    try std.testing.expectEqual(@as(u64, 300 * 1024 * 1024), s1_usage.memory_peak_bytes);
    removed.destroy();
    alloc.destroy(removed);

    const s2_alive = store.get(s2.execution_id).?;
    const s2_usage = s2_alive.getUsage();
    try std.testing.expectEqual(@as(u64, 50 * 1024 * 1024), s2_usage.memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 100), s2_usage.cpu_throttled_ms);
}

// ─────────────────────────────────────────────────────────────────────────────
// T8: Cgroup path uniqueness — no collision, no traversal
// ─────────────────────────────────────────────────────────────────────────────

test "T8: 10000 execution IDs produce unique cgroup paths" {
    var seen = std.AutoHashMap([32]u8, void).init(std.testing.allocator);
    defer seen.deinit();

    for (0..10_000) |_| {
        const id = types.generateExecutionId();
        const hex = types.executionIdHex(id);
        const result = try seen.getOrPut(hex);
        // Must be a fresh entry (no collision).
        try std.testing.expect(!result.found_existing);
    }
}

test "T8: execution ID hex contains only lowercase hex chars (no path traversal)" {
    for (0..100) |_| {
        const id = types.generateExecutionId();
        const hex = types.executionIdHex(id);
        for (&hex) |c| {
            try std.testing.expect((c >= '0' and c <= '9') or (c >= 'a' and c <= 'f'));
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// T8: Resource metrics under failure scenarios
// ─────────────────────────────────────────────────────────────────────────────

test "T8: OOM-killed stage still has resource metrics recorded before failure" {
    const alloc = std.testing.allocator;
    var session = Session.create(alloc, "/tmp/ws", testCorrelation(), .{}, 30_000);
    defer session.destroy();

    // Stage succeeds with 100 MB peak.
    session.recordStageResult(.{
        .exit_ok = true,
        .memory_peak_bytes = 100 * 1024 * 1024,
        .cpu_throttled_ms = 200,
        .token_count = 50,
        .wall_seconds = 5,
    });

    // Stage fails (OOM kill) — metrics should still accumulate.
    session.recordStageResult(.{
        .exit_ok = false,
        .failure = .oom_kill,
        .memory_peak_bytes = 512 * 1024 * 1024, // Hit limit before death.
        .cpu_throttled_ms = 1000,
        .token_count = 20,
        .wall_seconds = 2,
    });

    const usage = session.getUsage();
    // Peak memory should be from the failed stage (higher).
    try std.testing.expectEqual(@as(u64, 512 * 1024 * 1024), usage.memory_peak_bytes);
    // CPU throttle should be summed.
    try std.testing.expectEqual(@as(u64, 1200), usage.cpu_throttled_ms);
    // Failure propagated.
    try std.testing.expect(!usage.exit_ok);
    try std.testing.expectEqual(types.FailureClass.oom_kill, usage.failure.?);
}

test "T8: cancelled session preserves accumulated resource metrics" {
    const alloc = std.testing.allocator;
    var session = Session.create(alloc, "/tmp/ws", testCorrelation(), .{}, 30_000);
    defer session.destroy();

    session.recordStageResult(.{
        .exit_ok = true,
        .memory_peak_bytes = 150 * 1024 * 1024,
        .cpu_throttled_ms = 300,
        .token_count = 80,
        .wall_seconds = 8,
    });

    session.cancel();
    try std.testing.expect(session.isCancelled());

    // Metrics should still be readable after cancellation.
    const usage = session.getUsage();
    try std.testing.expectEqual(@as(u64, 150 * 1024 * 1024), usage.memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 300), usage.cpu_throttled_ms);
}

test "T8: lease-expired session metrics survive for scoring" {
    const alloc = std.testing.allocator;
    // Set lease timeout far in the past by manipulating timestamp after create.
    var session = Session.create(alloc, "/tmp/ws", testCorrelation(), .{}, 1); // 1ms lease
    defer session.destroy();

    session.recordStageResult(.{
        .exit_ok = true,
        .memory_peak_bytes = 200 * 1024 * 1024,
        .cpu_throttled_ms = 500,
    });

    // Force lease expiry by backdating the heartbeat.
    session.lease.last_heartbeat_ms = std.time.milliTimestamp() - 1000;
    try std.testing.expect(session.isLeaseExpired());

    // Metrics remain accessible even after lease expiry.
    const usage = session.getUsage();
    try std.testing.expectEqual(@as(u64, 200 * 1024 * 1024), usage.memory_peak_bytes);
}

// ─────────────────────────────────────────────────────────────────────────────
// T8: Resource limit bypass detection — scoring surface
// ─────────────────────────────────────────────────────────────────────────────

test "T8: agent exceeding memory limit is detectable in session metrics" {
    const alloc = std.testing.allocator;
    var session = Session.create(alloc, "/tmp/ws", testCorrelation(), .{
        .memory_limit_mb = 512,
    }, 30_000);
    defer session.destroy();

    // Agent uses 600 MB against 512 MB limit (117% usage — OOM-adjacent).
    session.recordStageResult(.{
        .exit_ok = true,
        .memory_peak_bytes = 600 * 1024 * 1024,
        .cpu_throttled_ms = 0,
        .token_count = 100,
        .wall_seconds = 10,
    });

    const usage = session.getUsage();
    const ctx = session.getResourceContext();

    // Peak exceeds limit — scoring layer will clamp to 0 mem_score.
    try std.testing.expect(usage.memory_peak_bytes > ctx.memory_limit_bytes);

    // 100% throttle + 100% memory scenario.
    var rogue = Session.create(alloc, "/tmp/ws2", testCorrelation(), .{ .memory_limit_mb = 512 }, 30_000);
    defer rogue.destroy();
    rogue.recordStageResult(.{
        .exit_ok = false,
        .failure = .resource_kill,
        .memory_peak_bytes = 512 * 1024 * 1024,
        .cpu_throttled_ms = 10_000,
        .token_count = 0,
        .wall_seconds = 10,
    });
    const rogue_usage = rogue.getUsage();
    try std.testing.expectEqual(@as(u64, 512 * 1024 * 1024), rogue_usage.memory_peak_bytes);
    try std.testing.expectEqual(@as(u64, 10_000), rogue_usage.cpu_throttled_ms);
}

// ─────────────────────────────────────────────────────────────────────────────
// T5+T8: Concurrent session creation and metric isolation
// ─────────────────────────────────────────────────────────────────────────────

test "T5+T8: concurrent sessions in store maintain independent resource metrics" {
    const page = std.heap.page_allocator;
    var store = SessionStore.init(page);
    defer store.deinit();

    const Worker = struct {
        fn run(s: *SessionStore, a: std.mem.Allocator, thread_id: usize) void {
            for (0..5) |i| {
                const sess = a.create(Session) catch return;
                sess.* = Session.create(a, "/tmp/ws", testCorrelation(), .{
                    .memory_limit_mb = @as(u64, @intCast(thread_id + 1)) * 128,
                }, 30_000);
                sess.recordStageResult(.{
                    .exit_ok = true,
                    .memory_peak_bytes = (thread_id + 1 + i) * 1024 * 1024,
                    .cpu_throttled_ms = (thread_id + 1) * 100,
                });
                s.put(sess) catch return;
            }
        }
    };

    var threads: [8]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = std.Thread.spawn(.{}, Worker.run, .{ &store, page, i }) catch unreachable;
    }
    for (&threads) |*t| t.join();

    // 8 threads * 5 sessions = 40 sessions.
    try std.testing.expectEqual(@as(usize, 40), store.activeCount());

    // Verify no session has corrupted metrics (peak > 0 for all).
    var it = store.sessions.iterator();
    while (it.next()) |entry| {
        const sess = entry.value_ptr.*;
        const usage = sess.getUsage();
        // Every session had at least one recordStageResult with memory > 0.
        try std.testing.expect(usage.memory_peak_bytes > 0);
        try std.testing.expect(usage.cpu_throttled_ms > 0);
        try std.testing.expectEqual(@as(u32, 1), sess.stages_executed);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// T7: FailureClass → error code mapping stability for resource-related failures
// ─────────────────────────────────────────────────────────────────────────────

test "T7: FailureClass covers all resource-related failure types" {
    // Ensure we can construct FailureClass values for all resource scenarios.
    const resource_failures = [_]types.FailureClass{
        .oom_kill,
        .resource_kill,
        .timeout_kill,
    };
    for (resource_failures) |fc| {
        const label = fc.label();
        try std.testing.expect(label.len > 0);
    }
}

test "T7: mapError maps OOM and Timeout correctly (resource boundary violations)" {
    try std.testing.expectEqual(types.FailureClass.oom_kill, runner.mapError(runner.RunnerError.OutOfMemory));
    try std.testing.expectEqual(types.FailureClass.timeout_kill, runner.mapError(runner.RunnerError.Timeout));
}

// ─────────────────────────────────────────────────────────────────────────────
// T8: Rogue agent scenario — full pipeline simulation
// ─────────────────────────────────────────────────────────────────────────────

test "T8: rogue agent consuming max resources across 3 stages has metrics visible" {
    const alloc = std.testing.allocator;
    var session = Session.create(alloc, "/tmp/ws", testCorrelation(), .{
        .memory_limit_mb = 512,
    }, 30_000);
    defer session.destroy();

    // Stage 1: moderate.
    session.recordStageResult(.{
        .exit_ok = true,
        .memory_peak_bytes = 100 * 1024 * 1024,
        .cpu_throttled_ms = 500,
        .token_count = 1000,
        .wall_seconds = 30,
    });
    // Stage 2: rogue — spikes to near-limit.
    session.recordStageResult(.{
        .exit_ok = true,
        .memory_peak_bytes = 490 * 1024 * 1024,
        .cpu_throttled_ms = 15_000,
        .token_count = 2000,
        .wall_seconds = 60,
    });
    // Stage 3: moderate again.
    session.recordStageResult(.{
        .exit_ok = true,
        .memory_peak_bytes = 80 * 1024 * 1024,
        .cpu_throttled_ms = 200,
        .token_count = 500,
        .wall_seconds = 10,
    });

    const usage = session.getUsage();
    const ctx = session.getResourceContext();

    // Peak memory should be from the rogue stage (max across stages).
    try std.testing.expectEqual(@as(u64, 490 * 1024 * 1024), usage.memory_peak_bytes);
    // CPU throttle should be cumulative across all stages.
    try std.testing.expectEqual(@as(u64, 15_700), usage.cpu_throttled_ms);
    // Memory limit is from the resource context.
    try std.testing.expectEqual(@as(u64, 512 * 1024 * 1024), ctx.memory_limit_bytes);

    // Verify the peak-to-limit ratio is > 95% — scoring layer will score this < 30.
    const peak_pct = (usage.memory_peak_bytes * 100) / ctx.memory_limit_bytes;
    try std.testing.expect(peak_pct >= 95);
    // Verify throttle-to-wall ratio is significant.
    const wall_ms = usage.wall_seconds * 1000;
    const throttle_pct = (usage.cpu_throttled_ms * 100) / wall_ms;
    try std.testing.expect(throttle_pct > 10);
}

// ─────────────────────────────────────────────────────────────────────────────
// T11: Memory leak detection under adversarial session patterns
// ─────────────────────────────────────────────────────────────────────────────

test "T11: rapid session create/record/destroy with high metric values has no leaks" {
    const alloc = std.testing.allocator;
    for (0..100) |i| {
        var session = Session.create(alloc, "/tmp/ws", testCorrelation(), .{
            .memory_limit_mb = @as(u64, @intCast(i + 1)) * 64,
        }, 30_000);
        session.recordStageResult(.{
            .exit_ok = i % 3 != 0,
            .failure = if (i % 3 == 0) .oom_kill else null,
            .memory_peak_bytes = @as(u64, @intCast(i + 1)) * 50 * 1024 * 1024,
            .cpu_throttled_ms = @as(u64, @intCast(i + 1)) * 100,
            .token_count = @as(u64, @intCast(i + 1)) * 10,
            .wall_seconds = @as(u64, @intCast(i + 1)),
        });
        _ = session.getUsage();
        _ = session.getResourceContext();
        session.destroy();
    }
    // std.testing.allocator detects leaks automatically.
}

test "T11: SessionStore with mixed failure sessions has no leaks" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);

    for (0..20) |i| {
        const sess = try alloc.create(Session);
        sess.* = Session.create(alloc, "/tmp/ws", testCorrelation(), .{}, 30_000);
        sess.recordStageResult(.{
            .exit_ok = i % 2 == 0,
            .failure = if (i % 2 != 0) .resource_kill else null,
            .memory_peak_bytes = (i + 1) * 1024 * 1024,
            .cpu_throttled_ms = (i + 1) * 50,
        });
        try store.put(sess);
    }

    // deinit destroys all sessions — allocator catches any leak.
    store.deinit();
}
