//! Unit tier for the probe-verdict parser.
//!
//! The spawn/reap half needs a real bubblewrap and lives in the integration
//! lane (`sandbox_integration_test.zig`). What is proven here is the part that
//! turns a child's line into an operator's verdict — because a parser that
//! reads a missing key as "passed" would report a probe that never ran as a
//! healthy sandbox, which is the exact failure this milestone exists to remove.

const std = @import("std");
const selftest_exec = @import("selftest_exec.zig");

const PASSING = "resolver=1 dns=1 egress=1 binds=1\n";

test "every check reads back off a full passing line" {
    const o = selftest_exec.outcomeFrom(PASSING, false);
    try std.testing.expect(o.resolver_readable);
    try std.testing.expect(o.dns_resolved);
    try std.testing.expect(o.egress_reachable);
    try std.testing.expect(o.extra_binds_present);
    try std.testing.expect(o.dns_testable);
    try std.testing.expect(!o.timed_out);
}

test "a check the probe never ran is untested, not failed" {
    // `x` is the probe saying "nothing asked me to test this". Reading it as a
    // failure would red-flag a runner that declared no registry — a correct
    // configuration, not a fault.
    const o = selftest_exec.outcomeFrom("resolver=1 dns=x egress=x binds=x", false);
    try std.testing.expect(o.resolver_readable);
    try std.testing.expect(!o.dns_testable);
    // Nothing was assigned, so nothing failed to land.
    try std.testing.expect(o.extra_binds_present);
}

test "an operator bind that did not land reads as absent" {
    const o = selftest_exec.outcomeFrom("resolver=1 dns=1 egress=1 binds=0", false);
    try std.testing.expect(!o.extra_binds_present);
}

test "a silent probe passes nothing" {
    // Empty stdout means the child died before printing, or printed nothing we
    // could read. Every check must read false: a probe that said nothing has
    // proven nothing, and defaulting to pass is how a dead sandbox reads green.
    const o = selftest_exec.outcomeFrom("", false);
    try std.testing.expect(!o.resolver_readable);
    try std.testing.expect(!o.dns_resolved);
    try std.testing.expect(!o.egress_reachable);
}

test "a truncated line does not read the next check's verdict" {
    // Key present, value cut off by the read cap. Must not fall through to
    // whatever byte follows.
    const o = selftest_exec.outcomeFrom("resolver=1 dns=", false);
    try std.testing.expect(o.resolver_readable);
    try std.testing.expect(!o.dns_resolved);
}

test "checks are matched by key, not by position" {
    // A future check inserted ahead of `egress` must not shift its verdict onto
    // a neighbour — the parser indexes by name for exactly this reason.
    const reordered = "binds=1 egress=0 dns=1 resolver=1";
    const o = selftest_exec.outcomeFrom(reordered, false);
    try std.testing.expect(o.resolver_readable);
    try std.testing.expect(o.dns_resolved);
    try std.testing.expect(!o.egress_reachable);
    try std.testing.expect(o.extra_binds_present);
}

test "an unrecognised verdict character is not a pass" {
    const o = selftest_exec.outcomeFrom("resolver=? dns=1 egress=1 binds=1", false);
    try std.testing.expect(!o.resolver_readable);
}

test "a reaped probe reports nothing it half-observed" {
    // The child may have printed a partial line before the kill landed.
    // Presenting that as fact would render a half-run as a verdict.
    const o = selftest_exec.outcomeFrom(PASSING, true);
    try std.testing.expect(o.timed_out);
    try std.testing.expect(!o.resolver_readable);
    try std.testing.expect(!o.dns_resolved);
    try std.testing.expect(!o.egress_reachable);
    try std.testing.expect(!o.extra_binds_present);
}
