//! Bounded admission policy for HTTP request spans.

const std = @import("std");
const router = @import("router.zig");

const TRACE_SAMPLE_SEED: u64 = 0x6d_313339_74726163;
const SAMPLE_DENOMINATOR: u64 = 100;
const EPOCH_MASK: u64 = 0xffff_ffff;
const RUNNER_REJECTION_LIMIT: u32 = 4;
const SERVER_ERROR_LIMIT: u32 = 4;
const SAMPLED_SUCCESS_LIMIT: u32 = 2;

pub const SuppressionReason = enum {
    noisy_route,
    runner_rejection_budget,
    server_error_budget,
    sampled_success_budget,
    sample_miss,
};

pub const Decision = union(enum) {
    emit,
    suppress: SuppressionReason,
};

var runner_rejections = std.atomic.Value(u64).init(0);
var server_errors = std.atomic.Value(u64).init(0);
var sampled_successes = std.atomic.Value(u64).init(0);

fn isRunner(route: router.Route) bool {
    return switch (route) {
        .register_runner,
        .runner_self,
        .runner_heartbeat,
        .runner_lease,
        .runner_report,
        .runner_credentials_mint,
        .runner_activity,
        .runner_renew,
        .runner_memory_hydrate,
        .runner_memory_capture,
        .runner_bundle,
        => true,
        else => false,
    };
}

fn isNoisySuccess(route: router.Route) bool {
    return switch (route) {
        .healthz,
        .readyz,
        .metrics,
        .runner_heartbeat,
        .runner_lease,
        .runner_report,
        .runner_activity,
        .runner_renew,
        => true,
        else => false,
    };
}

fn admit(window: *std.atomic.Value(u64), now_second: u64, limit: u32) bool {
    const epoch = now_second & EPOCH_MASK;
    while (true) {
        const old = window.load(.acquire);
        const old_epoch = old >> 32;
        const old_count: u32 = @intCast(old & EPOCH_MASK);
        const next = if (old_epoch != epoch)
            (epoch << 32) | 1
        else if (old_count < limit)
            (epoch << 32) | (@as(u64, old_count) + 1)
        else
            return false;
        // safe because: the packed epoch and count are one atomic admission state;
        // a failed compare-and-swap retries without exposing a partial reset.
        if (window.cmpxchgWeak(old, next, .acq_rel, .acquire) == null) return true;
    }
}

fn isSampled(span_id: []const u8) bool {
    return std.hash.Wyhash.hash(TRACE_SAMPLE_SEED, span_id) % SAMPLE_DENOMINATOR == 0;
}

pub fn decide(route: router.Route, status: u16, span_id: []const u8, monotonic_second: u64) Decision {
    if (status >= 500) {
        if (admit(&server_errors, monotonic_second, SERVER_ERROR_LIMIT)) return .emit;
        return .{ .suppress = .server_error_budget };
    }
    if (status >= 400 and isRunner(route)) {
        if (admit(&runner_rejections, monotonic_second, RUNNER_REJECTION_LIMIT)) return .emit;
        return .{ .suppress = .runner_rejection_budget };
    }
    if (status < 400 and isNoisySuccess(route)) return .{ .suppress = .noisy_route };
    if (status < 400) {
        if (!isSampled(span_id)) return .{ .suppress = .sample_miss };
        if (admit(&sampled_successes, monotonic_second, SAMPLED_SUCCESS_LIMIT)) return .emit;
        return .{ .suppress = .sampled_success_budget };
    }
    return .emit;
}

pub fn resetForTest() void {
    runner_rejections.store(0, .release);
    server_errors.store(0, .release);
    sampled_successes.store(0, .release);
}

test {
    _ = @import("route_trace_test.zig");
}
