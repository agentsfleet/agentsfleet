//! Token-bucket rate limiting utilities.

const std = @import("std");

pub const TokenBucket = struct {
    capacity: f64,
    tokens: f64,
    refill_per_sec: f64,
    last_ms: i64,

    pub fn init(capacity: u32, refill_per_sec: f64, now_ms: i64) TokenBucket {
        const cap = @as(f64, @floatFromInt(capacity));
        return .{
            .capacity = cap,
            .tokens = cap,
            .refill_per_sec = refill_per_sec,
            .last_ms = now_ms,
        };
    }

    pub fn allow(self: *TokenBucket, now_ms: i64, cost: f64) bool {
        self.refill(now_ms);
        if (self.tokens >= cost) {
            self.tokens -= cost;
            return true;
        }
        return false;
    }

    pub fn waitMsUntil(self: *TokenBucket, now_ms: i64, cost: f64) u64 {
        self.refill(now_ms);
        if (self.tokens >= cost or self.refill_per_sec <= 0) return 0;

        const missing = cost - self.tokens;
        const secs = missing / self.refill_per_sec;
        if (secs <= 0) return 0;
        return @as(u64, @intFromFloat(std.math.ceil(secs * 1000.0)));
    }

    fn refill(self: *TokenBucket, now_ms: i64) void {
        if (now_ms <= self.last_ms) return;
        const dt_ms = now_ms - self.last_ms;
        const refill_tokens = (@as(f64, @floatFromInt(dt_ms)) / 1000.0) * self.refill_per_sec;
        self.tokens = @min(self.capacity, self.tokens + refill_tokens);
        self.last_ms = now_ms;
    }
};

test "token bucket allows within capacity" {
    var bucket = TokenBucket.init(3, 1.0, 0);
    try std.testing.expect(bucket.allow(0, 1.0));
    try std.testing.expect(bucket.allow(0, 1.0));
    try std.testing.expect(bucket.allow(0, 1.0));
    try std.testing.expect(!bucket.allow(0, 1.0));
}

test "token bucket refills over time" {
    var bucket = TokenBucket.init(2, 2.0, 0);
    _ = bucket.allow(0, 2.0);
    try std.testing.expect(!bucket.allow(0, 1.0));
    try std.testing.expect(bucket.allow(500, 1.0)); // +1 token
}

test "token bucket computes wait time" {
    var bucket = TokenBucket.init(1, 2.0, 0);
    _ = bucket.allow(0, 1.0);
    const wait_ms = bucket.waitMsUntil(0, 1.0); // 0.5s at 2 tokens/sec
    try std.testing.expect(wait_ms >= 500);
    try std.testing.expect(wait_ms <= 501);
}
