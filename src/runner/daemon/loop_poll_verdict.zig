//! loop_poll_verdict.zig — whether a worker may lease at all, as a pure verdict.
//!
//! Extracted from `loop.zig` (Module Split Pattern: types + the decision over
//! them). The runner half of Invariant 2 decides nothing about io, a transport
//! or a clock, so it is a function of three values and is unit-testable without
//! any of them — which is exactly why it should not sit inside a file whose
//! other job is to run two loops.
//!
//! `loop.zig` re-exports both names, so callers and tests keep spelling them
//! `loop.PollVerdict` and `loop.pollVerdict`.

/// An unmet (degraded) or absent assignment leases nothing, and a worker above
/// the assigned count idles — the soft-shrink half of a worker-count change.
pub const PollVerdict = enum { proceed, refuse_degraded, refuse_no_policy, idle_above_count };

pub const LOG_EVENT_LEASE_REFUSED_NO_POLICY = "lease_refused_no_policy";

/// Precedence is fail-closed: degraded wins over everything, then a missing
/// policy, then the soft-shrink idle. Only a worker inside a met assignment
/// proceeds.
pub fn decide(degraded: bool, assigned_workers: ?u32, worker_index: u32) PollVerdict {
    if (degraded) return .refuse_degraded;
    const count = assigned_workers orelse return .refuse_no_policy;
    if (worker_index >= count) return .idle_above_count;
    return .proceed;
}
