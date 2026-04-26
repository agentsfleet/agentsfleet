//! Cascading-fastpath file copy. Linux + macOS only.
//!
//! Vendored and adapted from https://github.com/oven-sh/bun
//! (`src/copy_file.zig`, MIT, commit
//! `dc578b12eca413e16b6bbea117ff24b73b48187f`). Stripped: `bun.sys.Maybe`
//! result wrapper → `std`-style error unions, `bun.sys.read/write` and
//! `bun.linux.ioctl_ficlone` (private bun helpers) → direct `std.posix` /
//! linux syscalls, syslog, the BUN_CONFIG_DISABLE_* env-var knobs (we keep
//! the kernel-support cache but skip the test-only kill switches),
//! Windows path. Path-based wrappers (`copyFile`) are added to satisfy the
//! API contract — bun exposes only the fd form internally.
//!
//! Cascade: macOS → fcopyfile; Linux → ioctl FICLONE → copy_file_range →
//! sendfile → read/write loop. Each fallback step is gated on a per-call
//! state struct so a single EXDEV pins the rest of the call to read/write.

const std = @import("std");
const builtin = @import("builtin");
const posix = std.posix;
const linux = std.os.linux;
const fd_t = std.posix.fd_t;

pub const CopyFileError = error{
    AccessDenied,
    FileTooBig,
    InputOutput,
    IsDir,
    NoSpaceLeft,
    OutOfMemory,
    PermissionDenied,
    Unexpected,
    Unseekable,
} || std.fs.File.OpenError || std.posix.WriteError || std.posix.ReadError;

pub const CopyFileRangeResult = struct {
    bytes: u64,
    used_fast_path: bool,
};

const State = struct {
    has_seen_exdev: bool = false,
    has_ioctl_ficlone_failed: bool = false,
    has_copy_file_range_failed: bool = false,
    has_sendfile_failed: bool = false,
    used_fast_path: bool = false,
};

var can_use_copy_file_range = std.atomic.Value(i8).init(0);
var can_use_ficlone = std.atomic.Value(i8).init(0);

/// Copy by path. Truncates+overwrites the destination. Preserves file mode of
/// the source on creation; metadata beyond mode is not transferred.
pub fn copyFile(src_path: []const u8, dst_path: []const u8) CopyFileError!CopyFileRangeResult {
    var src = std.fs.cwd().openFile(src_path, .{ .mode = .read_only }) catch |err| return err;
    defer src.close();

    const stat = src.stat() catch return error.Unexpected;
    var dst = std.fs.cwd().createFile(dst_path, .{
        .truncate = true,
        .mode = @intCast(stat.mode & 0o777),
    }) catch |err| return err;
    defer dst.close();

    return copyFileFd(src.handle, dst.handle);
}

/// Copy between two open file descriptors, starting at offset 0 of each.
/// Caller owns the fds; this function does not close them.
pub fn copyFileFd(src_fd: fd_t, dst_fd: fd_t) CopyFileError!CopyFileRangeResult {
    var state: State = .{};
    const total = try copyAll(src_fd, dst_fd, &state);
    return .{ .bytes = total, .used_fast_path = state.used_fast_path };
}

fn copyAll(in: fd_t, out: fd_t, state: *State) CopyFileError!u64 {
    if (comptime builtin.os.tag == .macos) {
        if (try tryFcopyfile(in, out, state)) |bytes| return bytes;
    }

    if (comptime builtin.os.tag == .linux) {
        if (try tryFicloneOnce(in, out, state)) |bytes| {
            state.used_fast_path = true;
            return bytes;
        }

        var total: u64 = 0;
        while (true) {
            const amt = try copyOnceLinux(in, out, std.math.maxInt(i32) - 1, state);
            if (amt == 0) return total;
            total += amt;
        }
    }

    // Generic POSIX (non-Linux, non-macOS) — read/write loop.
    return readWriteLoop(in, out);
}

fn tryFcopyfile(in: fd_t, out: fd_t, state: *State) CopyFileError!?u64 {
    if (comptime builtin.os.tag != .macos) return null;
    const rc = std.c.fcopyfile(in, out, null, .{ .DATA = true });
    switch (posix.errno(rc)) {
        .SUCCESS => {
            state.used_fast_path = true;
            const stat = try posix.fstat(in);
            return @intCast(stat.size);
        },
        .OPNOTSUPP => return null,
        .NOSPC => return error.NoSpaceLeft,
        .ACCES, .PERM => return error.PermissionDenied,
        .IO => return error.InputOutput,
        .ISDIR => return error.IsDir,
        else => return error.Unexpected,
    }
}

fn tryFicloneOnce(in: fd_t, out: fd_t, state: *State) CopyFileError!?u64 {
    if (comptime builtin.os.tag != .linux) return null;
    if (state.has_seen_exdev or state.has_ioctl_ficlone_failed) return null;
    if (!supportsFiclone()) return null;

    // FICLONE = _IOW(0x94, 9, int)
    const FICLONE: u32 = 0x40049409;
    const rc = linux.ioctl(out, FICLONE, @as(usize, @intCast(in)));
    switch (posix.errno(rc)) {
        .SUCCESS => {
            const stat = try posix.fstat(in);
            return @intCast(stat.size);
        },
        .XDEV => {
            state.has_seen_exdev = true;
            return null;
        },
        .ACCES, .BADF, .INVAL, .OPNOTSUPP, .NOSYS, .PERM => {
            can_use_ficlone.store(-1, .monotonic);
            state.has_ioctl_ficlone_failed = true;
            return null;
        },
        else => {
            state.has_ioctl_ficlone_failed = true;
            return null;
        },
    }
}

fn copyOnceLinux(in: fd_t, out: fd_t, len: usize, state: *State) CopyFileError!u64 {
    if (supportsCopyFileRange() and !state.has_seen_exdev and !state.has_copy_file_range_failed) {
        while (true) {
            const rc = linux.copy_file_range(in, null, out, null, len, 0);
            switch (posix.errno(rc)) {
                .SUCCESS => {
                    state.used_fast_path = true;
                    return @intCast(rc);
                },
                .INTR => continue,
                .INVAL => {
                    state.has_copy_file_range_failed = true;
                    break;
                },
                .XDEV => {
                    state.has_seen_exdev = true;
                    state.has_copy_file_range_failed = true;
                    break;
                },
                .OPNOTSUPP, .NOSYS => {
                    can_use_copy_file_range.store(-1, .monotonic);
                    state.has_copy_file_range_failed = true;
                    break;
                },
                .NOSPC => return error.NoSpaceLeft,
                .IO => return error.InputOutput,
                else => {
                    state.has_copy_file_range_failed = true;
                    break;
                },
            }
        }
    }

    while (!state.has_sendfile_failed) {
        const rc = linux.sendfile(@intCast(out), @intCast(in), null, len);
        switch (posix.errno(rc)) {
            .SUCCESS => {
                state.used_fast_path = true;
                return @intCast(rc);
            },
            .INTR => continue,
            .INVAL, .OPNOTSUPP, .NOSYS => {
                state.has_sendfile_failed = true;
                break;
            },
            .XDEV => {
                state.has_seen_exdev = true;
                state.has_sendfile_failed = true;
                break;
            },
            else => {
                state.has_sendfile_failed = true;
                break;
            },
        }
    }

    return readWriteOnce(in, out, len);
}

fn readWriteLoop(in: fd_t, out: fd_t) CopyFileError!u64 {
    var total: u64 = 0;
    while (true) {
        const amt = try readWriteOnce(in, out, std.math.maxInt(i32) - 1);
        if (amt == 0) return total;
        total += amt;
    }
}

fn readWriteOnce(in: fd_t, out: fd_t, len: usize) CopyFileError!u64 {
    var buf: [8 * 4096]u8 = undefined;
    const cap = @min(buf.len, len);
    const amt_read = posix.read(in, buf[0..cap]) catch |err| return err;
    if (amt_read == 0) return 0;

    var amt_written: usize = 0;
    while (amt_written < amt_read) {
        const wrote = posix.write(out, buf[amt_written..amt_read]) catch |err| return err;
        if (wrote == 0) return amt_written;
        amt_written += wrote;
    }
    return amt_read;
}

fn supportsCopyFileRange() bool {
    const v = can_use_copy_file_range.load(.monotonic);
    if (v == 0) {
        // Linux 4.5+. Probing requires a real syscall, but we let the first
        // call discover support via ENOSYS — flip the cache then.
        can_use_copy_file_range.store(1, .monotonic);
        return true;
    }
    return v == 1;
}

fn supportsFiclone() bool {
    const v = can_use_ficlone.load(.monotonic);
    if (v == 0) {
        can_use_ficlone.store(1, .monotonic);
        return true;
    }
    return v == 1;
}
