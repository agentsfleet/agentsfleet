//! Generic retry wrapper for external side-effect calls.

const std = @import("std");
const backoff = @import("backoff.zig");
const classifier = @import("error_classify.zig");
const metrics = @import("../observability/metrics.zig");
const log = std.log.scoped(.reliable);

pub const RetryOptions = struct {
    max_retries: u32 = 2,
    base_delay_ms: u64 = 500,
    max_delay_ms: u64 = 30_000,
    operation_name: ?[]const u8 = null,
    cancel_flag: ?*const std.atomic.Value(bool) = null,
    deadline_ms: ?i64 = null,
    cancel_error: anyerror = error.ShutdownRequested,
    deadline_error: anyerror = error.RunDeadlineExceeded,
};

fn checkAbort(opts: RetryOptions) !void {
    if (opts.cancel_flag) |flag| {
        if (!flag.load(.acquire)) return opts.cancel_error;
    }
    if (opts.deadline_ms) |deadline| {
        if (std.time.milliTimestamp() > deadline) return opts.deadline_error;
    }
}

fn sleepCooperative(delay_ms: u64, opts: RetryOptions) !void {
    var remaining = delay_ms;
    while (true) {
        try checkAbort(opts);
        if (remaining == 0) return;
        const slice_ms: u64 = @min(remaining, 100);
        std.Thread.sleep(slice_ms * std.time.ns_per_ms);
        remaining -= slice_ms;
    }
}

pub fn call(
    comptime T: type,
    ctx: anytype,
    comptime operation: fn (@TypeOf(ctx), u32) anyerror!T,
    opts: RetryOptions,
) !T {
    return callWithDetail(T, ctx, operation, struct {
        fn detail(_: @TypeOf(ctx), _: anyerror) ?[]const u8 {
            return null;
        }
    }.detail, opts);
}

pub fn callWithDetail(
    comptime T: type,
    ctx: anytype,
    comptime operation: fn (@TypeOf(ctx), u32) anyerror!T,
    comptime detail_for_error: fn (@TypeOf(ctx), anyerror) ?[]const u8,
    opts: RetryOptions,
) !T {
    var attempt: u32 = 0;

    while (true) {
        try checkAbort(opts);
        const result = operation(ctx, attempt) catch |err| {
            const classified = classifier.classify(err, detail_for_error(ctx, err));
            if (!classified.retryable or attempt >= opts.max_retries) {
                metrics.incExternalFailure(classified.class);
                return err;
            }

            const retry_after_hint = classified.retry_after_ms;
            const delay_ms = retry_after_hint orelse
                backoff.expBackoffJitter(attempt, opts.base_delay_ms, opts.max_delay_ms);

            metrics.incExternalRetry(classified.class);
            metrics.addBackoffWaitMs(delay_ms);
            if (retry_after_hint != null) metrics.incRetryAfterHintsApplied();

            log.warn("reliability.retrying op={s} class={s} attempt={d}/{d} delay_ms={d} retry_after={} err={s}", .{
                opts.operation_name orelse "external_call",
                @tagName(classified.class),
                attempt + 1,
                opts.max_retries,
                delay_ms,
                retry_after_hint != null,
                @errorName(err),
            });

            try sleepCooperative(delay_ms, opts);
            attempt += 1;
            continue;
        };

        return result;
    }
}

test "reliable call retries and succeeds" {
    const Ctx = struct {
        var calls: u32 = 0;
    };

    const result = try call(i32, {}, struct {
        fn op(_: @TypeOf({}), _: u32) !i32 {
            Ctx.calls += 1;
            if (Ctx.calls < 3) return error.CommandTimedOut;
            return 42;
        }
    }.op, .{ .max_retries = 3, .base_delay_ms = 1, .max_delay_ms = 2 });

    try std.testing.expectEqual(@as(i32, 42), result);
    try std.testing.expectEqual(@as(u32, 3), Ctx.calls);
}

test "reliable call with detail respects retry-after classification path" {
    const Ctx = struct {
        var calls: u32 = 0;
    };

    const result = try callWithDetail(i32, {}, struct {
        fn op(_: @TypeOf({}), _: u32) !i32 {
            Ctx.calls += 1;
            if (Ctx.calls < 3) return error.PrRateLimited;
            return 7;
        }
    }.op, struct {
        fn detail(_: @TypeOf({}), _: anyerror) ?[]const u8 {
            return "HTTP 429\nRetry-After: 0\n";
        }
    }.detail, .{ .max_retries = 3, .base_delay_ms = 1, .max_delay_ms = 2 });

    try std.testing.expectEqual(@as(i32, 7), result);
    try std.testing.expectEqual(@as(u32, 3), Ctx.calls);
}

test "reliable call does not retry non-retryable classified failures" {
    const Ctx = struct {
        var calls: u32 = 0;
    };

    try std.testing.expectError(error.MissingConfig, callWithDetail(i32, {}, struct {
        fn op(_: @TypeOf({}), _: u32) !i32 {
            Ctx.calls += 1;
            return error.MissingConfig;
        }
    }.op, struct {
        fn detail(_: @TypeOf({}), _: anyerror) ?[]const u8 {
            return null;
        }
    }.detail, .{ .max_retries = 3, .base_delay_ms = 1, .max_delay_ms = 2 }));

    try std.testing.expectEqual(@as(u32, 1), Ctx.calls);
}

test "integration: reliable call retries until max_retries then returns error" {
    const Ctx = struct {
        var calls: u32 = 0;
    };

    try std.testing.expectError(error.CommandTimedOut, call(i32, {}, struct {
        fn op(_: @TypeOf({}), _: u32) !i32 {
            Ctx.calls += 1;
            return error.CommandTimedOut;
        }
    }.op, .{ .max_retries = 2, .base_delay_ms = 1, .max_delay_ms = 2 }));

    try std.testing.expectEqual(@as(u32, 3), Ctx.calls);
}

test "reliable call aborts immediately when cancel flag is false" {
    var running = std.atomic.Value(bool).init(false);
    try std.testing.expectError(error.ShutdownRequested, call(i32, {}, struct {
        fn op(_: @TypeOf({}), _: u32) !i32 {
            return 1;
        }
    }.op, .{
        .max_retries = 1,
        .base_delay_ms = 1,
        .max_delay_ms = 2,
        .cancel_flag = &running,
    }));
}

test "integration: reliable call aborts during retry backoff on deadline" {
    const Ctx = struct {
        var calls: u32 = 0;
    };
    Ctx.calls = 0;
    try std.testing.expectError(error.RunDeadlineExceeded, call(i32, {}, struct {
        fn op(_: @TypeOf({}), _: u32) !i32 {
            Ctx.calls += 1;
            return error.CommandTimedOut;
        }
    }.op, .{
        .max_retries = 3,
        .base_delay_ms = 200,
        .max_delay_ms = 200,
        .deadline_ms = std.time.milliTimestamp() + 10,
    }));
    try std.testing.expect(Ctx.calls >= 1);
}

const CancelFlipCtx = struct {
    running: *std.atomic.Value(bool),
    delay_ms: u64,
};

fn flipCancelFlagAfterDelay(ctx: CancelFlipCtx) void {
    std.Thread.sleep(ctx.delay_ms * std.time.ns_per_ms);
    ctx.running.store(false, .release);
}

test "integration: reliable call aborts during retry backoff when cancel flag flips" {
    const Ctx = struct {
        var calls: u32 = 0;
    };
    Ctx.calls = 0;

    var running = std.atomic.Value(bool).init(true);
    const flipper = try std.Thread.spawn(.{}, flipCancelFlagAfterDelay, .{CancelFlipCtx{
        .running = &running,
        .delay_ms = 20,
    }});
    defer flipper.join();

    try std.testing.expectError(error.ShutdownRequested, call(i32, {}, struct {
        fn op(_: @TypeOf({}), _: u32) !i32 {
            Ctx.calls += 1;
            return error.CommandTimedOut;
        }
    }.op, .{
        .max_retries = 5,
        .base_delay_ms = 200,
        .max_delay_ms = 200,
        .cancel_flag = &running,
    }));
    try std.testing.expect(Ctx.calls >= 1);
    try std.testing.expect(Ctx.calls <= 2);
}
