//! backoff.zig — bounded, jittered exponential backoff for control-plane
//! retries (M100). One helper replaces the ad-hoc per-site multipliers so
//! a persistent control-plane outage can never grow the retry sleep without
//! bound (Invariant 4) and a thundering herd of runners never re-synchronises
//! on a fixed interval (the jitter de-correlates them).
//!
//! Shape: `delay = min(BASE_MS << attempt, MAX_BACKOFF_MS)`, then ±JITTER_PCT
//! jitter drawn from the kernel CSPRNG. Attempt 0 → ~BASE_MS; the pre-jitter
//! spine is monotonic until it saturates at MAX_BACKOFF_MS and never exceeds it.
//!
//! A namespace (no owned state). `ms()` is the production entry; `jittered()`
//! is the pure, randomness-injected core the tests drive deterministically.

const std = @import("std");
const secureRandomBytes = @import("random.zig").secureRandomBytes;

/// Delay for attempt 0; doubles each attempt until the cap. Single-sourced
/// (RULE UFS) — the retry sites reference this, never a bare literal.
pub const BASE_MS: u64 = 2_000;
/// Hard ceiling: the pre-jitter delay never exceeds this (Invariant 4).
pub const MAX_BACKOFF_MS: u64 = 30_000;
/// Jitter band as a percent of the capped delay, applied symmetrically (±).
pub const JITTER_PCT: u64 = 20;
/// Shift ceiling so `BASE_MS << attempt` can neither overflow nor pointlessly
/// recompute once the cap is reached (BASE_MS << 16 already dwarfs the cap).
const SHIFT_CAP: u32 = 16;

/// Production entry: capped exponential backoff (ms) for a 0-based `attempt`
/// with ±JITTER_PCT jitter from the kernel CSPRNG. Never panics; never exceeds
/// MAX_BACKOFF_MS + its jitter band.
pub fn ms(attempt: u32) u64 {
    var buf: [8]u8 = undefined;
    secureRandomBytes(&buf) catch {
        // CSPRNG genuinely unavailable (not observed in practice) → fall back to
        // the un-jittered but still-capped spine rather than panic.
        return cappedMs(attempt);
    };
    return jittered(attempt, std.mem.readInt(u64, &buf, .little));
}

/// The monotonic, un-jittered spine: `min(BASE_MS << attempt, MAX_BACKOFF_MS)`,
/// saturating so a large attempt can't overflow the shift or the multiply.
pub fn cappedMs(attempt: u32) u64 {
    const shift: u6 = @intCast(@min(attempt, SHIFT_CAP));
    const grown = BASE_MS *| (@as(u64, 1) << shift);
    return @min(grown, MAX_BACKOFF_MS);
}

/// Pure jittered backoff with an injected random word — the deterministically
/// testable core. Result ∈ [capped - band, capped + band], where
/// band = capped * JITTER_PCT / 100.
pub fn jittered(attempt: u32, rand_word: u64) u64 {
    const capped = cappedMs(attempt);
    const band = capped * JITTER_PCT / 100;
    if (band == 0) return capped;
    const span = band * 2 + 1; // inclusive [-band, +band]
    const offset = rand_word % span; // [0, 2*band]
    return capped - band + offset;
}

// ── Tests ───────────────────────────────────────────────────────────────────

/// An attempt far past the shift cap — every path must already be saturated.
const SATURATED_ATTEMPT: u32 = 1000;

test "cappedMs is monotonic non-decreasing and saturates at MAX_BACKOFF_MS" {
    var prev: u64 = 0;
    for (0..40) |a| {
        const v = cappedMs(@intCast(a));
        try std.testing.expect(v >= prev); // monotonic up to the cap
        try std.testing.expect(v <= MAX_BACKOFF_MS); // never exceeds the cap
        prev = v;
    }
    // Attempt 0 is the base; a large attempt is pinned at the ceiling.
    try std.testing.expectEqual(BASE_MS, cappedMs(0));
    try std.testing.expectEqual(MAX_BACKOFF_MS, cappedMs(SATURATED_ATTEMPT));
}

test "jittered stays within the ±band for every random word, and never exceeds the cap+band" {
    // Sweep representative random words against several attempts; assert the band
    // bounds hold (a mutation widening/dropping the band, or removing the cap,
    // trips this).
    const words = [_]u64{ 0, 1, 7, 12345, std.math.maxInt(u64) / 2, std.math.maxInt(u64) };
    for (0..10) |a| {
        const capped = cappedMs(@intCast(a));
        const band = capped * JITTER_PCT / 100;
        for (words) |w| {
            const v = jittered(@intCast(a), w);
            try std.testing.expect(v >= capped - band);
            try std.testing.expect(v <= capped + band);
            try std.testing.expect(v <= MAX_BACKOFF_MS + (MAX_BACKOFF_MS * JITTER_PCT / 100));
        }
    }
}

test "jittered hits both band edges deterministically (low word → -band, high → +band)" {
    const capped = cappedMs(0);
    const band = capped * JITTER_PCT / 100;
    try std.testing.expect(band > 0);
    try std.testing.expectEqual(capped - band, jittered(0, 0)); // offset 0
    try std.testing.expectEqual(capped + band, jittered(0, 2 * band)); // offset 2*band
    // Mid word lands inside the band, not at an edge.
    const mid = jittered(0, band);
    try std.testing.expect(mid > capped - band and mid < capped + band);
}

test "ms() is bounded for any attempt (smoke: CSPRNG path, no panic)" {
    for ([_]u32{ 0, 1, 5, 50, SATURATED_ATTEMPT }) |a| {
        const v = ms(a);
        try std.testing.expect(v <= MAX_BACKOFF_MS + (MAX_BACKOFF_MS * JITTER_PCT / 100));
    }
}
