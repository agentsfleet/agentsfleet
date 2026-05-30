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

/// A forked child and the parent ends of its stdio pipes.
pub const Child = struct {
    pid: std.posix.pid_t,
    stdin_w: std.posix.fd_t,
    stdout_r: std.posix.fd_t,
};

/// Fork and, in the child, dup the pipes onto stdin/stdout, lead a new process
/// group (so the parent can signal the whole group on the dev path), and exec
/// the sandbox wrapper. Returns the child pid + parent pipe ends.
pub fn forkExec(alloc: std.mem.Allocator, cfg: Config, workspace_path: []const u8) !Child {
    const in_pipe = try std.posix.pipe(); // [0]=read (child), [1]=write (parent)
    const out_pipe = try std.posix.pipe(); // [0]=read (parent), [1]=write (child)

    const argv = try sandbox.buildArgv(alloc, cfg, workspace_path);
    defer sandbox.freeArgv(alloc, argv);

    const pid = try std.posix.fork();
    if (pid == 0) {
        // ── child ──
        std.posix.dup2(in_pipe[0], std.posix.STDIN_FILENO) catch std.posix.exit(127);
        std.posix.dup2(out_pipe[1], std.posix.STDOUT_FILENO) catch std.posix.exit(127);
        std.posix.close(in_pipe[0]);
        std.posix.close(in_pipe[1]);
        std.posix.close(out_pipe[0]);
        std.posix.close(out_pipe[1]);
        std.posix.setpgid(0, 0) catch |err| log.warn("child_setpgid_failed", .{ .err = @errorName(err) });
        const err = std.process.execv(alloc, argv);
        log.err("child_execv_failed", .{ .err = @errorName(err) });
        std.posix.exit(127);
    }
    // ── parent ──
    std.posix.close(in_pipe[0]);
    std.posix.close(out_pipe[1]);
    return .{ .pid = pid, .stdin_w = in_pipe[1], .stdout_r = out_pipe[0] };
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

/// Write `bytes` to `fd` in full, looping over partial writes.
pub fn writeAll(fd: std.posix.fd_t, bytes: []const u8) !void {
    var off: usize = 0;
    while (off < bytes.len) off += try std.posix.write(fd, bytes[off..]);
}
