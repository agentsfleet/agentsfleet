//! Landlock filesystem policy enforcement for the host backend.
//!
//! Applies a Landlock ruleset to restrict filesystem access:
//! - Workspace directory: read + write
//! - System paths (/usr, /bin, /lib, /etc): read-only + execute
//! - Everything else: denied by default
//!
//! Uses raw syscalls (Landlock has no libc wrapper).
//! Linux-only; no-ops on other platforms.

const std = @import("std");
const logging = @import("log");
const builtin = @import("builtin");
const protocol = @import("contract").protocol;

const log = logging.scoped(.runner_landlock);

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

const LandlockError = error{
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

/// Read-only paths landlock needs beyond the bind list's baseline: the
/// sandbox floor bwrap constructs (devtmpfs, proc) or ro-binds (/usr) rather
/// than bind-declares, so the shared lists do not carry them. The writable
/// tmpfs floor is NOT here — it takes `WORKSPACE_ACCESS` below, from the same
/// shared list bwrap mounts it from.
const LANDLOCK_FLOOR_RO_PATHS = [_][]const u8{ "/usr", "/dev", "/proc" };

/// System paths that get read-only access in the sandbox. Derived from the
/// bind contract so bwrap and landlock can never disagree on what a lease may
/// read: this list once omitted `/run/systemd/resolve` while bwrap bound it,
/// so `open("/etc/resolv.conf")` followed the symlink into a landlock-denied
/// target and every lease's DNS died — while the self-test, which did not
/// apply landlock, reported the resolver healthy.
const SYSTEM_READONLY_PATHS = protocol.BASELINE_RO_PATHS ++ LANDLOCK_FLOOR_RO_PATHS;

/// Apply Landlock filesystem policy.
/// After this call, the current process can only access:
/// - workspace_path with full RW
/// - system paths with read-only + execute
/// - everything else is denied
pub fn applyPolicy(workspace_path: []const u8, extra_binds: []const protocol.ExtraBind) LandlockError!void {
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
    defer _ = std.os.linux.close(ruleset_fd);

    // Add workspace rule (RW).
    try addPathRule(ruleset_fd, workspace_path, WORKSPACE_ACCESS);

    // The writable floor: every tmpfs bwrap constructs writable is granted
    // write here too, from the same shared list — so mount layer and policy
    // layer can never disagree on where a lease may write. This once diverged
    // by hand: `/tmp` was writable at the mount and read-only here, and every
    // credentialed dial died writing its header file (TempFileCreateFailed)
    // while the un-landlocked host wrote it fine.
    for (protocol.BASELINE_RW_TMPFS) |path| {
        try addPathRule(ruleset_fd, path, WORKSPACE_ACCESS);
    }

    // Add system readonly paths.
    for (SYSTEM_READONLY_PATHS) |path| {
        addPathRule(ruleset_fd, path, SYSTEM_READONLY_ACCESS) catch {
            // Path may not exist on all systems (e.g. /lib64).
            continue;
        };
    }

    // Operator-assigned binds, at the assigned mode. `catch continue` mirrors
    // bwrap's `-try` semantics: a path absent on THIS host is skipped and the
    // self-test reports it, rather than every lease failing on the runner.
    for (extra_binds) |b| {
        const access: u64 = switch (b.mode) {
            .read_only => SYSTEM_READONLY_ACCESS,
            .read_write => WORKSPACE_ACCESS,
        };
        addPathRule(ruleset_fd, b.path, access) catch continue;
    }

    // Restrict self.
    const restrict_result = raw.syscall3(
        SYS_landlock_restrict_self,
        @intCast(ruleset_fd),
        0,
        0,
    );
    if (restrict_result != 0) return LandlockError.RestrictSelfFailed;

    log.debug("landlock_applied", .{ .workspace = workspace_path });
}

fn addPathRule(ruleset_fd: i32, path: []const u8, access: u64) LandlockError!void {
    // Null-terminate cross-platform — the workspace path is a borrowed slice,
    // not a sentinel literal, so openatZ needs an explicit [*:0] form.
    const path_z = std.posix.toPosixPath(path) catch return LandlockError.PathOpenFailed;
    const fd = std.posix.openatZ(std.posix.AT.FDCWD, &path_z, .{ .ACCMODE = .RDONLY }, 0) catch {
        return LandlockError.PathOpenFailed;
    };
    defer _ = std.os.linux.close(fd);

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

test "applyPolicy returns UnsupportedPlatform on non-linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    try std.testing.expectError(LandlockError.UnsupportedPlatform, applyPolicy("/tmp/test", &.{}));
}

test "landlock write set contains every writable-floor path" {
    // The write-side twin of the read-set pin below: a path bwrap mounts
    // writable is never demoted to read-only by the policy layer. (That every
    // floor entry is operator-unbindable is enforced at comptime in
    // protocol_bind.zig — a runtime arm for it here could never fire.)
    for (protocol.BASELINE_RW_TMPFS) |rw| {
        for (SYSTEM_READONLY_PATHS) |ro| {
            try std.testing.expect(!std.mem.eql(u8, ro, rw));
        }
    }
}

test "landlock read set contains every bind-contract path" {
    // The derivation is comptime, but this pins the PROPERTY the M136 incident
    // violated: a path bwrap binds read-only is never landlock-denied.
    for (protocol.BASELINE_RO_PATHS) |contract_path| {
        var found = false;
        for (SYSTEM_READONLY_PATHS) |p| {
            if (std.mem.eql(u8, p, contract_path)) found = true;
        }
        try std.testing.expect(found);
    }
}
