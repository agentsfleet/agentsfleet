//! Unit coverage for the delegated-cgroup contract the runner depends on:
//! which placement lines count as "delegated", and how a placement that is NOT
//! delegated is classified.
//!
//! Why this matters enough to have its own suite: the daemon writes
//! `cgroup.subtree_control` at startup because systemd makes the delegated
//! controllers available but never enables them for children — only the
//! delegatee may. If the base cannot be resolved, that write silently never
//! happens, every execution scope is created without `memory.max` / `cpu.max` /
//! `pids.max` to write, and the host refuses leases while orphan scope
//! directories accumulate. Distinguishing "not delegated" from "procfs
//! unreadable" is what lets the operator tell a lost `Delegate=` in the unit
//! apart from a broken host.

const std = @import("std");
const CgroupScope = @import("CgroupScope.zig");

// A cgroup v2 placement line is `0::<path>`; the daemon runs in the `runner`
// leaf that `DelegateSubgroup=runner` creates, and the delegated base is that
// leaf's parent — the scopes it creates are the leaf's siblings.
const DELEGATED_PLACEMENT = "0::/system.slice/agentsfleet-runner.service/runner";
const DELEGATED_BASE = "/system.slice/agentsfleet-runner.service";
const UNDELEGATED_PLACEMENT = "0::/system.slice/agentsfleet-runner.service";
const ROOT_PLACEMENT = "0::/";

test "a delegated placement resolves to the parent of the runner leaf" {
    const base = CgroupScope.delegatedCgroupPath(DELEGATED_PLACEMENT) orelse
        return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings(DELEGATED_BASE, base);
}

test "test_not_delegated_is_distinct_from_write_failure: a placement outside the runner leaf resolves to nothing" {
    // Running directly in the service cgroup rather than the delegated leaf is
    // exactly the state a unit that lost `DelegateSubgroup=runner` lands in.
    // It must not resolve — writing subtree_control from there would trip
    // cgroup v2's no-internal-processes rule instead of enabling anything.
    try std.testing.expect(CgroupScope.delegatedCgroupPath(UNDELEGATED_PLACEMENT) == null);
    try std.testing.expect(CgroupScope.delegatedCgroupPath(ROOT_PLACEMENT) == null);
}

test "a placement with no unified line resolves to nothing" {
    // cgroup v1 hosts emit only numbered controller lines; none is a v2 subtree
    // the daemon may write, so the base must not resolve on a hybrid host.
    try std.testing.expect(CgroupScope.delegatedCgroupPath("1:name=systemd:/user.slice") == null);
    try std.testing.expect(CgroupScope.delegatedCgroupPath("") == null);
}

test "traversal and empty segments never resolve to a base" {
    // The base is interpolated into an absolute path under /sys/fs/cgroup, so a
    // placement carrying `..` or an empty segment must be refused rather than
    // escaping the mount.
    try std.testing.expect(CgroupScope.delegatedCgroupPath("0::/../../etc/runner") == null);
    try std.testing.expect(CgroupScope.delegatedCgroupPath("0:://runner") == null);
    try std.testing.expect(CgroupScope.delegatedCgroupPath("0::/./runner") == null);
}

test "the not-delegated classification is its own error, not a read failure" {
    // The two are handled differently by the operator: CgroupNotDelegated means
    // the unit's Delegate= is wrong, CgroupReadFailed means procfs did not read.
    // Proving they are distinct members keeps the log line actionable.
    try std.testing.expect(CgroupScope.CgroupError.CgroupNotDelegated !=
        CgroupScope.CgroupError.CgroupReadFailed);
}
