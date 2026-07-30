//! Reconcile a runner's ASSIGNED policy against its reported ACHIEVABLE
//! capability into the row's degraded verdict. Pure — no clock, no rows — so
//! the whole matrix is unit-tested here and the heartbeat handler only
//! orchestrates reads and writes around it.
//!
//! The reason strings are the operator-facing vocabulary: they land on the
//! runner row verbatim and each names the specific missing mechanism, which
//! maps to a step in the runner bootstrap playbook.

const std = @import("std");
const protocol = @import("contract").protocol;

pub const REASON_NO_ASSIGNED_POLICY = "no assigned policy";
pub const REASON_NO_CAPABILITY_REPORT = "no capability report";
pub const REASON_LANDLOCK_UNAVAILABLE = "landlock unavailable";
pub const REASON_SECCOMP_UNAVAILABLE = "seccomp unavailable";
pub const REASON_CGROUP_CONTROLLERS_MISSING = "cgroup controllers not delegated";
pub const REASON_BUBBLEWRAP_MISSING = "bubblewrap missing";
pub const REASON_EGRESS_ENFORCEMENT_UNAVAILABLE = "egress enforcement unavailable";

/// Controllers a cage-building tier needs in the delegated subtree — mirrors
/// the runner-side enablement set (`CgroupScope`: cpu, memory, pids).
const REQUIRED_CONTROLLERS = [_][]const u8{ "cpu", "memory", "pids" };

/// The verdict the heartbeat writes and replies with. `reason` is one of the
/// REASON_* constants (static — never row-owned memory).
pub const Verdict = struct {
    degraded: bool,
    reason: ?[]const u8,

    const ok = Verdict{ .degraded = false, .reason = null };
    fn missing(reason: []const u8) Verdict {
        return .{ .degraded = true, .reason = reason };
    }
};

/// What each assigned tier needs the host to prove. `dev_none` builds no cage,
/// so it demands no mechanism. A nested-container host runs where Landlock is
/// typically masked, so the cage there is bubblewrap + seccomp + controllers
/// without a Landlock demand.
const TierNeeds = struct {
    landlock: bool = false,
    seccomp: bool = false,
    controllers: bool = false,
    bubblewrap: bool = false,

    fn any(self: TierNeeds) bool {
        return self.landlock or self.seccomp or self.controllers or self.bubblewrap;
    }
};

fn tierNeeds(tier: protocol.SandboxTier) TierNeeds {
    return switch (tier) {
        .landlock_full => .{ .landlock = true, .seccomp = true, .controllers = true, .bubblewrap = true },
        .container_nested => .{ .seccomp = true, .controllers = true, .bubblewrap = true },
        .dev_none => .{},
    };
}

/// The reconciliation: assigned against achievable. Missing policy or missing
/// report degrade on their own; otherwise the first unmet mechanism (in a
/// fixed order) names the reason.
pub fn reconcile(assigned: ?protocol.AssignedPolicy, achievable: ?protocol.CapabilityReport) Verdict {
    const policy = assigned orelse return Verdict.missing(REASON_NO_ASSIGNED_POLICY);
    const needs = tierNeeds(policy.sandbox_tier);
    const wants_egress = policy.network_policy.enforcesEgress();
    if (!needs.any() and !wants_egress) return Verdict.ok;

    const cap = achievable orelse return Verdict.missing(REASON_NO_CAPABILITY_REPORT);
    if (needs.landlock and !cap.landlock) return Verdict.missing(REASON_LANDLOCK_UNAVAILABLE);
    if (needs.seccomp and !cap.seccomp) return Verdict.missing(REASON_SECCOMP_UNAVAILABLE);
    if (needs.controllers and !hasRequiredControllers(cap.cgroup_controllers)) return Verdict.missing(REASON_CGROUP_CONTROLLERS_MISSING);
    if (needs.bubblewrap and !cap.bubblewrap) return Verdict.missing(REASON_BUBBLEWRAP_MISSING);
    if (wants_egress and !cap.egress_enforcement) return Verdict.missing(REASON_EGRESS_ENFORCEMENT_UNAVAILABLE);
    return Verdict.ok;
}

fn hasRequiredControllers(present: []const []const u8) bool {
    for (REQUIRED_CONTROLLERS) |req| {
        var found = false;
        for (present) |p| {
            if (std.mem.eql(u8, p, req)) {
                found = true;
                break;
            }
        }
        if (!found) return false;
    }
    return true;
}

const FULL_CAP = protocol.CapabilityReport{
    .landlock = true,
    .seccomp = true,
    .cgroup_controllers = &REQUIRED_CONTROLLERS,
    .bubblewrap = true,
    .egress_enforcement = false,
};

fn assignedWith(tier: protocol.SandboxTier, network: protocol.NetworkPolicy) protocol.AssignedPolicy {
    return .{ .sandbox_tier = tier, .network_policy = network, .registry_allowlist = &.{}, .worker_count = 1 };
}

test "test_degraded_runner_names_the_missing_mechanism: a satisfied assignment is not degraded; each miss names itself" {
    try std.testing.expect(!reconcile(assignedWith(.landlock_full, .allow_all), FULL_CAP).degraded);

    var no_landlock = FULL_CAP;
    no_landlock.landlock = false;
    try std.testing.expectEqualStrings(REASON_LANDLOCK_UNAVAILABLE, reconcile(assignedWith(.landlock_full, .allow_all), no_landlock).reason.?);

    var no_seccomp = FULL_CAP;
    no_seccomp.seccomp = false;
    try std.testing.expectEqualStrings(REASON_SECCOMP_UNAVAILABLE, reconcile(assignedWith(.landlock_full, .allow_all), no_seccomp).reason.?);

    var no_controllers = FULL_CAP;
    no_controllers.cgroup_controllers = REQUIRED_CONTROLLERS[0..2]; // pids missing
    try std.testing.expectEqualStrings(REASON_CGROUP_CONTROLLERS_MISSING, reconcile(assignedWith(.landlock_full, .allow_all), no_controllers).reason.?);

    var no_bwrap = FULL_CAP;
    no_bwrap.bubblewrap = false;
    try std.testing.expectEqualStrings(REASON_BUBBLEWRAP_MISSING, reconcile(assignedWith(.landlock_full, .allow_all), no_bwrap).reason.?);
}

test "an assigned egress allowlist degrades until enforcement exists in a build" {
    const v = reconcile(assignedWith(.landlock_full, .allow_list_egress), FULL_CAP);
    try std.testing.expect(v.degraded);
    try std.testing.expectEqualStrings(REASON_EGRESS_ENFORCEMENT_UNAVAILABLE, v.reason.?);
    // dev_none + allowlist still demands egress enforcement — the tier does
    // not excuse the network policy.
    try std.testing.expect(reconcile(assignedWith(.dev_none, .allow_list_egress), FULL_CAP).degraded);
}

test "no policy and no report each degrade with their own reason" {
    try std.testing.expectEqualStrings(REASON_NO_ASSIGNED_POLICY, reconcile(null, FULL_CAP).reason.?);
    try std.testing.expectEqualStrings(REASON_NO_CAPABILITY_REPORT, reconcile(assignedWith(.landlock_full, .allow_all), null).reason.?);
}

test "a tier that builds no cage needs no report at all" {
    try std.testing.expect(!reconcile(assignedWith(.dev_none, .allow_all), null).degraded);
    try std.testing.expect(!reconcile(assignedWith(.dev_none, .deny_all_egress), null).degraded);
}

test "container_nested demands the container cage but never Landlock" {
    var no_landlock = FULL_CAP;
    no_landlock.landlock = false;
    try std.testing.expect(!reconcile(assignedWith(.container_nested, .allow_all), no_landlock).degraded);
    var no_bwrap = no_landlock;
    no_bwrap.bubblewrap = false;
    try std.testing.expect(reconcile(assignedWith(.container_nested, .allow_all), no_bwrap).degraded);
}

test "a report satisfying the assignment clears the verdict (recovery is just reconciliation)" {
    var no_landlock = FULL_CAP;
    no_landlock.landlock = false;
    try std.testing.expect(reconcile(assignedWith(.landlock_full, .allow_all), no_landlock).degraded);
    try std.testing.expect(!reconcile(assignedWith(.landlock_full, .allow_all), FULL_CAP).degraded);
}
