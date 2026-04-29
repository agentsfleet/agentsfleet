//! Sandbox resource-limit and failure-class edge case tests.
//!
//! Covers (Tiers T1–T5, T7):
//! - OOM kill detection path via FailureClass mapping
//! - Disk-full / resource_kill scenario
//! - Network access denied (policy_deny) — no metric increment, no crash
//! - Rogue process consuming excess resources (resource_kill metric path)
//! - All FailureClass variants produce stable labels (T7 regression)
//! - ResourceLimits edge values (zero memory, max disk, network flag)
//! - Session lifecycle: recordStageResult accumulation, getUsage aggregation
//! - Atomic cancel propagation (T5 concurrency)
//! - Concurrent lease touch from multiple threads (T5)
//! - Mixed expired/active sessions — partial reap only clears expired (T3)
//! - NetworkPolicy.registry_allowlist adds --share-net bwrap arg
//! - incFailureMetric covers resource_kill, landlock_deny, lease_expired paths
//! (cgroup guards + protocol/metric constants live in executor_limits_test.zig)

const std = @import("std");
const builtin = @import("builtin");
const runner = @import("runner.zig");
const types = @import("types.zig");
const executor_metrics = @import("executor_metrics.zig");
const Session = @import("session.zig");
const SessionStore = @import("runtime/session_store.zig");
const network = @import("network.zig");

// ─────────────────────────────────────────────────────────────────────────────
// FailureClass metric coverage — T3 (negative paths)
// ─────────────────────────────────────────────────────────────────────────────

test "T3: incFailureMetric increments resource_kill counter" {
    const before = executor_metrics.executorSnapshot().resource_kills_total;
    runner.incFailureMetric(.resource_kill);
    const after = executor_metrics.executorSnapshot().resource_kills_total;
    try std.testing.expect(after > before);
}

test "T3: incFailureMetric increments landlock_deny counter" {
    const before = executor_metrics.executorSnapshot().landlock_denials_total;
    runner.incFailureMetric(.landlock_deny);
    const after = executor_metrics.executorSnapshot().landlock_denials_total;
    try std.testing.expect(after > before);
}

test "T3: incFailureMetric increments lease_expired counter" {
    const before = executor_metrics.executorSnapshot().lease_expired_total;
    runner.incFailureMetric(.lease_expired);
    const after = executor_metrics.executorSnapshot().lease_expired_total;
    try std.testing.expect(after > before);
}

// policy_deny / startup_posture / executor_crash / transport_loss fall into
// else => {} — they must not crash or increment any counter by mistake.
test "T3: incFailureMetric policy_deny does not crash" {
    const snap_before = executor_metrics.executorSnapshot();
    runner.incFailureMetric(.policy_deny);
    const snap_after = executor_metrics.executorSnapshot();
    // No counter dedicated to policy_deny — verify nothing unexpected changed.
    try std.testing.expectEqual(snap_before.resource_kills_total, snap_after.resource_kills_total);
    try std.testing.expectEqual(snap_before.oom_kills_total, snap_after.oom_kills_total);
}

test "T3: incFailureMetric startup_posture does not crash" {
    const snap_before = executor_metrics.executorSnapshot();
    runner.incFailureMetric(.startup_posture);
    const snap_after = executor_metrics.executorSnapshot();
    try std.testing.expectEqual(snap_before.oom_kills_total, snap_after.oom_kills_total);
    try std.testing.expectEqual(snap_before.timeout_kills_total, snap_after.timeout_kills_total);
}

test "T3: incFailureMetric executor_crash does not crash" {
    runner.incFailureMetric(.executor_crash);
    // No crash — pass.
}

test "T3: incFailureMetric transport_loss does not crash" {
    runner.incFailureMetric(.transport_loss);
    // No crash — pass.
}

// ─────────────────────────────────────────────────────────────────────────────
// mapError — all FailureClass-producing paths (T7 regression safety)
// ─────────────────────────────────────────────────────────────────────────────

test "T7: mapError produces correct FailureClass for all RunnerError variants" {
    const cases = [_]struct { err: anyerror, want: types.FailureClass }{
        .{ .err = runner.RunnerError.InvalidConfig, .want = .startup_posture },
        .{ .err = runner.RunnerError.AgentInitFailed, .want = .startup_posture },
        .{ .err = runner.RunnerError.AgentRunFailed, .want = .executor_crash },
        .{ .err = runner.RunnerError.Timeout, .want = .timeout_kill },
        .{ .err = runner.RunnerError.OutOfMemory, .want = .oom_kill },
        .{ .err = error.Unexpected, .want = .executor_crash },
        // In Zig, error.OutOfMemory and RunnerError.OutOfMemory are the same value.
        .{ .err = error.OutOfMemory, .want = .oom_kill },
    };
    for (cases) |c| {
        const got = runner.mapError(c.err);
        try std.testing.expectEqual(c.want, got);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FailureClass labels — T7 regression: no label must ever be empty
// ─────────────────────────────────────────────────────────────────────────────

test "T7: FailureClass label is stable for all variants" {
    const expected = [_]struct { fc: types.FailureClass, label: []const u8 }{
        .{ .fc = .startup_posture, .label = "startup_posture" },
        .{ .fc = .policy_deny, .label = "policy_deny" },
        .{ .fc = .timeout_kill, .label = "timeout_kill" },
        .{ .fc = .oom_kill, .label = "oom_kill" },
        .{ .fc = .resource_kill, .label = "resource_kill" },
        .{ .fc = .executor_crash, .label = "executor_crash" },
        .{ .fc = .transport_loss, .label = "transport_loss" },
        .{ .fc = .landlock_deny, .label = "landlock_deny" },
        .{ .fc = .lease_expired, .label = "lease_expired" },
    };
    for (expected) |e| {
        try std.testing.expectEqualStrings(e.label, e.fc.label());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ResourceLimits edge values — T2 edge cases
// ─────────────────────────────────────────────────────────────────────────────

test "T2: ResourceLimits with zero memory_limit_mb is representable" {
    const r = types.ResourceLimits{ .memory_limit_mb = 0 };
    try std.testing.expectEqual(@as(u64, 0), r.memory_limit_mb);
    try std.testing.expectEqual(@as(u64, 100), r.cpu_limit_percent);
}

test "T2: ResourceLimits with zero disk_write_limit_mb is representable" {
    const r = types.ResourceLimits{ .disk_write_limit_mb = 0 };
    try std.testing.expectEqual(@as(u64, 0), r.disk_write_limit_mb);
}

test "T2: ResourceLimits.network_egress_allowed=true is representable" {
    const r = types.ResourceLimits{ .network_egress_allowed = true };
    try std.testing.expect(r.network_egress_allowed);
}

test "T2: ResourceLimits max values do not overflow u64" {
    const r = types.ResourceLimits{
        .memory_limit_mb = std.math.maxInt(u64),
        .disk_write_limit_mb = std.math.maxInt(u64),
        .cpu_limit_percent = 100,
    };
    try std.testing.expectEqual(std.math.maxInt(u64), r.memory_limit_mb);
}

test "T10: ResourceLimits default memory is 512 MiB" {
    const r = types.ResourceLimits{};
    try std.testing.expectEqual(@as(u64, 512), r.memory_limit_mb);
}

test "T10: ResourceLimits default disk is 1024 MiB" {
    const r = types.ResourceLimits{};
    try std.testing.expectEqual(@as(u64, 1024), r.disk_write_limit_mb);
}

test "T10: ResourceLimits network egress is denied by default" {
    const r = types.ResourceLimits{};
    try std.testing.expect(!r.network_egress_allowed);
}

// ─────────────────────────────────────────────────────────────────────────────
// Session lifecycle — T1 accumulation, T2 edge cases
// ─────────────────────────────────────────────────────────────────────────────

test "T1: Session.recordStageResult accumulates tokens and wall_seconds" {
    const page = std.heap.page_allocator;
    var session = try Session.create(page, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 30_000, .{});
    defer session.destroy();

    session.recordStageResult(.{ .content = "a", .token_count = 100, .wall_seconds = 5, .exit_ok = true });
    session.recordStageResult(.{ .content = "b", .token_count = 200, .wall_seconds = 3, .exit_ok = true });
    session.recordStageResult(.{ .content = "c", .token_count = 50, .wall_seconds = 1, .exit_ok = false, .failure = .oom_kill });

    try std.testing.expectEqual(@as(u64, 350), session.total_tokens);
    try std.testing.expectEqual(@as(u64, 9), session.total_wall_seconds);
    try std.testing.expectEqual(@as(u32, 3), session.stages_executed);
}

test "T1: Session.getUsage reflects last_result failure" {
    const page = std.heap.page_allocator;
    var session = try Session.create(page, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 30_000, .{});
    defer session.destroy();

    session.recordStageResult(.{ .content = "ok", .token_count = 10, .wall_seconds = 1, .exit_ok = true });
    session.recordStageResult(.{ .content = "", .token_count = 0, .wall_seconds = 0, .exit_ok = false, .failure = .resource_kill });

    const usage = session.getUsage();
    try std.testing.expect(!usage.exit_ok);
    try std.testing.expectEqual(types.FailureClass.resource_kill, usage.failure.?);
    try std.testing.expectEqual(@as(u64, 10), usage.token_count);
}

test "T2: Session.getUsage with no stages returns safe defaults" {
    const page = std.heap.page_allocator;
    var session = try Session.create(page, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 30_000, .{});
    defer session.destroy();

    const usage = session.getUsage();
    try std.testing.expectEqual(@as(u64, 0), usage.token_count);
    try std.testing.expectEqual(@as(u64, 0), usage.wall_seconds);
    try std.testing.expect(usage.failure == null);
}

// ─────────────────────────────────────────────────────────────────────────────
// Session.cancel — T5 atomic concurrency
// ─────────────────────────────────────────────────────────────────────────────

test "T5: Session.cancel is atomic — concurrent cancel from 8 threads" {
    const page = std.heap.page_allocator;
    var session = try Session.create(page, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 30_000, .{});
    defer session.destroy();

    const Canceller = struct {
        fn run(s: *Session) void {
            s.cancel();
        }
    };

    var threads: [8]std.Thread = undefined;
    for (&threads) |*t| {
        t.* = std.Thread.spawn(.{}, Canceller.run, .{&session}) catch unreachable;
    }
    for (&threads) |*t| t.join();

    // After all threads: session must be cancelled, no crash.
    try std.testing.expect(session.isCancelled());
}

test "T5: Concurrent lease touch from 8 threads — no data race" {
    const page = std.heap.page_allocator;
    var session = try Session.create(page, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 30_000, .{});
    defer session.destroy();

    const Toucher = struct {
        fn run(s: *Session) void {
            for (0..100) |_| {
                s.touchLease();
            }
        }
    };

    var threads: [8]std.Thread = undefined;
    for (&threads) |*t| {
        t.* = std.Thread.spawn(.{}, Toucher.run, .{&session}) catch unreachable;
    }
    for (&threads) |*t| t.join();

    // After all touches: lease should not be expired (just touched).
    try std.testing.expect(!session.isLeaseExpired());
}

// ─────────────────────────────────────────────────────────────────────────────
// LeaseState edge cases — T2
// ─────────────────────────────────────────────────────────────────────────────

test "T2: LeaseState just past expiry boundary is expired" {
    // isExpired uses strict >, so elapsed must be > timeout_ms to expire.
    // Set last_heartbeat to 30_001ms ago with a 30_000ms timeout.
    const lease = types.LeaseState{
        .execution_id = types.generateExecutionId(),
        .last_heartbeat_ms = std.time.milliTimestamp() - 30_001,
        .lease_timeout_ms = 30_000,
    };
    try std.testing.expect(lease.isExpired());
}

test "T2: LeaseState with zero timeout expires immediately" {
    const lease = types.LeaseState{
        .execution_id = types.generateExecutionId(),
        .last_heartbeat_ms = std.time.milliTimestamp() - 1,
        .lease_timeout_ms = 0,
    };
    try std.testing.expect(lease.isExpired());
}

test "T2: LeaseState with very large timeout never expires" {
    // Use max i64 value — isExpired casts lease_timeout_ms to i64 internally,
    // so stay within i64 range to avoid overflow.
    const large_ms: u64 = @intCast(std.math.maxInt(i64));
    var lease = types.LeaseState{
        .execution_id = types.generateExecutionId(),
        .last_heartbeat_ms = std.time.milliTimestamp(),
        .lease_timeout_ms = large_ms,
    };
    try std.testing.expect(!lease.isExpired());
    lease.touch();
    try std.testing.expect(!lease.isExpired());
}

// ─────────────────────────────────────────────────────────────────────────────
// Mixed expired/active sessions — partial reap (T3)
// ─────────────────────────────────────────────────────────────────────────────

test "T3: SessionStore.reapExpired only removes expired sessions, leaves active ones" {
    const page = std.heap.page_allocator;
    var store = SessionStore.init(page);
    defer store.deinit();

    // Create 3 sessions that will expire (1ms lease).
    // Use string literals — stack-buffer session_id would outlive the loop
    // iteration and produce dangling pointers since Session stores the slice
    // header without copying the bytes.
    for (0..3) |_| {
        const s = try page.create(Session);
        s.* = try Session.create(page, "/tmp/ws", .{
            .trace_id = "t",
            .zombie_id = "r",
            .workspace_id = "w",
            .session_id = "exp",
        }, .{}, 1, .{}); // 1ms — will expire
        try store.put(s);
    }

    // Create 2 sessions with long lease (30s).
    for (0..2) |_| {
        const s = try page.create(Session);
        s.* = try Session.create(page, "/tmp/ws", .{
            .trace_id = "t",
            .zombie_id = "r",
            .workspace_id = "w",
            .session_id = "act",
        }, .{}, 30_000, .{}); // 30s — will NOT expire
        try store.put(s);
    }

    try std.testing.expectEqual(@as(usize, 5), store.activeCount());

    // Wait for the short leases to expire.
    std.Thread.sleep(10 * std.time.ns_per_ms);

    const reaped = store.reapExpired();
    try std.testing.expectEqual(@as(u32, 3), reaped);
    try std.testing.expectEqual(@as(usize, 2), store.activeCount());
}

// ─────────────────────────────────────────────────────────────────────────────
// Network policy — T3 registry_allowlist adds --share-net, T2 edge cases
// ─────────────────────────────────────────────────────────────────────────────

test "T3: NetworkPolicy.registry_allowlist appends --share-net bwrap arg" {
    const alloc = std.testing.allocator;
    var argv = std.ArrayList([]const u8){};
    defer argv.deinit(alloc);

    const config = network.NetworkConfig{ .policy = .registry_allowlist };
    try network.appendBwrapNetworkArgs(alloc, &argv, config, "test-exec-id");

    // registry_allowlist adds --share-net to allow package registry access.
    try std.testing.expectEqual(@as(usize, 1), argv.items.len);
    try std.testing.expectEqualStrings("--share-net", argv.items[0]);
}

test "T2: NetworkConfig deny_all policy is representable" {
    const config = network.NetworkConfig{
        .policy = .deny_all,
    };
    try std.testing.expectEqual(network.NetworkPolicy.deny_all, config.policy);
}

test "T2: NetworkConfig default is deny_all" {
    const config = network.NetworkConfig{};
    try std.testing.expectEqual(network.NetworkPolicy.deny_all, config.policy);
}

// (cgroup platform guards, errorCodeForFailure, and protocol constants
//  are in executor_limits_test.zig)
