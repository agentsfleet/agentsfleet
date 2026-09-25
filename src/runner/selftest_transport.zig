//! selftest_transport.zig — prove a tool shell can start inside a lease.
//!
//! Shared by the self-test PARENT, which resolves the path on the host and puts
//! it on the probe's argv, and by the PROBE, which spawns it from behind the
//! lease's full hardening. One module so the two never disagree about which
//! binary is under test (RULE UFS), and split from both on the file-length
//! bound (RULE FLL).
//!
//! Model HTTP now uses in-process libcurl. Agent shell tools still need a
//! child process, so network reachability cannot establish tool availability.

const std = @import("std");
const builtin = @import("builtin");
const nullclaw = @import("nullclaw");

/// Where the shell tool lives on supported hosts. The engine spawns it by name
/// through `PATH`; the probe is handed an
/// absolute path instead, because `PATH` inside a lease is part of what the
/// sandbox decides and a check must not depend on the thing it is checking.
const SHELL_PATHS = [_][]const u8{ "/bin/sh", "/usr/bin/sh" };

/// A benign, offline command that produces no output.
const SHELL_COMMAND_ARG: [*:0]const u8 = "-c";
const SHELL_COMMAND: [*:0]const u8 = "exit 0";

/// The first shell binary present on this host, or null.
pub fn hostPath(io: std.Io) ?[]const u8 {
    for (SHELL_PATHS) |p| {
        std.Io.Dir.accessAbsolute(io, p, .{}) catch continue;
        return p;
    }
    return null;
}

/// Can this process execute a benign command through the shell at `path`?
///
/// Raw `fork` + `execve` rather than `std.process.spawn`: that helper does
/// pipe → fork → dup2 → setpgid → execvpe, and inside a lease its extra steps
/// fail with `AccessDenied` for reasons that have nothing to do with whether
/// the shell can run — measured, and it made this check report a broken
/// sandbox on a working one. A check that cannot tell its own plumbing from the
/// fault it looks for is worse than no check.
pub fn execs(path: []const u8) bool {
    if (builtin.os.tag != .linux) return false;
    var path_buf: [std.fs.max_path_bytes]u8 = undefined;
    if (path.len >= path_buf.len) return false;
    @memcpy(path_buf[0..path.len], path);
    path_buf[path.len] = 0;
    const path_z: [*:0]const u8 = @ptrCast(&path_buf);

    const forked: isize = @bitCast(std.os.linux.fork());
    if (forked < 0) return false;
    if (forked == 0) {
        // Child. Only async-signal-safe work between fork and exec — the probe
        // is multi-threaded and the child holds just this thread.
        //
        // stdout is CLOSED first: the parent's stdout is the pipe carrying the
        // verdict line, and a shell that printed one word into it would
        // corrupt the very report this check exists to write. stderr goes with
        // it so a usage message cannot reach the daemon's log either.
        _ = std.os.linux.close(1);
        _ = std.os.linux.close(2);
        const argv = [_:null]?[*:0]const u8{ path_z, SHELL_COMMAND_ARG, SHELL_COMMAND };
        const envp = [_:null]?[*:0]const u8{};
        _ = std.os.linux.execve(path_z, &argv, &envp);
        std.os.linux.exit(127);
    }

    var status: u32 = 0;
    _ = std.os.linux.wait4(@intCast(forked), &status, 0, null);
    if (!std.posix.W.IFEXITED(status)) return false;
    return std.posix.W.EXITSTATUS(status) == 0;
}

/// Can the ENGINE's spawn plumbing run `path` from in here? The complement of
/// `execs`, and the two must stay separate: `execs` proves the BINARY runs via
/// raw `fork`+`execve`, deliberately bypassing `std.process.spawn`'s extra
/// steps — so it kept passing while every lease died in exactly those steps.
/// This check drives one spawn through the same NullClaw compat layer the
/// engine starts subprocess tools with (all-pipe stdio, pre-fork argv
/// allocation through the process `Io`), so it fails where a lease fails: a
/// missing `compat.initProcess` leaves the fallback `Io` whose allocator
/// refuses everything (a synthetic pre-fork `OutOfMemory` the event surface
/// mislabels `oom_kill`), and a sandbox rule can refuse the spawn's own
/// plumbing while the naked `execve` still works.
///
/// The shell must execute the benign command successfully.
pub fn engineSpawns(path: []const u8) bool {
    if (builtin.os.tag != .linux) return false;
    // Only `Child.init`'s bookkeeping draws on this; the spawn itself
    // allocates through the process `Io` — the very path under test.
    var buf: [512]u8 = undefined;
    var fba = std.heap.FixedBufferAllocator.init(&buf);
    const argv = [_][]const u8{ path, std.mem.span(SHELL_COMMAND_ARG), std.mem.span(SHELL_COMMAND) };
    var child = nullclaw.compat.process.Child.init(&argv, fba.allocator());
    child.stdin_behavior = .Pipe;
    child.stdout_behavior = .Pipe;
    child.stderr_behavior = .Pipe;
    child.spawn() catch return false;
    // EOF on stdin so a shell that reads it cannot wait on us; stdout and
    // stderr stay open until `wait` reaps them — closing our read ends early
    // would SIGPIPE the child mid-command and grade a healthy spawn failed.
    if (child.stdin) |f| {
        f.close();
        child.stdin = null;
    }
    const term = child.wait() catch return false;
    return switch (term) {
        .exited => |code| code == 0,
        else => false,
    };
}

test "every candidate shell path is absolute" {
    // The probe is handed this path with `PATH` unresolvable inside the lease,
    // so a relative entry would silently never match.
    for (SHELL_PATHS) |p| try std.testing.expect(std.fs.path.isAbsolute(p));
}
