const std = @import("std");
const reliable = @import("../reliability/reliable_call.zig");

pub const WorkerError = error{
    ShutdownRequested,
    RunDeadlineExceeded,
    PathTraversal,
    MissingGitHubInstallation,
    InvalidPipelineRole,
    InvalidPipelineProfile,
    SideEffectClaimedNoResult,
};

pub fn ensureBeforeDeadline(deadline_ms: i64) WorkerError!void {
    if (std.time.milliTimestamp() > deadline_ms) return WorkerError.RunDeadlineExceeded;
}

pub fn ensureRunActive(cancel_flag: *const std.atomic.Value(bool), deadline_ms: i64) WorkerError!void {
    if (!cancel_flag.load(.acquire)) return WorkerError.ShutdownRequested;
    try ensureBeforeDeadline(deadline_ms);
}

pub fn retryOptionsForRun(
    cancel_flag: *std.atomic.Value(bool),
    deadline_ms: i64,
    max_retries: u32,
    base_delay_ms: u64,
    max_delay_ms: u64,
    operation_name: ?[]const u8,
) reliable.RetryOptions {
    return .{
        .max_retries = max_retries,
        .base_delay_ms = base_delay_ms,
        .max_delay_ms = max_delay_ms,
        .operation_name = operation_name,
        .cancel_flag = cancel_flag,
        .deadline_ms = deadline_ms,
        .cancel_error = WorkerError.ShutdownRequested,
        .deadline_error = WorkerError.RunDeadlineExceeded,
    };
}

pub fn sleepCooperative(
    total_ms: u64,
    cancel_flag: *const std.atomic.Value(bool),
    deadline_ms: i64,
) WorkerError!void {
    var remaining = total_ms;
    while (true) {
        try ensureRunActive(cancel_flag, deadline_ms);
        if (remaining == 0) return;

        const slice_ms: u64 = @min(remaining, 100);
        std.Thread.sleep(slice_ms * std.time.ns_per_ms);
        remaining -= slice_ms;
    }
}

pub fn sleepWhileRunning(cancel_flag: *const std.atomic.Value(bool), total_ms: u64) void {
    var remaining = total_ms;
    while (remaining > 0 and cancel_flag.load(.acquire)) {
        const slice_ms: u64 = @min(remaining, 100);
        std.Thread.sleep(slice_ms * std.time.ns_per_ms);
        remaining -= slice_ms;
    }
}

test "retryOptionsForRun wires operation and cancellation semantics" {
    var running = std.atomic.Value(bool).init(true);
    const opts = retryOptionsForRun(&running, 12345, 3, 100, 500, "op");

    try std.testing.expectEqual(@as(u32, 3), opts.max_retries);
    try std.testing.expectEqual(@as(u64, 100), opts.base_delay_ms);
    try std.testing.expectEqual(@as(u64, 500), opts.max_delay_ms);
    try std.testing.expectEqual(@as(i64, 12345), opts.deadline_ms.?);
    try std.testing.expectEqualStrings("op", opts.operation_name.?);
    try std.testing.expect(opts.cancel_flag == &running);
}

test "ensureRunActive returns deadline exceeded when past deadline" {
    var running = std.atomic.Value(bool).init(true);
    try std.testing.expectError(
        WorkerError.RunDeadlineExceeded,
        ensureRunActive(&running, std.time.milliTimestamp() - 1),
    );
}

test "sleepCooperative returns shutdown when worker stops" {
    var running = std.atomic.Value(bool).init(false);
    try std.testing.expectError(
        WorkerError.ShutdownRequested,
        sleepCooperative(1, &running, std.time.milliTimestamp() + 10_000),
    );
}

test "sleepCooperative returns deadline exceeded when deadline already passed" {
    var running = std.atomic.Value(bool).init(true);
    try std.testing.expectError(
        WorkerError.RunDeadlineExceeded,
        sleepCooperative(10, &running, std.time.milliTimestamp() - 1),
    );
}

test "integration: sleepCooperative returns deadline exceeded during wait window" {
    var running = std.atomic.Value(bool).init(true);
    const deadline_ms = std.time.milliTimestamp() + 10;
    try std.testing.expectError(
        WorkerError.RunDeadlineExceeded,
        sleepCooperative(250, &running, deadline_ms),
    );
}

const FlipCtx = struct {
    running: *std.atomic.Value(bool),
};

fn flipOff(ctx: FlipCtx) void {
    std.Thread.sleep(20 * std.time.ns_per_ms);
    ctx.running.store(false, .release);
}

test "integration: sleepCooperative exits on concurrent shutdown signal" {
    var running = std.atomic.Value(bool).init(true);
    const t = try std.Thread.spawn(.{}, flipOff, .{FlipCtx{ .running = &running }});
    defer t.join();

    try std.testing.expectError(
        WorkerError.ShutdownRequested,
        sleepCooperative(500, &running, std.time.milliTimestamp() + 10_000),
    );
}

test "integration: sleepWhileRunning returns immediately when stopped" {
    var running = std.atomic.Value(bool).init(false);
    sleepWhileRunning(&running, 500);
}
