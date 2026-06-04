//! child_process.zig — raw fork/exec/kill plumbing for a leased child.
//!
//! The mechanical process layer under `child_supervisor`: it forks, wires the
//! child's stdio onto pipes, execs the sandbox wrapper, and kills the whole
//! child tree. It owns no lifecycle policy (deadlines, renewal, reaping order,
//! result classification) — that stays in the supervisor. The split keeps each
//! file focused and within the line budget.
//!
//! The lease's inline secrets ride the child's stdin pipe — never argv or env
//! (both readable in /proc) — per RULE VLT.

const std = @import("std");
const logging = @import("log");

const cgroup = @import("engine/cgroup.zig");
const sandbox = @import("sandbox_args.zig");
const Config = @import("daemon/config.zig");

const log = logging.scoped(.runner_supervisor);

/// Spawn the sandboxed child. Zig 0.16 removed raw fork/pipe/dup2/close/waitpid,
/// so std.process.spawn does pipe → fork → dup2 → setpgid(0,0) → execvpe, and the
/// returned `process.Child` is the only portable reap/close path (child_supervisor
/// drives wait()/close). `pgid = 0` keeps the child its own process-group leader
/// (killChild's `kill(-pgid)` fallback). Containment stays cgroup-centric — the
/// wrapper's kill() is never called: it touches only the single pid, letting a
/// hostile child's descendants outlive the lease; scope.kill reaps the whole tree
/// atomically.
pub fn forkExec(io: std.Io, alloc: std.mem.Allocator, cfg: Config, workspace_path: []const u8) !std.process.Child {
    const argv = try sandbox.buildArgv(io, alloc, cfg, workspace_path);
    defer sandbox.freeArgv(alloc, argv);
    return std.process.spawn(io, .{
        .argv = argv,
        .stdin = .pipe,
        .stdout = .pipe,
        .stderr = .inherit,
        .pgid = 0,
    }) catch |err| {
        log.err("child_spawn_failed", .{ .err = @errorName(err) });
        return err;
    };
}

/// Kill the whole child tree. On Linux the cgroup is the atomic kill domain;
/// otherwise signal the child's process group (it leads its own via setpgid).
pub fn killChild(pid: std.posix.pid_t, scope: *?cgroup.CgroupScope) void {
    if (scope.*) |*s| {
        s.kill() catch |err| {
            log.warn("cgroup_kill_failed_fallback_signal", .{ .err = @errorName(err) });
            std.posix.kill(-pid, std.posix.SIG.KILL) catch |kerr| log.warn("child_group_kill_failed", .{ .err = @errorName(kerr) });
        };
        return;
    }
    std.posix.kill(-pid, std.posix.SIG.KILL) catch |err| log.warn("child_group_kill_failed", .{ .err = @errorName(err) });
}

