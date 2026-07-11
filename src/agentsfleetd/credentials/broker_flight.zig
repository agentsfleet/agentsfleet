//! Cold-miss coordination for the credential broker: the per-key
//! single-flight guard plus the cache write-back. Implementation detail of
//! `broker.zig`, split out by concern (and the 350-line cap).
//!
//! Why single-flight: cache.zig's get/put does not coordinate concurrent
//! cold-misses — two simultaneous misses for one key would BOTH mint. For a
//! ROTATING refresh provider both mints post the same refresh token and the
//! provider's reuse detection can revoke the token family. The guard makes
//! exactly one caller mint per key; losers wait on the condition and re-read
//! what the winner cached.

const CredentialBroker = @import("broker.zig");

/// True → the caller owns the flight for `key` and MUST `endFlight` it.
/// False → another flight for the key completed while we waited; the caller
/// re-reads the cache (and contends again if the winner cached nothing).
pub fn beginFlight(self: *CredentialBroker, key: []const u8) bool {
    self.inflight_mutex.lock();
    defer self.inflight_mutex.unlock();
    if (self.inflight.contains(key)) {
        while (self.inflight.contains(key)) self.inflight_cond.wait(&self.inflight_mutex);
        return false;
    }
    // OOM degrade: fly unguarded — the pre-guard behavior, a rare double
    // mint bounded to the documented one-reconnect cost — rather than fail
    // a mint the caller needs. endFlight tolerates the missing entry.
    const owned = self.alloc.dupe(u8, key) catch return true;
    self.inflight.put(self.alloc, owned, {}) catch {
        self.alloc.free(owned);
        return true;
    };
    return true;
}

/// Release the flight for `key` and wake every waiter (each re-reads the
/// cache; on a failed mint the first one through takes its own flight).
pub fn endFlight(self: *CredentialBroker, key: []const u8) void {
    self.inflight_mutex.lock();
    defer self.inflight_mutex.unlock();
    if (self.inflight.fetchRemove(key)) |kv| self.alloc.free(kv.key);
    self.inflight_cond.broadcast();
}

/// Store a freshly-minted token (cache.zig owns the duped bytes; frees via
/// `removedFromCache` on eviction). A put failure is non-fatal — the caller
/// still gets its token, just without a cache entry.
pub fn cacheMinted(self: *CredentialBroker, key: []const u8, token: []const u8, expires_at_ms: i64, now_ms: i64) void {
    const owned = self.alloc.dupe(u8, token) catch return;
    self.store.put(key, .{ .token = owned, .expires_at_ms = expires_at_ms }, .{
        .ttl = ttlSeconds(expires_at_ms, now_ms),
    }) catch self.alloc.free(owned);
}

/// Floor for the cache.zig TTL (seconds); the broker's own `now_ms` skew
/// check is the authoritative expiry, so this is only a positive backstop.
const MIN_TTL_S: u32 = 1;

/// Ceiling for the cache.zig TTL (seconds). cache.zig stores the entry expiry
/// as `@as(u32, now_epoch_seconds) + ttl` (segment.zig) — a u32 add — so an
/// unbounded ttl (a never-expires `static` token) overflows it and panics.
/// One day is a safe backstop: the read-path skew check is authoritative.
const MAX_TTL_S: u32 = 24 * 60 * 60;

/// cache.zig expiry backstop (seconds). In production `now_ms` tracks wall
/// time so this matches the real remaining life.
fn ttlSeconds(expires_at_ms: i64, now_ms: i64) u32 {
    // Saturating subtraction: a never-expires token uses `maxInt(i64)`, and
    // `maxInt(i64) - negative_now` would overflow a plain i64 subtraction.
    const remaining_ms = expires_at_ms -| now_ms;
    if (remaining_ms <= 0) return MIN_TTL_S;
    const secs = @divFloor(remaining_ms, 1000);
    // Clamp to MAX_TTL_S, never `maxInt(u32)`: cache.zig adds epoch seconds
    // to this ttl in a u32, so the type max is guaranteed to overflow it.
    if (secs >= MAX_TTL_S) return MAX_TTL_S;
    return @max(MIN_TTL_S, @as(u32, @intCast(secs)));
}
