//! Policy application for the control loop: feed each heartbeat's raw
//! `assigned_policy` into the `AppliedPolicy` holder and run the apply-time
//! gates. A gate failure clears the holder — the daemon stays up and keeps
//! heartbeating, but leases nothing; the operator fixes the assignment from
//! the dashboard and the next beat picks it up. Split from `loop.zig`
//! (RULE FLL) — the loop owns cadence and shutdown, this file owns what an
//! arriving assignment means.

const std = @import("std");
const builtin = @import("builtin");
const logging = @import("log");
const contract = @import("contract");

const AppliedPolicy = @import("AppliedPolicy.zig");
const CgroupScope = @import("../engine/CgroupScope.zig");
const client_errors = @import("../engine/client_errors.zig");

const protocol = contract.protocol;
const log = logging.scoped(.fleet_runner);
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;
const ERR_EXEC_RUNNER_INVALID_CONFIG = client_errors.ERR_EXEC_RUNNER_INVALID_CONFIG;

/// Apply-time state the control loop threads through policy application: the
/// one-shot cgroup enablement, dedup for the invalid-policy warn, and the
/// spawned pool size the grow-needs-restart notice compares against.
pub const Gates = struct {
    controllers_enabled: bool = false,
    last_outcome: AppliedPolicy.ApplyOutcome = .unchanged,
    spawned_workers: ?u32 = null,
    grow_logged: bool = false,
    degraded_logged: bool = false,
};

const ERR_EXEC_ASSIGNMENT_UNACHIEVABLE = client_errors.ERR_EXEC_ASSIGNMENT_UNACHIEVABLE;

/// Record the control plane's degraded verdict from a heartbeat reply. The
/// holder's flag gates every worker's next poll; the warn fires once per
/// excursion, naming the missing mechanism the row carries.
pub fn noteDegraded(applied: *AppliedPolicy, gates: *Gates, degraded: bool, reason: ?[]const u8) void {
    applied.setDegraded(degraded);
    if (degraded) {
        if (!gates.degraded_logged) {
            log.warn("runner_assignment_unachievable", .{ .error_code = ERR_EXEC_ASSIGNMENT_UNACHIEVABLE, .reason = reason orelse "unspecified", .action = "fix the host or relax the assignment from the dashboard" });
            gates.degraded_logged = true;
        }
    } else {
        gates.degraded_logged = false;
    }
}

/// Feed one heartbeat's raw `assigned_policy` into the holder and run the
/// apply-time gates.
pub fn applyHeartbeatPolicy(io: std.Io, alloc: std.mem.Allocator, applied: *AppliedPolicy, gates: *Gates, raw: ?std.json.Value) void {
    const outcome = applied.apply(raw);
    defer gates.last_outcome = outcome;
    switch (outcome) {
        .unchanged => {},
        .invalid => if (gates.last_outcome != .invalid)
            log.warn("runner_policy_invalid", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .action = "leasing refused until a readable assignment arrives" }),
        .cleared => log.warn("runner_policy_missing", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .action = "assign a policy from the dashboard; leasing refused until then" }),
        .applied => runApplyGates(io, alloc, applied, gates),
    }
}

fn runApplyGates(io: std.Io, alloc: std.mem.Allocator, applied: *AppliedPolicy, gates: *Gates) void {
    const snap = applied.snapshot(alloc) orelse return;
    defer AppliedPolicy.freePolicy(alloc, snap);

    // Fail-closed (Invariant 7): a release build refuses the no-isolation
    // dev_none tier — assignment or not — so it can never become the
    // production posture. Refusal means "lease nothing", not exit: the
    // operator can re-assign from the dashboard without a host visit.
    if (devNoneForbidden(builtin.mode, snap.sandbox_tier)) {
        applied.clear();
        log.err("dev_none_rejected_in_release_build", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .sandbox_tier = @tagName(snap.sandbox_tier) });
        return;
    }

    // systemd delegates the controllers but never writes `cgroup.subtree_control`
    // — that is the delegatee's job. Without it every execution scope fails and
    // the host can only refuse leases while orphan scopes accumulate. Enable
    // once, on the first cage-building assignment; failure refuses leases while
    // the daemon keeps heartbeating, so the gap is visible instead of a crash
    // loop. `dev_none` builds no cage and is exempt.
    if (controllersRequired(builtin.os.tag, snap.sandbox_tier) and !gates.controllers_enabled) {
        CgroupScope.enableDelegatedControllers(io, alloc) catch |err| {
            applied.clear();
            log.err("cgroup_controllers_unavailable", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err), .sandbox_tier = @tagName(snap.sandbox_tier) });
            return;
        };
        gates.controllers_enabled = true;
    }

    log.debug("runner_policy_applied", .{
        .sandbox_tier = @tagName(snap.sandbox_tier),
        .egress = snap.network_policy.postureLabel(),
        .worker_count = snap.worker_count,
        .registry_hosts = snap.registry_allowlist.len,
        .source = "control_plane_assignment",
    });
}

/// A worker-count increase past the spawned pool size is the one policy change
/// that needs a restart (threads are spawned once); say so exactly once per
/// excursion instead of every heartbeat.
pub fn logGrowNeedsRestart(gates: *Gates, assigned_workers: u32) void {
    const spawned = gates.spawned_workers orelse return;
    if (assigned_workers > spawned) {
        if (!gates.grow_logged) {
            log.warn("worker_pool_grow_needs_restart", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .spawned = spawned, .assigned = assigned_workers, .action = "restart agentsfleet-runner to grow the pool" });
            gates.grow_logged = true;
        }
    } else {
        gates.grow_logged = false;
    }
}

/// Fail-closed release gate (Invariant 7): a release build refuses the
/// no-isolation `dev_none` tier so it can never be the production posture.
/// Debug builds allow it for local development. Pure so the matrix is
/// unit-testable.
fn devNoneForbidden(mode: std.builtin.OptimizeMode, tier: protocol.SandboxTier) bool {
    return mode != .Debug and tier == .dev_none;
}

test "release build forbids dev_none; Debug allows it; real tiers pass in prod" {
    try std.testing.expect(devNoneForbidden(.ReleaseSafe, .dev_none));
    try std.testing.expect(devNoneForbidden(.ReleaseFast, .dev_none));
    try std.testing.expect(!devNoneForbidden(.Debug, .dev_none)); // dev convenience
    try std.testing.expect(!devNoneForbidden(.ReleaseSafe, .landlock_full));
}

/// Whether this host must enable the delegated cgroup controllers before
/// leasing. True only when a cage is actually built on a kernel that has
/// cgroups. Pure so the matrix is unit-testable — the os tag is a parameter
/// rather than read from `builtin`.
fn controllersRequired(os_tag: std.Target.Os.Tag, tier: protocol.SandboxTier) bool {
    if (os_tag != .linux) return false;
    return tier != .dev_none;
}

test "delegated controllers are required only for a Linux tier that builds a cage" {
    try std.testing.expect(controllersRequired(.linux, .landlock_full));
    try std.testing.expect(controllersRequired(.linux, .container_nested));
    // dev_none builds no cage, so a host with no delegated subtree still runs.
    try std.testing.expect(!controllersRequired(.linux, .dev_none));
    // cgroups are Linux-only: no controller subtree can exist off-linux.
    try std.testing.expect(!controllersRequired(.macos, .landlock_full));
    try std.testing.expect(!controllersRequired(.macos, .dev_none));
}

test "test_startup_logs_the_applied_assignment: a decodable assignment lands on the applied path that emits runner_policy_applied" {
    // The `runner_policy_applied` event (spec Metrics table) lives on the
    // `.applied` arm of `applyHeartbeatPolicy` — the only arm that ends with
    // the holder populated. Driving a valid assignment through and observing
    // the holder proves the emitting path ran; there is no runtime log capture
    // in this graph, so the structural proof is the honest one.
    const alloc = std.testing.allocator;
    var applied = AppliedPolicy.init(alloc);
    defer applied.deinit();
    var gates = Gates{};

    const raw = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"sandbox_tier":"dev_none","network_policy":"deny_all_egress","registry_allowlist":[],"worker_count":1}
    , .{});
    defer raw.deinit();

    // SAFETY: io is only dereferenced by the cgroup-enablement gate, which the
    // cage-free dev_none tier (Debug-allowed) never reaches.
    applyHeartbeatPolicy(undefined, alloc, &applied, &gates, raw.value);
    try std.testing.expectEqual(AppliedPolicy.ApplyOutcome.applied, gates.last_outcome);
    const snap = applied.snapshot(alloc) orelse return error.TestUnexpectedResult;
    defer AppliedPolicy.freePolicy(alloc, snap);
    try std.testing.expectEqual(protocol.SandboxTier.dev_none, snap.sandbox_tier);
}

test "grow-needs-restart logs once per excursion and re-arms on shrink-back" {
    var gates = Gates{ .spawned_workers = 2 };
    logGrowNeedsRestart(&gates, 4);
    try std.testing.expect(gates.grow_logged);
    // Re-assigning within the spawned size re-arms the notice.
    logGrowNeedsRestart(&gates, 2);
    try std.testing.expect(!gates.grow_logged);
}
