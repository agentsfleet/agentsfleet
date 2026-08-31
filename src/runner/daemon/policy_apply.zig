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
const client_errors = @import("../engine/client_errors.zig");

const protocol = contract.protocol;
const log = logging.scoped(.fleet_runner);
const ERR_EXEC_RUNNER_INVALID_CONFIG = client_errors.ERR_EXEC_RUNNER_INVALID_CONFIG;

/// Apply-time state the control loop threads through policy application: dedup
/// for the invalid-policy warn, and the spawned pool size the grow-needs-restart
/// notice compares against.
pub const Gates = struct {
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
/// apply-time gates. The release-build dev_none refusal runs BEFORE the holder
/// publishes: a worker polling between publish and a post-publish clear could
/// otherwise snapshot the forbidden tier and start one cage-less lease.
pub fn applyHeartbeatPolicy(alloc: std.mem.Allocator, applied: *AppliedPolicy, gates: *Gates, raw: ?std.json.Value) void {
    if (forbiddenTier(alloc, raw)) {
        applied.clear();
        log.err("dev_none_rejected_in_release_build", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .sandbox_tier = @tagName(protocol.SandboxTier.dev_none) });
        gates.last_outcome = .cleared;
        return;
    }
    const outcome = applied.apply(raw);
    defer gates.last_outcome = outcome;
    switch (outcome) {
        .unchanged => {},
        .invalid => if (gates.last_outcome != .invalid)
            log.warn("runner_policy_invalid", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .action = "leasing refused until a readable assignment arrives" }),
        .cleared => log.warn("runner_policy_missing", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .action = "assign a policy from the dashboard; leasing refused until then" }),
        .applied => runApplyGates(alloc, applied),
    }
}

/// Pre-publish peek at the assigned tier: true when a release build must
/// refuse it. A raw value that fails to decode returns false — the full
/// decode inside `apply` classifies it `.invalid` (also fail-closed).
fn forbiddenTier(alloc: std.mem.Allocator, raw: ?std.json.Value) bool {
    const value = raw orelse return false;
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const decoded = std.json.parseFromValueLeaky(protocol.AssignedPolicy, arena.allocator(), value, .{ .ignore_unknown_fields = true }) catch return false;
    return devNoneForbidden(builtin.mode, decoded.sandbox_tier);
}

fn runApplyGates(alloc: std.mem.Allocator, applied: *AppliedPolicy) void {
    const snap = applied.snapshot(alloc) orelse return;
    defer AppliedPolicy.freePolicy(alloc, snap);

    // (The release-build dev_none refusal — Invariant 7 — already ran in
    // `applyHeartbeatPolicy`, BEFORE the holder published this policy.)
    //
    // Delegated-controller enablement is NOT here: it depends only on the host,
    // so the daemon does it once at startup (`main.zig`). A host that cannot
    // deliver its assigned isolation is caught by the control plane's
    // reconciliation of assigned-against-reported capability, which degrades the
    // runner row — not by a gate on this path.

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

test "test_startup_logs_the_applied_assignment: a decodable assignment lands on the applied path that emits runner_policy_applied" {
    // The `runner_policy_applied` event (spec Metrics table) lives on the
    // `.applied` arm of `applyHeartbeatPolicy` — the only arm that ends with
    // the holder populated. Driving a valid assignment through and observing
    // the holder proves the emitting path ran; there is no runtime log capture
    // in this graph, so the structural proof is the honest one.
    //
    // `dev_none` keeps the expected outcome build-mode dependent, because
    // Invariant 7 refuses dev_none in a release build. The retired leak lane pinned
    // ReleaseSafe on Linux (valgrind needs an optimised binary), so each mode
    // asserts its own arm instead of the Debug one twice; both arms run the
    // function, which is what the leak gate needs.
    const alloc = std.testing.allocator;
    var applied = AppliedPolicy.init(alloc);
    defer applied.deinit();
    var gates = Gates{};

    const raw = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"sandbox_tier":"dev_none","network_policy":"deny_all_egress","registry_allowlist":[],"worker_count":1}
    , .{});
    defer raw.deinit();

    applyHeartbeatPolicy(alloc, &applied, &gates, raw.value);

    if (devNoneForbidden(builtin.mode, .dev_none)) {
        // Release arm: refused BEFORE the holder published, so no worker can
        // snapshot the forbidden tier between publish and clear.
        try std.testing.expectEqual(AppliedPolicy.ApplyOutcome.cleared, gates.last_outcome);
        try std.testing.expect(applied.snapshot(alloc) == null);
        return;
    }
    try std.testing.expectEqual(AppliedPolicy.ApplyOutcome.applied, gates.last_outcome);
    const snap = applied.snapshot(alloc) orelse return error.TestUnexpectedResult;
    defer AppliedPolicy.freePolicy(alloc, snap);
    try std.testing.expectEqual(protocol.SandboxTier.dev_none, snap.sandbox_tier);
}

test "test_policy_apply_has_no_controller_gate: cgroup enablement is not apply-time state" {
    // Enabling the delegated controllers depends only on the host, so the daemon
    // does it once at startup (`main.zig`) rather than on the first cage-building
    // assignment. A one-shot flag reappearing here would mean the write is back
    // to racing the first heartbeat — which is exactly what made a freshly
    // deployed runner fail its post-deploy readiness check.
    try std.testing.expect(!@hasField(Gates, "controllers_enabled"));
}

test "grow-needs-restart logs once per excursion and re-arms on shrink-back" {
    var gates = Gates{ .spawned_workers = 2 };
    logGrowNeedsRestart(&gates, 4);
    try std.testing.expect(gates.grow_logged);
    // Re-assigning within the spawned size re-arms the notice.
    logGrowNeedsRestart(&gates, 2);
    try std.testing.expect(!gates.grow_logged);
}
