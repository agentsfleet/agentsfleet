//! child_supervisor.zig — fork-per-lease sandboxed execution (parent side).
//!
//! Each lease runs in a fresh child process. The runner forks; the child
//! re-execs `agentsfleet-runner __execute` under bubblewrap (Linux) or directly
//! (dev/macOS). The parent binds the child to a cgroup scope, feeds the lease
//! in over the child's stdin, reads the `ExecutionResult` back over its stdout,
//! and enforces a wall-clock deadline (`lease_expires_at`).
//!
//! The cgroup is the control plane: `scope.kill()` on a timeout/revocation
//! (atomic, whole-tree, no PID chase), `wasOomKilled()` to classify a memory
//! death. The child is ALWAYS reaped (orphan-safe) and the scope ALWAYS
//! destroyed (idempotent), on every exit path — the lifecycle, not the
//! syscalls, is the point.
//!
//! Isolation rests on the process boundary: a misbehaving agent (OOM, crash,
//! escape attempt) is contained in the child; the parent daemon survives,
//! reaps it, and reports the failure rather than dying. The lease's inline
//! secrets ride the child's stdin pipe — never argv or env (both readable in
//! /proc) — per RULE VLT.

const std = @import("std");
const builtin = @import("builtin");
const logging = @import("log");

const types = @import("engine/types.zig");
const cgroup = @import("engine/CgroupScope.zig");
const client_errors = @import("engine/client_errors.zig");
const Config = @import("daemon/config.zig");
const contract = @import("contract");
const pipe_proto = @import("pipe_proto.zig");
const child_process = @import("child_process.zig");
const result_mod = @import("child_supervisor_result.zig");
const read_mod = @import("child_supervisor_read.zig");

const log = logging.scoped(.runner_supervisor);

const ExecutionResult = types.ExecutionResult;
const LeasePayload = contract.protocol.LeasePayload;

// The child→parent read loop (sinks, renewal hook, readResult + frame dispatch)
// lives in child_supervisor_read.zig; the terminal result mapping (ReadOutcome +
// classify + parseResult) in child_supervisor_result.zig — both split out to keep
// this file under the line budget. Re-exported so callers and tests keep using
// `child_supervisor.{ActivitySink,MemorySink,RenewDecision,RenewHook,readResult,
// ReadOutcome,classify,UsageSnapshot}` against one import.
pub const ActivitySink = read_mod.ActivitySink;
pub const MemorySink = read_mod.MemorySink;
pub const RenewDecision = read_mod.RenewDecision;
pub const RenewHook = read_mod.RenewHook;
pub const readResult = read_mod.readResult;
pub const ReadOutcome = result_mod.ReadOutcome;
pub const classify = result_mod.classify;
const failed = result_mod.failed;

/// Hook-facing alias so RenewHook implementors observe the cumulative usage
/// snapshot without importing pipe_proto directly.
pub const UsageSnapshot = pipe_proto.UsageSnapshot;

/// The one tier that runs WITHOUT isolation (dev only) — derived from the
/// `SandboxTier` enum (RULE UFS: single source, never a re-spelled literal). A
/// production startup guard must reject this tier so it can't become the prod
/// default.
const SANDBOX_TIER_DEV_NONE = @tagName(contract.protocol.SandboxTier.dev_none);

/// Run one leased event in a forked, sandboxed child and return its result.
/// Never errors: every supervision failure maps to a failed `ExecutionResult`
/// so the caller always has an outcome to report.
pub fn run(
    io: std.Io,
    alloc: std.mem.Allocator,
    cfg: Config,
    env_map: *const std.process.Environ.Map,
    workspace_path: []const u8,
    payload: LeasePayload,
    /// Prior memory the parent hydrated; piped to the child (no child fetch).
    hydrated_memory: []const contract.protocol.MemoryDelta,
    sink: ActivitySink,
    /// Receives the child's `.memory` capture frames for the parent to POST.
    mem_sink: MemorySink,
    renew_hook: ?RenewHook,
) ExecutionResult {
    const input = contract.protocol.RunnerChildInput{ .lease = payload, .hydrated_memory = hydrated_memory };
    const lease_json = std.json.Stringify.valueAlloc(alloc, input, .{}) catch
        return failed(.startup_posture);
    defer alloc.free(lease_json);

    return supervise(io, alloc, cfg, env_map, workspace_path, payload, lease_json, sink, mem_sink, renew_hook) catch |err| {
        log.err("supervise_failed", .{ .error_code = client_errors.ERR_EXEC_CRASH, .lease_id = payload.lease_id, .err = @errorName(err) });
        return failed(.runner_crash);
    };
}

/// Fork → bind to cgroup → feed lease → read result under a deadline → reap.
/// `defer` reaps the child and destroys the scope on every path (errors too).
fn supervise(
    io: std.Io,
    alloc: std.mem.Allocator,
    cfg: Config,
    env_map: *const std.process.Environ.Map,
    workspace_path: []const u8,
    payload: LeasePayload,
    lease_json: []const u8,
    sink: ActivitySink,
    mem_sink: MemorySink,
    renew_hook: ?RenewHook,
) !ExecutionResult {
    // Fail-closed (Invariant 7): if the tier requires isolation and it cannot
    // be established, refuse the lease — never run prompt-injectable tool
    // execution unsandboxed.
    const requires_sandbox = !std.mem.eql(u8, cfg.sandbox_tier, SANDBOX_TIER_DEV_NONE);
    var scope: ?cgroup = establishSandbox(io, alloc, requires_sandbox) catch {
        log.err("sandbox_unavailable_fail_closed", .{
            .error_code = client_errors.ERR_RUN_SANDBOX_ESTABLISH_FAILED,
            .lease_id = payload.lease_id,
            .tier = cfg.sandbox_tier,
        });
        return failed(.startup_posture);
    };
    defer if (scope) |*s| {
        _ = s.destroy(.{});
    };

    // Egress strategy (network/Policy.Mode):
    //   deny_all_egress     → unshared netns, no veth (handled in sandbox_args).
    //   allow_all (default) → interim allow-all (`--share-net` in sandbox_args).
    //   allow_list_egress   → kernel-enforced EgressScope boundary (option D).
    // The strict posture's per-lease establishment (resolve allowlist → named
    // netns + veth + nft + rendered resolver files) is unbuilt (2.0.1). Fail
    // CLOSED when it is selected — refuse the lease rather than run as if the
    // boundary were enforced. The other postures pass egress=null (no
    // EgressScope; --share-net or nothing per mode).
    if (cfg.network_policy.enforcesEgress()) {
        log.err("egress_strict_unimplemented_fail_closed", .{
            .error_code = client_errors.ERR_RUN_SANDBOX_ESTABLISH_FAILED,
            .lease_id = payload.lease_id,
        });
        return failed(.startup_posture);
    }
    var child = try child_process.forkExec(io, alloc, cfg, env_map, workspace_path, null);
    var reaped = false;
    // Zig 0.16 removed raw posix.waitpid/close; the process.Child wrapper is the
    // only portable reap/close path — `wait` blocks, reaps, and closes any still-
    // open stdio. The kill switch stays cgroup-atomic (killChild); the wrapper's
    // kill() would touch only the single pid and let a hostile child's tree escape.
    // OWNERSHIP CONTRACT: this supervisor is the SOLE reaper — `reaped` gates the one
    // wait(); never call child.wait() elsewhere (double-reap). Wrapper non-authoritative.
    defer if (!reaped) {
        // Best-effort terminal reap (the normal path sets `reaped`); the child may
        // already be gone. Can't propagate from a defer, so log and move on.
        if (child.wait(io)) |_| {} else |err| {
            log.warn("reap_wait_failed", .{ .error_code = client_errors.ERR_EXEC_CRASH, .err = @errorName(err) });
        }
    };

    // FAIL-CLOSED: if the child cannot be enrolled in the cgroup kill
    // domain, refuse the lease — it would otherwise run unmetered in the daemon's
    // cgroup AND leave the exec-cgroup empty (a later scope.kill would no-op).
    enrollOrFail(&scope, child.id.?, payload.lease_id) catch return failed(.startup_posture);

    // Feed the lease (incl. inline secrets) in, then EOF the child's stdin. Null
    // the File after closing so the terminal `wait` does not double-close it.
    child.stdin.?.writeStreamingAll(io, lease_json) catch |err|
        log.warn("lease_write_failed", .{ .error_code = client_errors.ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
    child.stdin.?.close(io);
    child.stdin = null;

    const outcome = try readResult(alloc, child.stdout.?.handle, payload.lease_expires_at, sink, mem_sink, renew_hook);
    defer alloc.free(outcome.bytes);
    // child.stdout is closed by the terminal `wait` below — no manual close.

    if (outcome.terminated) {
        log.warn("lease_terminated_by_renewal", .{ .error_code = client_errors.ERR_EXEC_RENEWAL_TERMINATED, .lease_id = payload.lease_id });
        child_process.killChild(child.id.?, &scope);
    } else if (outcome.timed_out) {
        log.warn("lease_timed_out", .{ .error_code = client_errors.ERR_EXEC_LEASE_EXPIRED, .lease_id = payload.lease_id });
        child_process.killChild(child.id.?, &scope);
    }

    // Claim the reap before wait() so the `!reaped` defer never double-waits
    // (wait() asserts id != null and nulls it on success).
    reaped = true;
    const term = child.wait(io) catch |err| {
        log.err("child_wait_failed", .{ .error_code = client_errors.ERR_EXEC_CRASH, .lease_id = payload.lease_id, .err = @errorName(err) });
        return failed(.runner_crash);
    };
    return classify(alloc, outcome, term, &scope);
}

/// Fail-closed sandbox setup (Invariant 7). `dev_none` (requires=false) returns
/// null — explicit no-isolation, dev only. Every other tier MUST have its
/// isolation domain established or this errors, so the caller refuses the lease
/// rather than running the agent unsandboxed. (Landlock + netns are applied by
/// the child before it runs the agent; this sets up the parent-side cgroup
/// kill/limit domain.)
pub fn establishSandbox(io: std.Io, alloc: std.mem.Allocator, requires: bool) !?cgroup {
    if (!requires) return null;
    // macOS Seatbelt is not yet wired — a required sandbox we cannot apply must
    // fail closed rather than pretend. (Dev on macOS uses the dev_none tier.)
    if (builtin.os.tag != .linux) return error.SandboxUnavailable;
    // Zig 0.16 removed the global std.crypto.random on linux; fill the cgroup
    // execution id from the kernel CSPRNG (getrandom). Linux-only path (guarded
    // above); fail-closed on a short/failed read — matches sandbox setup posture.
    // SAFETY: written in full by getrandom on the next line (length-checked) before any read.
    var exec_id: types.ExecutionId = undefined;
    if (std.os.linux.getrandom(&exec_id, exec_id.len, 0) != exec_id.len)
        return error.SandboxUnavailable;
    return cgroup.create(io, alloc, exec_id, .{}) catch error.SandboxUnavailable;
}

/// Bind the just-forked child to the cgroup kill/limit domain — FAIL-CLOSED.
/// On failure the child would run unmetered in the daemon's cgroup
/// with an empty exec-cgroup, so refuse the lease: kill it (via `killChild`,
/// which also signals the pgroup — `scope.kill` alone no-ops on the empty
/// cgroup) and surface the error. Null scope (`dev_none`) has nothing to enroll.
pub fn enrollOrFail(scope: *?cgroup, child_id: std.posix.pid_t, lease_id: []const u8) error{CgroupEnrollFailed}!void {
    const s = if (scope.*) |*sc| sc else return;
    s.addProcess(child_id) catch |err| {
        log.err("cgroup_enroll_failed_fail_closed", .{
            .error_code = client_errors.ERR_RUN_SANDBOX_ESTABLISH_FAILED,
            .lease_id = lease_id,
            .err = @errorName(err),
        });
        child_process.killChild(child_id, scope);
        return error.CgroupEnrollFailed;
    };
}

// Tests live in child_supervisor_test.zig (sibling) to keep this file within
// the line budget; registered via the runner test aggregator in main.zig.
