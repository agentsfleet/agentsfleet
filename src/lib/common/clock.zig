//! Wall-clock helper. Zig 0.16 removed `std.time.milliTimestamp` / `nanoTimestamp`
//! (wall-clock time moved behind the `Io` interface). These read `REALTIME`
//! directly via the libc syscall so every call site stays a drop-in swap and no
//! `Io` has to be threaded through the wall-clock sites. Same epoch semantics as
//! the removed std functions: nanoseconds/milliseconds since the Unix epoch.
//! Mirrors the org pattern (nullclaw's compat shim). Targets are POSIX
//! (Linux deploy, macOS dev); no Windows/WASI branch exists because no such
//! target is built (RULE NDC).

const std = @import("std");

/// Wall-clock milliseconds since the Unix epoch. Drop-in replacement for the
/// `std.time.milliTimestamp()` removed in Zig 0.16.
pub fn nowMillis() i64 {
    return @intCast(@divTrunc(nowNanos(), std.time.ns_per_ms));
}

/// Wall-clock seconds since the Unix epoch. Drop-in replacement for the
/// `std.time.timestamp()` removed in Zig 0.16.
pub fn nowSeconds() i64 {
    return @intCast(@divTrunc(nowNanos(), std.time.ns_per_s));
}

/// Wall-clock nanoseconds since the Unix epoch. Drop-in replacement for the
/// `std.time.nanoTimestamp()` removed in Zig 0.16.
pub fn nowNanos() i128 {
    // SAFETY: clock_gettime fully populates ts before sec/nsec are read.
    var ts: std.posix.timespec = undefined;
    return switch (std.posix.errno(std.posix.system.clock_gettime(.REALTIME, &ts))) {
        .SUCCESS => @as(i128, ts.sec) * std.time.ns_per_s + ts.nsec,
        // CLOCK_REALTIME can only fail with EFAULT/EINVAL — both programmer
        // errors given the stack `timespec` + hard-coded clock id. Fail closed:
        // a silent epoch-0 return would corrupt UUIDv7 timestamp ordering (the
        // ids stay unique, but stop sorting by mint time) and make the redis
        // pool's acquire deadline never fire (it loops forever).
        else => |err| std.debug.panic("clock_gettime(CLOCK_REALTIME) failed: {s}", .{@tagName(err)}),
    };
}

/// First instant of the UTC calendar month containing `now_ms`, in epoch ms.
///
/// A PURE function of its argument — it never reads the clock, so the budget
/// gates can pass one `now_ms` into both their day and month windows and the
/// two can never straddle a tick (RULE TIM: timing invariants are explicit).
///
/// Pre-epoch input is not a reachable state (every caller passes `nowMillis()`),
/// but the `u47` day cast would trap on a negative day index, so it clamps to
/// the epoch rather than aborting the daemon.
pub fn startOfUtcMonthMillis(now_ms: i64) i64 {
    if (now_ms <= 0) return 0;
    const day_index = @divFloor(now_ms, std.time.ms_per_day);
    const epoch_day = std.time.epoch.EpochDay{ .day = @intCast(day_index) };
    const month_day = epoch_day.calculateYearDay().calculateMonthDay();
    // `day_index` counts days since the epoch; `day_index_in_month` is 0-based,
    // so subtracting it lands on the month's first day.
    return (day_index - month_day.day_index) * std.time.ms_per_day;
}

test "startOfUtcMonthMillis truncates to the first instant of the UTC month" {
    // 2026-07-10T16:04:00Z → 2026-07-01T00:00:00Z
    try std.testing.expectEqual(@as(i64, 1_782_864_000_000), startOfUtcMonthMillis(1_783_699_440_000));
    // Already exactly the month start → itself (idempotent).
    try std.testing.expectEqual(@as(i64, 1_782_864_000_000), startOfUtcMonthMillis(1_782_864_000_000));
    // One millisecond before a month start belongs to the PREVIOUS month:
    // 2026-06-30T23:59:59.999Z → 2026-06-01T00:00:00Z
    try std.testing.expectEqual(@as(i64, 1_780_272_000_000), startOfUtcMonthMillis(1_782_863_999_999));
}

test "startOfUtcMonthMillis handles leap-year February" {
    // 2024-02-29T23:59:59.999Z → 2024-02-01T00:00:00Z
    try std.testing.expectEqual(@as(i64, 1_706_745_600_000), startOfUtcMonthMillis(1_709_251_199_999));
    // 2024-03-01T00:00:00Z → itself, proving the leap day did not bleed forward.
    try std.testing.expectEqual(@as(i64, 1_709_251_200_000), startOfUtcMonthMillis(1_709_251_200_000));
}

test "startOfUtcMonthMillis clamps pre-epoch input instead of trapping" {
    try std.testing.expectEqual(@as(i64, 0), startOfUtcMonthMillis(0));
    try std.testing.expectEqual(@as(i64, 0), startOfUtcMonthMillis(-1));
}

test "nowMillis returns wall-clock time, not a small monotonic counter" {
    const ms = nowMillis();
    // > Jan 1 2020 in epoch ms — proves it is wall time, not a boot-relative
    // counter and not seconds/nanos at the wrong scale.
    try std.testing.expect(ms > 1_577_836_800_000);
    // nanos and millis agree in magnitude (nanos ≈ millis × 1e6).
    const ns = nowNanos();
    try std.testing.expect(ns > @as(i128, ms) * std.time.ns_per_ms - std.time.ns_per_s);
}
