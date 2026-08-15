//! Pre-fork fail-closed proofs for `child_supervisor.zig` — every arm of
//! `run`/`supervise` that must refuse a lease BEFORE any child exists. The fork
//! and everything downstream of it belong to the runner integration suite; the
//! unit module's exec target is the test binary itself, so these tests prove
//! exactly the refusals that guard that boundary:
//!   - a lease that cannot even be serialized fails as a startup posture;
//!   - a required sandbox that cannot be established refuses the lease
//!     (Invariant 7 — never run prompt-injectable execution unsandboxed);
//!   - the unbuilt strict-egress posture fails CLOSED rather than running as if
//!     the boundary were enforced;
//!   - a child that cannot be enrolled in the cgroup kill domain is refused
//!     (it would otherwise run unmetered with a no-op kill switch).

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const contract = @import("contract");
const Config = @import("daemon/config.zig");
const child_supervisor = @import("child_supervisor.zig");
const result_mod = @import("child_supervisor_result.zig");
const cgroup = @import("engine/CgroupScope.zig");

const protocol = contract.protocol;
const ALLOC = testing.allocator;

const RUNNER_TOKEN = protocol.RUNNER_TOKEN_PREFIX ++ "a" ** 64;
const WORKSPACE = "/tmp/agentsfleet-m164-supervisor-prefork";
/// No cgroup lives here on any platform: enrolment must fail, deterministically.
const BOGUS_CGROUP_PATH = "/nonexistent/agentsfleet-m164-bogus-scope";
/// Outside every real pid range (Linux pid_max and macOS alike), so the
/// fail-closed kill sweep hits ESRCH — swallowed, never a live process.
const BOGUS_PID: std.posix.pid_t = 999_999_9;

fn discardActivity(_: *anyopaque, _: contract.activity.ActivityFrame) void {}
fn discardMemory(_: *anyopaque, _: []const u8) void {}

fn leasePayload() protocol.LeasePayload {
    return .{
        .lease_id = "lease-prefork-guard",
        .fencing_token = 3,
        .lease_expires_at = 1 << 40,
        .secret_delivery = .@"inline",
        .policy = .{},
        .event = .{
            .event_id = "event-prefork-guard",
            .fleet_id = "fleet-prefork-guard",
            .workspace_id = "workspace-prefork-guard",
            .actor = "actor-prefork-guard",
            .event_type = .webhook,
            .request_json = "{}",
            .created_at = 1,
        },
        .bundle = null,
    };
}

fn testConfig(alloc: std.mem.Allocator, tier: protocol.SandboxTier, net: protocol.NetworkPolicy) !Config {
    return .{
        .control_plane_url = try alloc.dupe(u8, "http://127.0.0.1:9"),
        .runner_token = try alloc.dupe(u8, RUNNER_TOKEN),
        .sandbox_tier = tier,
        .storage_home = try alloc.dupe(u8, WORKSPACE),
        .network_policy = net,
        .worker_count = 1,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        .alloc = alloc,
    };
}

/// Drive `run` through its pre-fork guards with no renewal or mint hooks (both
/// null — the documented test posture: every ask is rejected fail-closed).
fn runGuarded(alloc: std.mem.Allocator, cfg: Config) contract.execution_result.ExecutionResult {
    var dummy: u8 = 0;
    const sink = child_supervisor.ActivitySink{ .ctx = &dummy, .forward = discardActivity };
    const mem_sink = child_supervisor.MemorySink{ .ctx = &dummy, .forward = discardMemory };
    var env_map: std.process.Environ.Map = .init(ALLOC);
    defer env_map.deinit();
    return child_supervisor.run(common.globalIo(), alloc, cfg, &env_map, WORKSPACE, leasePayload(), &.{}, sink, mem_sink, null, null);
}

test "a lease that cannot be serialized refuses as a startup posture, not a crash" {
    const cfg = try testConfig(ALLOC, .dev_none, .deny_all_egress);
    defer cfg.deinit();
    // The very first allocation — the lease JSON for the child's stdin — fails.
    // The supervisor must refuse with a reportable result; under the same
    // pressure the detail dupe degrades to the bare class (documented fallback).
    var failing = std.testing.FailingAllocator.init(ALLOC, .{ .fail_index = 0 });
    const result = runGuarded(failing.allocator(), cfg);
    try testing.expect(!result.succeeded());
    try testing.expectEqual(contract.execution_result.FailureClass.startup_posture, result.outcome.failed.class);
    try testing.expectEqual(@as(usize, 0), result.failureDetail().len);
}

test "the unbuilt strict-egress posture refuses the lease fail-closed" {
    // dev_none establishes no sandbox scope, so the egress gate is the first
    // guard reached: `allow_list_egress` enforcement is unbuilt and must refuse
    // rather than run as if the kernel boundary existed.
    const cfg = try testConfig(ALLOC, .dev_none, .allow_list_egress);
    defer cfg.deinit();
    const result = runGuarded(ALLOC, cfg);
    defer if (result.failureDetail().len > 0) ALLOC.free(result.failureDetail());
    try testing.expect(!result.succeeded());
    try testing.expectEqual(contract.execution_result.FailureClass.startup_posture, result.outcome.failed.class);
    try testing.expectEqualStrings(result_mod.DETAIL_EGRESS_UNIMPLEMENTED, result.failureDetail());
}

test "a required sandbox that cannot be established refuses the lease (Invariant 7)" {
    const io = common.globalIo();
    // Probe first: on a host where a sandbox genuinely establishes (delegated
    // cgroup v2), the refusal arm is unreachable — release the scope and skip
    // rather than assert a falsehood. macOS and unprivileged Linux both error.
    if (child_supervisor.establishSandbox(io, ALLOC, true)) |maybe_scope| {
        var scope = maybe_scope;
        if (scope) |*s| _ = s.destroy(.{});
        return error.SkipZigTest;
    } else |_| {}

    const cfg = try testConfig(ALLOC, .container_nested, .deny_all_egress);
    defer cfg.deinit();
    const result = runGuarded(ALLOC, cfg);
    defer if (result.failureDetail().len > 0) ALLOC.free(result.failureDetail());
    try testing.expect(!result.succeeded());
    try testing.expectEqual(contract.execution_result.FailureClass.startup_posture, result.outcome.failed.class);
    try testing.expectEqualStrings(result_mod.DETAIL_SANDBOX_UNAVAILABLE, result.failureDetail());
}

test "no sandbox requirement establishes no scope (dev-only explicit no-isolation)" {
    const scope = try child_supervisor.establishSandbox(common.globalIo(), ALLOC, false);
    try testing.expectEqual(@as(?cgroup, null), scope);
}

test "enrolment with no scope is a no-op; a dead scope refuses the lease fail-closed" {
    // dev_none: nothing to enroll, nothing to fail.
    var no_scope: ?cgroup = null;
    try child_supervisor.enrollOrFail(&no_scope, BOGUS_PID, "lease-prefork-guard");

    // A scope whose cgroup vanished (or a platform without one): the child
    // would run unmetered in the daemon's cgroup with a no-op kill switch —
    // the enrolment must refuse, and its kill sweep of the bogus pid/pgroup
    // lands on ESRCH, swallowed.
    const bogus_path = try ALLOC.dupe(u8, BOGUS_CGROUP_PATH);
    defer ALLOC.free(bogus_path);
    var dead_scope: ?cgroup = cgroup{ .path = bogus_path, .alloc = ALLOC, .io = common.globalIo() };
    try testing.expectError(error.CgroupEnrollFailed, child_supervisor.enrollOrFail(&dead_scope, BOGUS_PID, "lease-prefork-guard"));
}
