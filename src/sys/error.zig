//! System call error type + Maybe(T) wrapper.
//!
//! Inspired by https://github.com/oven-sh/bun (`src/sys/Error.zig`, MIT) and
//! significantly trimmed for usezombie's surface — no Windows / libuv, no
//! WTF-string formatting, no JS bridge. This is the shape we want for new
//! sys/fs/network code that needs richer error context than a bare error
//! union: errno + the syscall tag + an optional path. Existing DB / HTTP
//! code keeps its `!T` returns; do not retrofit blindly. See
//! ZIG_RULES.md "Allocator Ownership in Structs" for the doc-comment
//! contract this type follows.

const std = @import("std");
const errno = @import("errno.zig");

pub const SyscallTag = enum {
    unknown,
    open,
    read,
    write,
    close,
    stat,
    fstat,
    mkdir,
    unlink,
    rmdir,
    rename,
    connect,
    accept,
    bind,
    listen,
    recv,
    send,
    chmod,
    chown,
    pipe,
    fork,
    exec,
    kill,
    waitpid,
    socket,
    setsockopt,
    getsockopt,
    fcntl,
    ioctl,
    mmap,
    munmap,
    sigaction,

    pub fn name(self: SyscallTag) []const u8 {
        return @tagName(self);
    }
};

/// Error that preserves errno + syscall tag + optional path. Plain value type;
/// copy freely. `path` is borrowed by default — to keep it across function
/// boundaries, call `clone(alloc)` and free with `deinit(alloc)`.
pub const Error = struct {
    errno: u16,
    syscall: SyscallTag = .unknown,
    /// Borrowed by default. Owned only after `clone(alloc)`.
    path: []const u8 = "",

    pub fn fromCode(code: anytype, syscall_tag: SyscallTag) Error {
        return .{
            .errno = @intCast(code),
            .syscall = syscall_tag,
        };
    }

    pub fn fromErrno(e: errno.E, syscall_tag: SyscallTag) Error {
        return .{
            .errno = @intCast(@intFromEnum(e)),
            .syscall = syscall_tag,
        };
    }

    pub fn withPath(self: Error, path: []const u8) Error {
        return .{
            .errno = self.errno,
            .syscall = self.syscall,
            .path = path,
        };
    }

    pub inline fn isRetry(self: Error) bool {
        // EAGAIN / EWOULDBLOCK on Linux = 11, on Darwin = 35.
        return self.errno == @intFromEnum(errno.E.AGAIN);
    }

    pub fn errnoName(self: Error) []const u8 {
        return errno.nameOf(self.errno);
    }

    /// Caller passes the same allocator to deinit that clone received.
    pub fn clone(self: Error, alloc: std.mem.Allocator) std.mem.Allocator.Error!Error {
        return .{
            .errno = self.errno,
            .syscall = self.syscall,
            .path = if (self.path.len > 0) try alloc.dupe(u8, self.path) else "",
        };
    }

    /// Frees the path slice if it was cloned. No-op on borrowed errors.
    /// Caller must pass the allocator that `clone` received.
    pub fn deinit(self: *Error, alloc: std.mem.Allocator) void {
        if (self.path.len > 0) alloc.free(self.path);
        self.path = "";
    }

    pub fn format(self: Error, writer: anytype) !void {
        try writer.print("syscall={s} errno={s}({d})", .{
            self.syscall.name(),
            self.errnoName(),
            self.errno,
        });
        if (self.path.len > 0) try writer.print(" path={s}", .{self.path});
    }
};

/// `Maybe(T)` — tagged result for fallible syscalls / I/O. Callers branch with
/// switch instead of catch, preserving the full error context (errno + syscall
/// + path). For pure logic errors, prefer Zig's `!T` error union; `Maybe` is
/// for system-level failures where errno matters.
pub fn Maybe(comptime T: type) type {
    return union(enum) {
        result: T,
        err: Error,

        pub fn ok(value: T) @This() {
            return .{ .result = value };
        }

        pub fn fail(e: Error) @This() {
            return .{ .err = e };
        }

        pub fn isOk(self: @This()) bool {
            return self == .result;
        }

        /// Convenience: lift to Zig error union for callers that want `try`.
        pub fn unwrap(self: @This()) error{SyscallFailed}!T {
            return switch (self) {
                .result => |v| v,
                .err => error.SyscallFailed,
            };
        }
    };
}

test "Error: fromErrno + name lookup" {
    const e = Error.fromErrno(.NOENT, .open);
    try std.testing.expectEqualStrings("ENOENT", e.errnoName());
    try std.testing.expectEqual(SyscallTag.open, e.syscall);
}

test "Error: withPath returns a borrowed copy" {
    const e = Error.fromErrno(.NOENT, .open).withPath("/tmp/missing");
    try std.testing.expectEqualStrings("/tmp/missing", e.path);
}

test "Error: clone owns the path; deinit frees it" {
    const alloc = std.testing.allocator;
    var e = try Error.fromErrno(.NOENT, .open).withPath("/tmp/missing").clone(alloc);
    defer e.deinit(alloc);
    try std.testing.expectEqualStrings("/tmp/missing", e.path);
}

test "Error: isRetry detects EAGAIN" {
    const e = Error.fromErrno(.AGAIN, .read);
    try std.testing.expect(e.isRetry());
    const other = Error.fromErrno(.NOENT, .read);
    try std.testing.expect(!other.isRetry());
}

test "Maybe(T): ok and fail variants" {
    const m = Maybe(u32).ok(42);
    try std.testing.expect(m.isOk());
    switch (m) {
        .result => |v| try std.testing.expectEqual(@as(u32, 42), v),
        .err => return error.WrongVariant,
    }

    const m2 = Maybe(u32).fail(Error.fromErrno(.NOENT, .open));
    try std.testing.expect(!m2.isOk());
}

test "Maybe(T): unwrap lifts to error union" {
    const m = Maybe(u32).ok(7);
    const v = try m.unwrap();
    try std.testing.expectEqual(@as(u32, 7), v);

    const m2 = Maybe(u32).fail(Error.fromErrno(.NOENT, .open));
    try std.testing.expectError(error.SyscallFailed, m2.unwrap());
}
