//! retry.zig — one bounded retry loop for a call that is safe to repeat.
//!
//! The Zig tree's retries were written inline at each call site, each its own
//! loop around the shared `backoff` delay. This is that loop, once. The caller
//! supplies the call, a `Policy` naming which errors a retry can fix and how
//! many attempts and how much time it may spend, and a pacer that owns the
//! clock. `RealPacer` is the production pacer; a test passes a fake clock and
//! never sleeps.
//!
//! Only for idempotent calls. The loop cannot tell a request that never left
//! from one that half-landed; `Policy.retryable` is where the caller says which
//! failures are the first kind.

const std = @import("std");
const backoff = @import("backoff.zig");
const clock = @import("clock.zig");

/// What one caller allows.
pub const Policy = struct {
    /// Attempts, including the first.
    max_attempts: u32,
    /// Time spent since the first attempt beyond which no further attempt
    /// starts. The whole call is bounded by this plus one attempt's own worst
    /// case, which is what a caller racing a deadline sizes it against.
    budget_ms: u64,
    /// True for an error a retry can fix. Anything else returns at once.
    retryable: *const fn (anyerror) bool,

    /// Whether the failed 0-based `attempt` earns another, after `elapsed_ms`
    /// spent and a `delay_ms` pause before it.
    pub fn allows(self: Policy, err: anyerror, attempt: u32, elapsed_ms: u64, delay_ms: u64) bool {
        if (!self.retryable(err)) return false;
        if (attempt +| 1 >= self.max_attempts) return false;
        return elapsed_ms +| delay_ms < self.budget_ms;
    }
};

/// Production pacing: wall time from `clock`, delays from the shared
/// `backoff`, and a real sleep on `io`.
pub const RealPacer = struct {
    io: std.Io,

    pub fn nowMs(_: RealPacer) u64 {
        return @intCast(@max(0, clock.nowMillis()));
    }

    pub fn delayMs(_: RealPacer, attempt: u32) u64 {
        return backoff.ms(attempt);
    }

    pub fn sleepMs(self: RealPacer, ms: u64) void {
        clock.sleepMs(self.io, ms);
    }
};

/// Call `fetcher.fetch()` until it succeeds or `policy` refuses, pausing
/// `pacer.delayMs(attempt)` between tries and reporting each retry through
/// `fetcher.onRetry(attempt, err, delay_ms)` — the caller holds the context a
/// useful log line needs. Returns exactly what `fetch` returns.
pub fn run(policy: Policy, fetcher: anytype, pacer: anytype) @TypeOf(fetcher.fetch()) {
    const started = pacer.nowMs();
    var attempt: u32 = 0;
    while (true) : (attempt += 1) {
        if (fetcher.fetch()) |value| return value else |err| {
            const delay_ms = pacer.delayMs(attempt);
            if (!policy.allows(err, attempt, pacer.nowMs() -| started, delay_ms)) return err;
            fetcher.onRetry(attempt, err, delay_ms);
            pacer.sleepMs(delay_ms);
        }
    }
}

test {
    _ = @import("retry_test.zig");
}
