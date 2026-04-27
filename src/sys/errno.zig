//! POSIX errno → name lookup for Linux and Darwin.
//!
//! Vendored and adapted from https://github.com/oven-sh/bun
//! (`src/errno/linux_errno.zig`, `src/errno/darwin_errno.zig`, MIT). Stripped:
//! Windows + libuv tables (usezombie deploys on Linux); the full UV_E mapping
//! (we don't speak libuv); Bun runtime imports.
//!
//! Use this for cleaner system-call error logging than `@errorName(err)`,
//! which gives Zig error-set names rather than POSIX errno names. Wrap a
//! syscall-rc with `getErrno(rc)`, then either log `nameOf(errno)` directly
//! or pass the numeric value into a structured logger.

const std = @import("std");
const builtin = @import("builtin");

/// `std.posix.E` — kept as a type alias so callers don't need a second import.
pub const E = std.posix.E;

/// Returns the POSIX errno name (e.g. "EAGAIN") for a numeric errno value.
/// Returns "EUNKNOWN" for codes outside the platform's table — never null,
/// so log lines stay clean. Names are stable across platforms when the
/// underlying concept is shared (EAGAIN, ENOENT, etc.); names that differ
/// (e.g. EPROCLIM on Darwin only) are reported on the platform that has them.
pub fn nameOf(errno: anytype) []const u8 {
    const code: i32 = @intCast(errno);
    return platformNameOf(code);
}

/// Convenience: extract errno from a syscall return code (negative-return
/// convention on Linux raw syscalls; -1 + thread-local errno on glibc).
/// Returns `.SUCCESS` if `rc` does not indicate an error.
fn getErrno(rc: anytype) E {
    const T = @TypeOf(rc);
    return switch (T) {
        usize => blk: {
            const signed: isize = @bitCast(rc);
            const int = if (signed > -4096 and signed < 0) -signed else 0;
            break :blk @enumFromInt(@as(u16, @intCast(int)));
        },
        i32, c_int, u32, isize, i64 => blk: {
            if (rc == -1) {
                break :blk @enumFromInt(@as(u16, @intCast(std.c._errno().*)));
            }
            break :blk .SUCCESS;
        },
        else => @compileError("errno.getErrno: unsupported return type " ++ @typeName(T)),
    };
}

const platformNameOf = if (builtin.os.tag == .linux)
    nameOfLinux
else if (builtin.os.tag.isDarwin())
    nameOfDarwin
else
    @compileError("errno: unsupported OS — only Linux and Darwin are vendored");

fn nameOfLinux(code: i32) []const u8 {
    return switch (code) {
        0 => "SUCCESS",
        1 => "EPERM",
        2 => "ENOENT",
        3 => "ESRCH",
        4 => "EINTR",
        5 => "EIO",
        6 => "ENXIO",
        7 => "E2BIG",
        8 => "ENOEXEC",
        9 => "EBADF",
        10 => "ECHILD",
        11 => "EAGAIN",
        12 => "ENOMEM",
        13 => "EACCES",
        14 => "EFAULT",
        15 => "ENOTBLK",
        16 => "EBUSY",
        17 => "EEXIST",
        18 => "EXDEV",
        19 => "ENODEV",
        20 => "ENOTDIR",
        21 => "EISDIR",
        22 => "EINVAL",
        23 => "ENFILE",
        24 => "EMFILE",
        25 => "ENOTTY",
        26 => "ETXTBSY",
        27 => "EFBIG",
        28 => "ENOSPC",
        29 => "ESPIPE",
        30 => "EROFS",
        31 => "EMLINK",
        32 => "EPIPE",
        33 => "EDOM",
        34 => "ERANGE",
        35 => "EDEADLK",
        36 => "ENAMETOOLONG",
        37 => "ENOLCK",
        38 => "ENOSYS",
        39 => "ENOTEMPTY",
        40 => "ELOOP",
        42 => "ENOMSG",
        43 => "EIDRM",
        60 => "ENOSTR",
        61 => "ENODATA",
        62 => "ETIME",
        63 => "ENOSR",
        67 => "ENOLINK",
        71 => "EPROTO",
        72 => "EMULTIHOP",
        74 => "EBADMSG",
        75 => "EOVERFLOW",
        84 => "EILSEQ",
        87 => "EUSERS",
        88 => "ENOTSOCK",
        89 => "EDESTADDRREQ",
        90 => "EMSGSIZE",
        91 => "EPROTOTYPE",
        92 => "ENOPROTOOPT",
        93 => "EPROTONOSUPPORT",
        94 => "ESOCKTNOSUPPORT",
        95 => "ENOTSUP",
        96 => "EPFNOSUPPORT",
        97 => "EAFNOSUPPORT",
        98 => "EADDRINUSE",
        99 => "EADDRNOTAVAIL",
        100 => "ENETDOWN",
        101 => "ENETUNREACH",
        102 => "ENETRESET",
        103 => "ECONNABORTED",
        104 => "ECONNRESET",
        105 => "ENOBUFS",
        106 => "EISCONN",
        107 => "ENOTCONN",
        108 => "ESHUTDOWN",
        109 => "ETOOMANYREFS",
        110 => "ETIMEDOUT",
        111 => "ECONNREFUSED",
        112 => "EHOSTDOWN",
        113 => "EHOSTUNREACH",
        114 => "EALREADY",
        115 => "EINPROGRESS",
        116 => "ESTALE",
        122 => "EDQUOT",
        125 => "ECANCELED",
        130 => "EOWNERDEAD",
        131 => "ENOTRECOVERABLE",
        else => "EUNKNOWN",
    };
}

fn nameOfDarwin(code: i32) []const u8 {
    return switch (code) {
        0 => "SUCCESS",
        1 => "EPERM",
        2 => "ENOENT",
        3 => "ESRCH",
        4 => "EINTR",
        5 => "EIO",
        6 => "ENXIO",
        7 => "E2BIG",
        8 => "ENOEXEC",
        9 => "EBADF",
        10 => "ECHILD",
        11 => "EDEADLK",
        12 => "ENOMEM",
        13 => "EACCES",
        14 => "EFAULT",
        15 => "ENOTBLK",
        16 => "EBUSY",
        17 => "EEXIST",
        18 => "EXDEV",
        19 => "ENODEV",
        20 => "ENOTDIR",
        21 => "EISDIR",
        22 => "EINVAL",
        23 => "ENFILE",
        24 => "EMFILE",
        25 => "ENOTTY",
        26 => "ETXTBSY",
        27 => "EFBIG",
        28 => "ENOSPC",
        29 => "ESPIPE",
        30 => "EROFS",
        31 => "EMLINK",
        32 => "EPIPE",
        33 => "EDOM",
        34 => "ERANGE",
        35 => "EAGAIN",
        36 => "EINPROGRESS",
        37 => "EALREADY",
        38 => "ENOTSOCK",
        39 => "EDESTADDRREQ",
        40 => "EMSGSIZE",
        41 => "EPROTOTYPE",
        42 => "ENOPROTOOPT",
        43 => "EPROTONOSUPPORT",
        44 => "ESOCKTNOSUPPORT",
        45 => "ENOTSUP",
        46 => "EPFNOSUPPORT",
        47 => "EAFNOSUPPORT",
        48 => "EADDRINUSE",
        49 => "EADDRNOTAVAIL",
        50 => "ENETDOWN",
        51 => "ENETUNREACH",
        52 => "ENETRESET",
        53 => "ECONNABORTED",
        54 => "ECONNRESET",
        55 => "ENOBUFS",
        56 => "EISCONN",
        57 => "ENOTCONN",
        58 => "ESHUTDOWN",
        59 => "ETOOMANYREFS",
        60 => "ETIMEDOUT",
        61 => "ECONNREFUSED",
        62 => "ELOOP",
        63 => "ENAMETOOLONG",
        64 => "EHOSTDOWN",
        65 => "EHOSTUNREACH",
        66 => "ENOTEMPTY",
        67 => "EPROCLIM",
        68 => "EUSERS",
        69 => "EDQUOT",
        70 => "ESTALE",
        77 => "ENOLCK",
        78 => "ENOSYS",
        84 => "EOVERFLOW",
        89 => "ECANCELED",
        90 => "EIDRM",
        91 => "ENOMSG",
        92 => "EILSEQ",
        94 => "EBADMSG",
        96 => "ENODATA",
        97 => "ENOLINK",
        100 => "EPROTO",
        102 => "EOPNOTSUPP",
        104 => "ENOTRECOVERABLE",
        105 => "EOWNERDEAD",
        else => "EUNKNOWN",
    };
}

test "nameOf returns common POSIX names" {
    try std.testing.expectEqualStrings("SUCCESS", nameOf(0));
    try std.testing.expectEqualStrings("EPERM", nameOf(1));
    try std.testing.expectEqualStrings("ENOENT", nameOf(2));
    try std.testing.expectEqualStrings("EINVAL", nameOf(22));
}

test "nameOf returns EUNKNOWN for out-of-range codes" {
    try std.testing.expectEqualStrings("EUNKNOWN", nameOf(9999));
    try std.testing.expectEqualStrings("EUNKNOWN", nameOf(-1));
}

test "platform-specific names resolve" {
    if (builtin.os.tag == .linux) {
        try std.testing.expectEqualStrings("EAGAIN", nameOf(11));
    } else if (builtin.os.tag.isDarwin()) {
        try std.testing.expectEqualStrings("EAGAIN", nameOf(35));
    }
}
