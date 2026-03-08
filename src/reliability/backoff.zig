//! Exponential backoff helpers with bounded jitter.

const std = @import("std");

fn pow2Clamped(attempt: u32) u64 {
    var out: u64 = 1;
    var i: u32 = 0;
    const cap = @min(attempt, 20);
    while (i < cap) : (i += 1) {
        out *= 2;
    }
    return out;
}

/// Returns delay in milliseconds in range [base*2^n/2, base*2^n] capped by max_ms.
pub fn expBackoffJitter(attempt: u32, base_ms: u64, max_ms: u64) u64 {
    if (base_ms == 0) return 0;

    const growth = pow2Clamped(attempt);
    const raw = std.math.mul(u64, base_ms, growth) catch max_ms;
    const capped = @min(raw, max_ms);

    if (capped <= 1) return capped;

    const half = capped / 2;
    const span = capped - half;
    const jitter = @as(u64, std.crypto.random.int(u16)) % (span + 1);
    return half + jitter;
}

test "expBackoffJitter stays within bounded window" {
    const d = expBackoffJitter(3, 500, 30_000);
    // base*2^3 = 4000 => expected range [2000, 4000]
    try std.testing.expect(d >= 2_000);
    try std.testing.expect(d <= 4_000);
}

test "expBackoffJitter respects max cap" {
    const d = expBackoffJitter(20, 500, 30_000);
    try std.testing.expect(d >= 15_000);
    try std.testing.expect(d <= 30_000);
}
