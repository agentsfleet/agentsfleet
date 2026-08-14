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

const RouteTraits = struct {
    runner: bool = false,
    noisy_success: bool = false,
};

var runner_rejections = std.atomic.Value(u64).init(0);
var server_errors = std.atomic.Value(u64).init(0);
var sampled_successes = std.atomic.Value(u64).init(0);

fn classify(route: router.Route) RouteTraits {
    return switch (route) {
        .healthz,
        .readyz,
        => .{ .noisy_success = true },
        .runner_heartbeat,
        .runner_lease,
        .runner_report,
        .runner_activity,
        .runner_renew,
        => .{ .runner = true, .noisy_success = true },
        .register_runner,
        .runner_self,
        .runner_credentials_mint,
        .runner_memory_hydrate,
        .runner_memory_capture,
        .runner_bundle,
        => .{ .runner = true },
        // Everything else is an ordinary tenant-facing request: not a runner
        // call, and not noisy enough on success to need its own budget. The
        // default is the conservative one — an unlisted route is traced
        // normally rather than silently suppressed — so a route added without
        // touching this file loses no telemetry.
        else => .{},
    };
}

fn admit(window: *std.atomic.Value(u64), now_second: u64, limit: u32) bool {
    const epoch = now_second & EPOCH_MASK;
    while (true) {
        const old = window.load(.acquire);
        const old_epoch = old >> 32;
        const old_count: u32 = @intCast(old & EPOCH_MASK);
        if (old_epoch > epoch) return false;
        const next = if (old_epoch < epoch)
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

/// Derive the exported epoch end from one wall-clock read plus boot-clock
/// elapsed time. A regressed test clock clamps to zero elapsed; addition
/// saturates instead of wrapping an OpenTelemetry timestamp.
pub fn endEpochNanos(wall_start_ns: u64, boot_start_ns: i96, boot_end_ns: i96) u64 {
    if (boot_end_ns <= boot_start_ns) return wall_start_ns;
    const elapsed = std.math.cast(u64, boot_end_ns - boot_start_ns) orelse std.math.maxInt(u64);
    return wall_start_ns +| elapsed;
}

pub fn decide(route: router.Route, status: u16, span_id: []const u8, monotonic_second: u64) Decision {
    const traits = classify(route);
    if (status >= 500) {
        if (admit(&server_errors, monotonic_second, SERVER_ERROR_LIMIT)) return .emit;
        return .{ .suppress = .server_error_budget };
    }
    if (status >= 400 and traits.runner) {
        if (admit(&runner_rejections, monotonic_second, RUNNER_REJECTION_LIMIT)) return .emit;
        return .{ .suppress = .runner_rejection_budget };
    }
    if (status < 400 and traits.noisy_success) return .{ .suppress = .noisy_route };
    if (!isSampled(span_id)) return .{ .suppress = .sample_miss };
    if (admit(&sampled_successes, monotonic_second, SAMPLED_SUCCESS_LIMIT)) return .emit;
    return .{ .suppress = .sampled_success_budget };
}

pub fn resetForTest() void {
    runner_rejections.store(0, .release);
    server_errors.store(0, .release);
    sampled_successes.store(0, .release);
}

/// Every route whose traits are NOT the default, named so the test below can
/// prove no other route quietly acquired a runner budget or lost its success
/// span. Replaces what the old exhaustive `.{}` arm bought, without listing
/// every route in the system to buy it.
const RUNNER_NOISY_ROUTES = [_][]const u8{
    "runner_heartbeat", "runner_lease", "runner_report", "runner_activity", "runner_renew",
};
const RUNNER_QUIET_ROUTES = [_][]const u8{
    "register_runner",       "runner_self",           "runner_credentials_mint",
    "runner_memory_hydrate", "runner_memory_capture", "runner_bundle",
};
const PROBE_ROUTES = [_][]const u8{ "healthz", "readyz" };

fn expectedTraits(tag_name: []const u8) RouteTraits {
    for (PROBE_ROUTES) |n| if (std.mem.eql(u8, n, tag_name)) return .{ .noisy_success = true };
    for (RUNNER_NOISY_ROUTES) |n| if (std.mem.eql(u8, n, tag_name)) return .{ .runner = true, .noisy_success = true };
    for (RUNNER_QUIET_ROUTES) |n| if (std.mem.eql(u8, n, tag_name)) return .{ .runner = true };
    return .{};
}

test "every route's traits are the default unless it is one of the thirteen named" {
    // The `else` arm means a new route no longer fails compilation here. This
    // walks the whole union instead: a route that silently gained
    // `noisy_success` would lose its success spans without anyone noticing,
    // and one that silently gained `runner` would be charged the wrong budget.
    const info = @typeInfo(router.Route).@"union";
    inline for (info.fields) |f| {
        // SAFETY: classify switches on the tag only and reads no payload.
        const route: router.Route = @unionInit(router.Route, f.name, undefined);
        try std.testing.expectEqual(expectedTraits(f.name), classify(route));
    }
}

test {
    _ = @import("route_trace_test.zig");
}
