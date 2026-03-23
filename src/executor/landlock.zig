//! Landlock filesystem policy enforcement for the host backend (§4.2).
//!
//! Applies a Landlock ruleset to restrict filesystem access:
//! - Workspace directory: read + write
//! - System paths (/usr, /bin, /lib, /etc): read-only + execute
//! - Everything else: denied by default
//!
//! Uses raw syscalls (Landlock has no libc wrapper).
//! Linux-only; no-ops on other platforms.

const std = @import("std");
const builtin = @import("builtin");
const executor_metrics = @import("executor_metrics.zig");

const log = std.log.scoped(.executor_landlock);

// Landlock syscall numbers (same on x86_64 and aarch64).
const SYS_landlock_create_ruleset: usize = 444;
const SYS_landlock_add_rule: usize = 445;
const SYS_landlock_restrict_self: usize = 446;

// Raw Linux syscall interface. On non-Linux, stubs return error values;
// all call sites guard with `if (builtin.os.tag != .linux)` before use.
const raw = if (builtin.os.tag == .linux) struct {
    const sys = std.os.linux;
    fn syscall3(n: usize, a1: usize, a2: usize, a3: usize) usize {
        return sys.syscall3(@enumFromInt(n), a1, a2, a3);
    }
    fn syscall4(n: usize, a1: usize, a2: usize, a3: usize, a4: usize) usize {
        return sys.syscall4(@enumFromInt(n), a1, a2, a3, a4);
    }
} else struct {
    fn syscall3(_: usize, _: usize, _: usize, _: usize) usize {
        return std.math.maxInt(usize);
    }
    fn syscall4(_: usize, _: usize, _: usize, _: usize, _: usize) usize {
        return std.math.maxInt(usize);
    }
};

// Landlock access flags for filesystem (ABI v1).
const LANDLOCK_ACCESS_FS_EXECUTE: u64 = 1 << 0;
const LANDLOCK_ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
const LANDLOCK_ACCESS_FS_READ_FILE: u64 = 1 << 2;
const LANDLOCK_ACCESS_FS_READ_DIR: u64 = 1 << 3;
const LANDLOCK_ACCESS_FS_REMOVE_DIR: u64 = 1 << 4;
const LANDLOCK_ACCESS_FS_REMOVE_FILE: u64 = 1 << 5;
const LANDLOCK_ACCESS_FS_MAKE_CHAR: u64 = 1 << 6;
const LANDLOCK_ACCESS_FS_MAKE_DIR: u64 = 1 << 7;
const LANDLOCK_ACCESS_FS_MAKE_REG: u64 = 1 << 8;
const LANDLOCK_ACCESS_FS_MAKE_SOCK: u64 = 1 << 9;
const LANDLOCK_ACCESS_FS_MAKE_FIFO: u64 = 1 << 10;
const LANDLOCK_ACCESS_FS_MAKE_BLOCK: u64 = 1 << 11;
const LANDLOCK_ACCESS_FS_MAKE_SYM: u64 = 1 << 12;

const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;

// Full set of handled access rights for ruleset creation.
const ALL_FS_ACCESS: u64 = LANDLOCK_ACCESS_FS_EXECUTE |
    LANDLOCK_ACCESS_FS_WRITE_FILE |
    LANDLOCK_ACCESS_FS_READ_FILE |
    LANDLOCK_ACCESS_FS_READ_DIR |
    LANDLOCK_ACCESS_FS_REMOVE_DIR |
    LANDLOCK_ACCESS_FS_REMOVE_FILE |
    LANDLOCK_ACCESS_FS_MAKE_CHAR |
    LANDLOCK_ACCESS_FS_MAKE_DIR |
    LANDLOCK_ACCESS_FS_MAKE_REG |
    LANDLOCK_ACCESS_FS_MAKE_SOCK |
    LANDLOCK_ACCESS_FS_MAKE_FIFO |
    LANDLOCK_ACCESS_FS_MAKE_BLOCK |
    LANDLOCK_ACCESS_FS_MAKE_SYM;

// Workspace gets full RW access.
const WORKSPACE_ACCESS: u64 = LANDLOCK_ACCESS_FS_READ_FILE |
    LANDLOCK_ACCESS_FS_WRITE_FILE |
    LANDLOCK_ACCESS_FS_READ_DIR |
    LANDLOCK_ACCESS_FS_MAKE_REG |
    LANDLOCK_ACCESS_FS_MAKE_DIR |
    LANDLOCK_ACCESS_FS_REMOVE_FILE |
    LANDLOCK_ACCESS_FS_REMOVE_DIR |
    LANDLOCK_ACCESS_FS_MAKE_SYM;

// System paths get read-only + execute.
const SYSTEM_READONLY_ACCESS: u64 = LANDLOCK_ACCESS_FS_READ_FILE |
    LANDLOCK_ACCESS_FS_READ_DIR |
    LANDLOCK_ACCESS_FS_EXECUTE;

pub const LandlockError = error{
    UnsupportedPlatform,
    RulesetCreationFailed,
    RuleAddFailed,
    RestrictSelfFailed,
    PathOpenFailed,
};

const LandlockRulesetAttr = extern struct {
    handled_access_fs: u64,
};

const LandlockPathBeneathAttr = extern struct {
    allowed_access: u64,
    parent_fd: i32,
};

/// System paths that get read-only access in the sandbox.
const SYSTEM_READONLY_PATHS = [_][]const u8{
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib64",
    "/etc",
    "/dev",
    "/proc",
    "/tmp",
};

/// Apply Landlock filesystem policy.
/// After this call, the current process can only access:
/// - workspace_path with full RW
/// - system paths with read-only + execute
/// - everything else is denied
pub fn applyPolicy(workspace_path: []const u8) LandlockError!void {
    if (builtin.os.tag != .linux) return LandlockError.UnsupportedPlatform;

    // Create ruleset.
    var attr = LandlockRulesetAttr{ .handled_access_fs = ALL_FS_ACCESS };
    const ruleset_fd_raw = raw.syscall3(
        SYS_landlock_create_ruleset,
        @intFromPtr(&attr),
        @sizeOf(LandlockRulesetAttr),
        0,
    );
    const ruleset_fd = if (ruleset_fd_raw > std.math.maxInt(i32))
        return LandlockError.RulesetCreationFailed
    else
        @as(i32, @intCast(@as(i64, @bitCast(ruleset_fd_raw))));
    if (ruleset_fd < 0) return LandlockError.RulesetCreationFailed;
    defer std.posix.close(@intCast(ruleset_fd));

    // Add workspace rule (RW).
    try addPathRule(ruleset_fd, workspace_path, WORKSPACE_ACCESS);

    // Add system readonly paths.
    for (SYSTEM_READONLY_PATHS) |path| {
        addPathRule(ruleset_fd, path, SYSTEM_READONLY_ACCESS) catch {
            // Path may not exist on all systems (e.g. /lib64).
            continue;
        };
    }

    // Restrict self.
    const restrict_result = raw.syscall3(
        SYS_landlock_restrict_self,
        @intCast(ruleset_fd),
        0,
        0,
    );
    if (restrict_result != 0) return LandlockError.RestrictSelfFailed;

    log.info("landlock.applied workspace={s}", .{workspace_path});
}

fn addPathRule(ruleset_fd: i32, path: []const u8, access: u64) LandlockError!void {
    const fd = std.posix.openat(std.posix.AT.FDCWD, @ptrCast(path.ptr), .{ .ACCMODE = .RDONLY }, 0) catch {
        return LandlockError.PathOpenFailed;
    };
    defer std.posix.close(fd);

    var rule_attr = LandlockPathBeneathAttr{
        .allowed_access = access,
        .parent_fd = fd,
    };

    const result = raw.syscall4(
        SYS_landlock_add_rule,
        @intCast(ruleset_fd),
        LANDLOCK_RULE_PATH_BENEATH,
        @intFromPtr(&rule_attr),
        0,
    );
    if (result != 0) return LandlockError.RuleAddFailed;
}

/// Check if Landlock is available on the current kernel.
pub fn isAvailable() bool {
    if (builtin.os.tag != .linux) return false;

    var attr = LandlockRulesetAttr{ .handled_access_fs = ALL_FS_ACCESS };
    const result = raw.syscall3(
        SYS_landlock_create_ruleset,
        @intFromPtr(&attr),
        @sizeOf(LandlockRulesetAttr),
        0,
    );
    if (result > std.math.maxInt(i32)) return false;
    const fd = @as(i32, @intCast(@as(i64, @bitCast(result))));
    if (fd < 0) return false;
    std.posix.close(@intCast(fd));
    return true;
}

test "isAvailable returns false on non-linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    try std.testing.expect(!isAvailable());
}

test "applyPolicy returns UnsupportedPlatform on non-linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    try std.testing.expectError(LandlockError.UnsupportedPlatform, applyPolicy("/tmp/test"));
}
