//! selftest_transport.zig — the engine's model transport as a self-test
//! subject: where it lives on the host, and whether a sandbox can execute it.
//!
//! Shared by the self-test PARENT, which resolves the path on the host and puts
//! it on the probe's argv, and by the PROBE, which spawns it from behind the
//! lease's full hardening. One module so the two never disagree about which
//! binary is under test (RULE UFS), and split from both on the file-length
//! bound (RULE FLL).
//!
//! This exists because the egress check cannot answer the question. That check
//! opens a TCP stream from inside the statically linked runner and spawns
//! nothing, so it measured the one path in a lease that needs no executable —
//! and M170 §3 removed the executable trees on the strength of it, which would
//! have killed every lease at `execve` before its first model call.
//! Reachability and executability are different facts.

const std = @import("std");

/// Where the engine's model transport lives, in the two locations a host puts
/// it. The engine spawns it by NAME through `PATH`; the probe is handed an
/// absolute path instead, because `PATH` inside a lease is part of what the
/// sandbox decides and a check must not depend on the thing it is checking.
pub const TRANSPORT_PATHS = [_][]const u8{ "/usr/bin/curl", "/bin/curl" };

/// The argument the transport is asked for: benign, offline, immediate.
const VERSION_ARG: [*:0]const u8 = "--version";

/// What the forked child exits with when `execve` returned instead of replacing
/// it. 127 is the shell's own "command could not be executed", so a reader
/// already knows what it means. A transport that legitimately exits 127 would
/// read as a failed exec — accepted, because no transport does, and the
/// alternative is a self-pipe this probe deliberately has no allocator for.
const EXEC_FAILED_EXIT: u8 = 127;

/// The first transport binary present on this host, or null. Null is reported
/// as its own fault rather than as an untested row: a host with no transport
/// runs no lease, and "nothing to measure" would be the green-probe/dead-runner
/// pairing this milestone exists to remove.
pub fn hostPath(io: std.Io) ?[]const u8 {
    for (TRANSPORT_PATHS) |p| {
        std.Io.Dir.accessAbsolute(io, p, .{}) catch continue;
        return p;
    }
    return null;
}

/// Can this process SPAWN `path`? Graded on the exec, not on the exit code:
/// what the transport prints is not the probe's business, a usage error still
/// proves the binary ran, and requiring a particular code would make the check
/// a hostage to the transport's command-line conventions.
///
/// Raw `fork` + `execve` rather than `std.process.spawn`: that helper does
/// pipe → fork → dup2 → setpgid → execvpe, and inside a lease its extra steps
/// fail with `AccessDenied` for reasons that have nothing to do with whether
/// the transport can run — measured, and it made this check report a broken
/// sandbox on a working one. A check that cannot tell its own plumbing from the
/// fault it looks for is worse than no check.
pub fn execs(path: []const u8) bool {
    if (@import("builtin").os.tag != .linux) return false;
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
        // verdict line, and a transport that printed one word into it would
        // corrupt the very report this check exists to write. stderr goes with
        // it so a usage message cannot reach the daemon's log either.
        _ = std.os.linux.close(1);
        _ = std.os.linux.close(2);
        const argv = [_:null]?[*:0]const u8{ path_z, VERSION_ARG };
        const envp = [_:null]?[*:0]const u8{};
        _ = std.os.linux.execve(path_z, &argv, &envp);
        std.os.linux.exit(EXEC_FAILED_EXIT);
    }

    var status: u32 = 0;
    _ = std.os.linux.wait4(@intCast(forked), &status, 0, null);
    if (!std.posix.W.IFEXITED(status)) return false;
    return std.posix.W.EXITSTATUS(status) != EXEC_FAILED_EXIT;
}

test "every candidate transport path is absolute" {
    // The probe is handed this path with `PATH` unresolvable inside the lease,
    // so a relative entry would silently never match.
    for (TRANSPORT_PATHS) |p| try std.testing.expect(std.fs.path.isAbsolute(p));
}
