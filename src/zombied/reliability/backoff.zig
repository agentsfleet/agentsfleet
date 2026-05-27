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

test "expBackoffJitter returns 0 when base_ms is 0" {
    try std.testing.expectEqual(@as(u64, 0), expBackoffJitter(0, 0, 30_000));
    try std.testing.expectEqual(@as(u64, 0), expBackoffJitter(5, 0, 30_000));
}

test "expBackoffJitter first attempt (0) stays at base range" {
    // attempt=0 → 2^0 = 1 → raw = base*1 → range [base/2, base]
    const d = expBackoffJitter(0, 2_000, 16_000);
    try std.testing.expect(d >= 1_000);
    try std.testing.expect(d <= 2_000);
}

test "expBackoffJitter worker error scenario: poll_interval 2s, max 8x" {
    // Worker uses: base = poll_interval_ms (2000), max = poll_interval_ms * 8 (16000)
    // After 4 consecutive errors (attempt=3): 2000*2^3 = 16000 → capped at 16000
    const poll_ms: u64 = 2_000;
    const max_delay_ms = std.math.mul(u64, poll_ms, 8) catch poll_ms;
    const d = expBackoffJitter(3, poll_ms, max_delay_ms);
    try std.testing.expect(d >= 8_000); // half of 16000
    try std.testing.expect(d <= 16_000); // capped at max
}

test "expBackoffJitter handles overflow gracefully — falls back to max_ms" {
    // Very high attempt + base should saturate to max without panic
    const d = expBackoffJitter(30, 1_000_000, 50_000);
    try std.testing.expect(d >= 25_000);
    try std.testing.expect(d <= 50_000);
}
