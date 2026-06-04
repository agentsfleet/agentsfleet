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
    // SAFETY: clock_gettime fully populates ts before sec/nsec are read; on the
    // error path ts is untouched and the switch returns 0 without reading it.
    var ts: std.posix.timespec = undefined;
    return switch (std.posix.errno(std.posix.system.clock_gettime(.REALTIME, &ts))) {
        .SUCCESS => @as(i128, ts.sec) * std.time.ns_per_s + ts.nsec,
        else => 0,
    };
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
