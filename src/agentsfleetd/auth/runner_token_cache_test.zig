//! Proofs for the runner token verdict memo.
//!
//! The load-bearing ones are the negatives: an expired verdict must stop
//! answering, an invalidated runner must stop answering, and a slot recycled by
//! a colliding token must never hand back the previous runner's identity. A
//! false positive here is an authentication decision, not a wasted round-trip.

const std = @import("std");
const constants = @import("common");
const cache = @import("runner_token_cache.zig");

const HASH_A = "a" ** cache.TOKEN_HASH_HEX_LEN;
const HASH_B = "b" ** cache.TOKEN_HASH_HEX_LEN;
const RUNNER_A = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d7a01";
const RUNNER_B = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d7b01";
const EPOCH_MS: i64 = 1_700_000_000_000;

test "a cold table answers nothing" {
    cache.resetForTest();
    try std.testing.expect(cache.get(HASH_A, EPOCH_MS) == null);
}

test "a stored verdict is returned before it expires" {
    cache.resetForTest();
    cache.put(HASH_A, RUNNER_A, true, EPOCH_MS, cache.generation());

    const hit = cache.get(HASH_A, EPOCH_MS + cache.ENTRY_TTL_MS - 1) orelse return error.ExpectedHit;
    try std.testing.expectEqualStrings(RUNNER_A, hit.runnerId());
    try std.testing.expect(hit.active);
}

test "a verdict stops answering the instant it expires" {
    cache.resetForTest();
    cache.put(HASH_A, RUNNER_A, true, EPOCH_MS, cache.generation());

    // The boundary is exclusive: at exactly now + TTL the entry is already gone,
    // so the window a revoked runner survives can never exceed one TTL.
    try std.testing.expect(cache.get(HASH_A, EPOCH_MS + cache.ENTRY_TTL_MS) == null);
    try std.testing.expect(cache.get(HASH_A, EPOCH_MS + cache.ENTRY_TTL_MS + 60_000) == null);
}

test "an expired read drops the row rather than leaving it to answer later" {
    cache.resetForTest();
    cache.put(HASH_A, RUNNER_A, true, EPOCH_MS, cache.generation());
    _ = cache.get(HASH_A, EPOCH_MS + cache.ENTRY_TTL_MS);

    // Reading with a clock BEFORE the original expiry must still miss: the row
    // is gone, not merely hidden. Guards a reader that only compared timestamps.
    try std.testing.expect(cache.get(HASH_A, EPOCH_MS) == null);
}

test "a non-active verdict is remembered as non-active" {
    cache.resetForTest();
    cache.put(HASH_A, RUNNER_A, false, EPOCH_MS, cache.generation());

    const hit = cache.get(HASH_A, EPOCH_MS + 1) orelse return error.ExpectedHit;
    try std.testing.expect(!hit.active);
}

test "an unknown token hash never reads another runner's verdict" {
    cache.resetForTest();
    cache.put(HASH_A, RUNNER_A, true, EPOCH_MS, cache.generation());
    try std.testing.expect(cache.get(HASH_B, EPOCH_MS + 1) == null);
}

test "a recycled slot reads as absent, never as the runner it replaced" {
    cache.resetForTest();
    cache.put(HASH_A, RUNNER_A, true, EPOCH_MS, cache.generation());
    cache.put(HASH_B, RUNNER_B, true, EPOCH_MS, cache.generation());

    // The table is direct-mapped, so whether these two share a slot depends on
    // the hash. Either way the invariant holds: the most recent write is always
    // readable, and the other is EITHER itself OR gone — never the other's id.
    const b = cache.get(HASH_B, EPOCH_MS + 1) orelse return error.ExpectedHit;
    try std.testing.expectEqualStrings(RUNNER_B, b.runnerId());
    if (cache.get(HASH_A, EPOCH_MS + 1)) |a| try std.testing.expectEqualStrings(RUNNER_A, a.runnerId());
}

test "invalidating a runner drops its verdict at once" {
    cache.resetForTest();
    cache.put(HASH_A, RUNNER_A, true, EPOCH_MS, cache.generation());
    cache.invalidateRunner(RUNNER_A);

    // Well inside the TTL — the drop is what stops it, not the clock.
    try std.testing.expect(cache.get(HASH_A, EPOCH_MS + 1) == null);
}

test "invalidating one runner leaves an unrelated runner's verdict intact" {
    cache.resetForTest();
    cache.put(HASH_A, RUNNER_A, true, EPOCH_MS, cache.generation());
    cache.put(HASH_B, RUNNER_B, true, EPOCH_MS, cache.generation());
    cache.invalidateRunner(RUNNER_A);

    if (cache.get(HASH_B, EPOCH_MS + 1)) |b| {
        try std.testing.expectEqualStrings(RUNNER_B, b.runnerId());
    } else {
        // Only legitimate if the two hashed to one slot, in which case B evicted
        // A and the invalidate found nothing to drop — still never a wrong id.
        try std.testing.expect(cache.get(HASH_A, EPOCH_MS + 1) == null);
    }
}

test "invalidating a runner that was never stored is a no-op" {
    cache.resetForTest();
    cache.put(HASH_A, RUNNER_A, true, EPOCH_MS, cache.generation());
    cache.invalidateRunner(RUNNER_B);

    const hit = cache.get(HASH_A, EPOCH_MS + 1) orelse return error.ExpectedHit;
    try std.testing.expectEqualStrings(RUNNER_A, hit.runnerId());
}

test "a token hash of the wrong width is neither stored nor answered" {
    cache.resetForTest();
    cache.put("deadbeef", RUNNER_A, true, EPOCH_MS, cache.generation());
    try std.testing.expect(cache.get("deadbeef", EPOCH_MS + 1) == null);
    // And a short prefix of a real hash must not match a stored full-width one.
    cache.put(HASH_A, RUNNER_A, true, EPOCH_MS, cache.generation());
    try std.testing.expect(cache.get(HASH_A[0..32], EPOCH_MS + 1) == null);
}

test "an oversized runner id is skipped rather than truncated" {
    cache.resetForTest();
    const too_long = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d7a01-overflow";
    cache.put(HASH_A, too_long, true, EPOCH_MS, cache.generation());

    // Storing a truncated id would authenticate a DIFFERENT runner; refusing the
    // memo only costs the Postgres read it exists to save.
    try std.testing.expect(cache.get(HASH_A, EPOCH_MS + 1) == null);
}

test "an empty runner id is refused" {
    cache.resetForTest();
    cache.put(HASH_A, "", true, EPOCH_MS, cache.generation());
    try std.testing.expect(cache.get(HASH_A, EPOCH_MS + 1) == null);
}

test "an invalidation mid-lookup refuses the verdict that lookup was about to store" {
    cache.resetForTest();
    // The exact interleaving: a request reads the generation, its Postgres read
    // is in flight, the operator revokes (invalidating an EMPTY table), and only
    // then does the request try to store what it read. Storing it would put a
    // pre-revoke `active` verdict back for a full window on the very machine
    // that served the revoke.
    const seen = cache.generation();
    cache.invalidateRunner(RUNNER_A);
    cache.put(HASH_A, RUNNER_A, true, EPOCH_MS, seen);

    try std.testing.expect(cache.get(HASH_A, EPOCH_MS + 1) == null);
}

test "an invalidation for one runner does not refuse another runner's in-flight store" {
    cache.resetForTest();
    // The counter is global, so a bump refuses every in-flight store, not just
    // the invalidated runner's. That costs one extra Postgres read and is the
    // safe direction — but the store must succeed once re-read against the
    // current generation, or the memo would never refill under churn.
    const stale = cache.generation();
    cache.invalidateRunner(RUNNER_A);
    cache.put(HASH_B, RUNNER_B, true, EPOCH_MS, stale);
    try std.testing.expect(cache.get(HASH_B, EPOCH_MS + 1) == null);

    cache.put(HASH_B, RUNNER_B, true, EPOCH_MS, cache.generation());
    const hit = cache.get(HASH_B, EPOCH_MS + 1) orelse return error.ExpectedHit;
    try std.testing.expectEqualStrings(RUNNER_B, hit.runnerId());
}

test "the trusted window is one runner heartbeat" {
    // The window in which a cordoned or revoked runner still authenticates
    // against a machine that did not serve the operator's request. Pinning it to
    // the heartbeat keeps that inside one liveness cycle; a bare number here
    // would let the two drift apart silently.
    try std.testing.expectEqual(constants.HEARTBEAT_INTERVAL_MS, cache.ENTRY_TTL_MS);
}
