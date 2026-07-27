//! What one lease poll cost, plus the Redis-brownout bailout it feeds.
//!
//! Reported to the metrics registry exactly once on every exit path. An idle
//! poll reports zeroes rather than reporting nothing: an absent sample would
//! leave the idle case invisible, and the idle case is the whole reason these
//! counters exist.
//!
//! **The bailout.** `assign.selectInner` acquires a pooled Postgres connection
//! before the candidate loop and holds it to the end, while every candidate's
//! event read is a Redis round-trip — so a degraded Redis pins that connection
//! for one request timeout per remaining candidate without ever touching
//! Postgres. `redisBrownedOut` lets the loop stop after
//! `MAX_CONSECUTIVE_REDIS_FAILURES_PER_POLL` instead of paying every timeout.
//! Consecutive rather than cumulative: one candidate timing out is noise, a run
//! of them means the store is degraded and every remaining candidate will pay
//! the same timeout.
//!
//! File-as-struct because the counters and the policy reading them are one
//! behaviour bound to one piece of state; `assign.zig` is the only consumer.

const PollCost = @This();

const constants = @import("common");
const metrics = @import("../observability/metrics_counters.zig");

candidates_examined: u64 = 0,
db_roundtrips: u64 = 0,
/// Run length of candidates whose Redis read failed, reset by any candidate
/// whose read reached a verdict — a degraded store, not scattered noise.
redis_failures_in_a_row: u32 = 0,

pub fn countDb(self: *PollCost, roundtrips: u64) void {
    self.db_roundtrips += roundtrips;
}

pub fn noteRedisFailure(self: *PollCost) void {
    self.redis_failures_in_a_row += 1;
}

pub fn noteRedisReachable(self: *PollCost) void {
    self.redis_failures_in_a_row = 0;
}

/// True once the failure run reaches the bailout threshold.
pub fn redisBrownedOut(self: *const PollCost) bool {
    return self.redis_failures_in_a_row >= constants.MAX_CONSECUTIVE_REDIS_FAILURES_PER_POLL;
}

pub fn report(self: *const PollCost) void {
    metrics.observeLeasePoll(self.candidates_examined, self.db_roundtrips);
}

test {
    _ = @import("PollCost_test.zig");
}
