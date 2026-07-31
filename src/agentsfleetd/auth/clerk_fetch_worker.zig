//! Bounded accounting for the Clerk metadata fire-and-forget workers
//! (`clerk_backend.zig`). A webhook burst must not spawn threads without
//! limit, and shutdown must not hang on a stalled vendor: submissions claim a
//! slot from a fixed budget (beyond it they are rejected and logged by the
//! caller), and teardown waits a bounded interval for in-flight fetches.
//! std + common only — same portability wall as the rest of
//! `src/agentsfleetd/auth/`.

const std = @import("std");
const common = @import("common");
const logging = @import("log");
const ec = @import("auth_codes");

const log = logging.scoped(.clerk_backend);

/// Concurrent metadata fetches allowed. Signup webhooks arrive at human
/// cadence; a burst past this is dropped (the metadata write is best-effort
/// and operator-repairable via the Clerk dashboard) instead of growing
/// threads without bound.
pub const MAX_IN_FLIGHT_FETCHES: u32 = 8;
/// Bounded shutdown drain. A straggler past this owns only its self-lifetime
/// job memory (freed by its own thread; the process is exiting), so timing
/// out is safe — nothing shared is freed underneath it.
const SHUTDOWN_DRAIN_TIMEOUT_MS: u64 = 5_000;
const DRAIN_POLL_INTERVAL_MS: u64 = 25;

var in_flight = std.atomic.Value(u32).init(0);

/// Claim a worker slot; false when the budget is exhausted. Pair every true
/// with exactly one `releaseSlot` (worker exit or spawn failure).
pub fn tryAcquireSlot() bool {
    // safe because: pure slot counting — no memory is published or consumed
    // across this counter; the job handoff happens via Thread.spawn.
    const prev = in_flight.fetchAdd(1, .monotonic);
    if (prev >= MAX_IN_FLIGHT_FETCHES) {
        _ = in_flight.fetchSub(1, .monotonic); // safe because: undo of the claim above.
        return false;
    }
    return true;
}

pub fn releaseSlot() void {
    // safe because: slot release only; no ordering-dependent data rides it.
    _ = in_flight.fetchSub(1, .monotonic);
}

/// Wait (bounded) for in-flight metadata fetches at shutdown. Logs and
/// returns on timeout rather than hanging teardown on a stalled vendor.
pub fn drainForShutdown() void {
    var waited_ms: u64 = 0;
    // safe because: monotonic poll of the counter; the loop body owns no data.
    while (in_flight.load(.monotonic) > 0 and waited_ms < SHUTDOWN_DRAIN_TIMEOUT_MS) {
        common.sleepNanos(DRAIN_POLL_INTERVAL_MS * std.time.ns_per_ms);
        waited_ms += DRAIN_POLL_INTERVAL_MS;
    }
    const left = in_flight.load(.monotonic); // safe because: diagnostic read only.
    if (left > 0) {
        log.warn("shutdown_drain_timeout", .{
            .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
            .in_flight = left,
        });
    }
}

test "slots reject beyond the budget and recover on release" {
    var claimed: u32 = 0;
    while (tryAcquireSlot()) claimed += 1;
    try std.testing.expectEqual(MAX_IN_FLIGHT_FETCHES, claimed);
    // Budget exhausted: the next claim is refused, nothing is spawned.
    try std.testing.expect(!tryAcquireSlot());
    releaseSlot();
    try std.testing.expect(tryAcquireSlot());
    var i: u32 = 0;
    while (i < claimed) : (i += 1) releaseSlot();
    try std.testing.expectEqual(@as(u32, 0), in_flight.load(.monotonic));
}

test "drainForShutdown returns once a held slot is released by a worker" {
    try std.testing.expect(tryAcquireSlot());
    const Worker = struct {
        fn run() void {
            common.sleepNanos(50 * std.time.ns_per_ms);
            releaseSlot();
        }
    };
    const t = try std.Thread.spawn(.{}, Worker.run, .{});
    drainForShutdown();
    // Asserted BEFORE join: a drain that returns without waiting reads the
    // slot the worker still holds for ~50 ms and fails here — join() alone
    // would mask a no-op drain by doing the waiting itself.
    try std.testing.expectEqual(@as(u32, 0), in_flight.load(.monotonic));
    t.join();
}
