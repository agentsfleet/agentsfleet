//! The self-test verdict a runner reports, and the bounds that keep a runner
//! token from becoming a persistence amplifier.
//!
//! Lives in the shared wire layer because both sides read this shape: the runner
//! produces it from `selftest.grade`, and the control plane stores it on
//! `fleet.runners` and renders it. A second copy would let the two disagree
//! about what a verdict is.
//!
//! Split from `protocol_policy.zig` on the 350-line bound (RULE FLL), and kept
//! out of `protocol.zig` for the same reason — that file sits seven lines under
//! the cap and only aliases these.

const std = @import("std");

/// One check's verdict. Deliberately the same `{name, ok, detail}` triple the
/// runner's `doctor` already speaks (`cmd/doctor.zig`), so an operator reads one
/// vocabulary across both surfaces rather than two shapes for one idea.
///
/// `detail` is prose even when `ok` — every passing doctor check carries a line,
/// and a whitespace-free cause reads to the dashboard as a leaked internal
/// identifier, which it then hides from the operator.
pub const SelftestCheck = struct {
    name: []const u8,
    ok: bool,
    detail: []const u8,
};

/// One probe run as it crosses the wire. `sandbox_tier` and `network_policy`
/// travel WITH the verdict rather than being read from the runner row at render
/// time: a result outlives the assignment that produced it, so the page compares
/// these against the row's live values and labels a mismatch stale instead of
/// presenting a verdict on a policy nothing has tested (Dimension 1.3).
pub const SelftestReport = struct {
    checks: []const SelftestCheck,
    all_ok: bool,
    sandbox_tier: []const u8,
    network_policy: []const u8,
};

/// Verdict bounds. A runner token must not be a persistence amplifier: the
/// probe emits a handful of checks drawn from a fixed vocabulary, so anything
/// past these caps is a malformed report — dropped as "no verdict this beat",
/// never a mebibyte of JSONB the runner page re-reads on every load.
///
/// The caps are sized to the vocabulary plus room for per-operator-bind checks
/// (Dimension 4.5 reports each assigned bind as its own named check, and
/// MAX_EXTRA_BINDS is 16), not to a guess.
pub const MAX_SELFTEST_CHECKS: usize = 32;
pub const MAX_CHECK_NAME_LEN: usize = 128;
pub const MAX_CHECK_DETAIL_LEN: usize = 256;
pub const MAX_SELFTEST_POLICY_LEN: usize = 64;

/// Why a verdict was refused, or `.none`. One verdict rather than two separate
/// predicates so a caller cannot check the bounds and forget the consistency —
/// the two failures have different causes but the same consequence, and
/// splitting them is how one gets skipped at a boundary. The variant is what
/// lets the refusing side log which it was.
pub const Rejection = enum {
    none,
    /// Past a cap, or carrying an empty name/detail/policy.
    unbounded,
    /// `all_ok` contradicts the checks it arrived with.
    all_ok_disagrees,
};

/// Total-report shaped: the whole verdict is refused on one bad entry, because
/// a partially stored self-test is a verdict nobody reasoned about.
pub fn selftestReportRejection(report: SelftestReport) Rejection {
    if (report.checks.len > MAX_SELFTEST_CHECKS) return .unbounded;
    if (report.sandbox_tier.len == 0 or report.sandbox_tier.len > MAX_SELFTEST_POLICY_LEN) return .unbounded;
    if (report.network_policy.len == 0 or report.network_policy.len > MAX_SELFTEST_POLICY_LEN) return .unbounded;

    var every_check_passed = true;
    for (report.checks) |c| {
        if (c.name.len == 0 or c.name.len > MAX_CHECK_NAME_LEN) return .unbounded;
        // `detail` may be long-ish prose but never empty: an empty cause line
        // reads to the dashboard as a leaked internal identifier and is hidden
        // from the operator, so a check would silently lose its explanation.
        if (c.detail.len == 0 or c.detail.len > MAX_CHECK_DETAIL_LEN) return .unbounded;
        if (!c.ok) every_check_passed = false;
    }

    // `all_ok` is reported by the daemon rather than derived on arrival, so a
    // runner could otherwise claim health its own checks contradict — which is
    // the exact shape of the incident: a host reading ACTIVE·ONLINE while every
    // lease dies inside its sandbox.
    if (report.all_ok != every_check_passed) return .all_ok_disagrees;
    return .none;
}

test "test_selftest_report_bounds_reject_a_malformed_verdict" {
    const ok_check = SelftestCheck{ .name = "a hostname resolves inside the sandbox", .ok = true, .detail = "no fault detected" };
    try std.testing.expectEqual(Rejection.none, selftestReportRejection(.{
        .checks = &.{ok_check},
        .all_ok = true,
        .sandbox_tier = "landlock_full",
        .network_policy = "allow_all",
    }));

    // An empty detail would be hidden from the operator as an internal
    // identifier, so the check would arrive explanation-less.
    try std.testing.expectEqual(Rejection.unbounded, selftestReportRejection(.{
        .checks = &.{.{ .name = "n", .ok = false, .detail = "" }},
        .all_ok = false,
        .sandbox_tier = "landlock_full",
        .network_policy = "allow_all",
    }));

    // An empty policy string cannot be compared against the row for staleness.
    try std.testing.expectEqual(Rejection.unbounded, selftestReportRejection(.{
        .checks = &.{ok_check},
        .all_ok = true,
        .sandbox_tier = "",
        .network_policy = "allow_all",
    }));

    var many: [MAX_SELFTEST_CHECKS + 1]SelftestCheck = undefined;
    for (&many) |*c| c.* = ok_check;
    try std.testing.expectEqual(Rejection.unbounded, selftestReportRejection(.{
        .checks = &many,
        .all_ok = true,
        .sandbox_tier = "landlock_full",
        .network_policy = "allow_all",
    }));
}

test "test_all_ok_must_agree_with_the_checks_it_arrived_with" {
    const pass = SelftestCheck{ .name = "n", .ok = true, .detail = "no fault detected" };
    const fail = SelftestCheck{ .name = "n", .ok = false, .detail = "the resolver did not answer inside the sandbox" };
    const R = Rejection;

    try std.testing.expectEqual(R.none, selftestReportRejection(.{ .checks = &.{pass}, .all_ok = true, .sandbox_tier = "t", .network_policy = "p" }));
    try std.testing.expectEqual(R.none, selftestReportRejection(.{ .checks = &.{ pass, fail }, .all_ok = false, .sandbox_tier = "t", .network_policy = "p" }));

    // A runner claiming health its own checks contradict — the shape that would
    // let a broken host keep reading ACTIVE·ONLINE, which is the whole incident.
    try std.testing.expectEqual(R.all_ok_disagrees, selftestReportRejection(.{ .checks = &.{ pass, fail }, .all_ok = true, .sandbox_tier = "t", .network_policy = "p" }));
    // And the mirror: claiming failure when every check passed.
    try std.testing.expectEqual(R.all_ok_disagrees, selftestReportRejection(.{ .checks = &.{pass}, .all_ok = false, .sandbox_tier = "t", .network_policy = "p" }));
}
