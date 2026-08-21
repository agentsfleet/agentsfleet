//! Landlock filesystem policy enforcement for the host backend.
//!
//! Applies a Landlock ruleset to restrict filesystem access:
//! - Workspace directory: read + write
//! - System paths (the bind contract's `BASELINE_RO_PATHS`): read-only + execute
//! - Everything else: denied by default
//!
//! Uses raw syscalls (Landlock has no libc wrapper).
//! Linux-only; no-ops on other platforms.

const std = @import("std");
const logging = @import("log");
const builtin = @import("builtin");
const protocol = @import("contract").protocol;
const policy = @import("landlock_policy.zig");

/// Re-exported so callers that consume the POLICY through this module — the
/// self-test probe, the lease-hardening proofs — keep one import, and so the
/// set they grade is by construction the set `applyPolicy` grants.
pub const LANDLOCK_FLOOR_RW_FILES = policy.LANDLOCK_FLOOR_RW_FILES;

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

/// Apply Landlock filesystem policy.
/// After this call, the current process can only access:
/// - workspace_path with full RW
/// - system paths with read-only + execute
/// - everything else is denied
pub fn applyPolicy(workspace_path: []const u8, extra_binds: []const protocol.ExtraBind) LandlockError!void {
    if (builtin.os.tag != .linux) return LandlockError.UnsupportedPlatform;

    // Create ruleset.
    var attr = LandlockRulesetAttr{ .handled_access_fs = policy.ALL_FS_ACCESS };
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
    try addPathRule(ruleset_fd, workspace_path, policy.WORKSPACE_ACCESS);

    // The writable floor: every tmpfs bwrap constructs writable is granted
    // write here too, from the same shared list — so mount layer and policy
    // layer can never disagree on where a lease may write. This once diverged
    // by hand: `/tmp` was writable at the mount and read-only here, and every
    // credentialed dial died writing its header file (TempFileCreateFailed)
    // while the un-landlocked host wrote it fine.
    for (protocol.BASELINE_RW_TMPFS) |path| {
        try addPathRule(ruleset_fd, path, policy.WORKSPACE_ACCESS);
    }

    // Add system readonly paths. The two failure kinds are NOT the same event
    // and no longer share an arm: a path absent on this host is tolerated
    // (bwrap skips it too, via `-try`, so mount and policy still agree), while
    // a rule the KERNEL refused means the path IS present, IS bound, and would
    // be unreadable — the mount-vs-policy divergence this list exists to
    // prevent. Refusing the lease is the fail-closed reading (Invariant 7);
    // swallowing it is how a lease runs with a dead resolver and a green panel.
    for (policy.SYSTEM_READONLY_PATHS) |path| {
        addPathRule(ruleset_fd, path, policy.SYSTEM_READONLY_ACCESS) catch |err| switch (err) {
            LandlockError.PathOpenFailed => continue,
            else => return err,
        };
    }

    // The writable device files, after the floor granted their directory read.
    // Landlock rules are additive and the nested rule wins for its own path, so
    // `/dev` stays read-only while `/dev/null` gains write.
    //
    // `try`, not the `catch continue` the two lists around this one use: those
    // tolerate a path absent on THIS host because bwrap skipped it too, whereas
    // `--dev` constructs this one on every sandbox. A missing `/dev/null` is a
    // sandbox that was not built, and running a lease in it only moves the
    // failure somewhere with less to say about it.
    for (policy.LANDLOCK_FLOOR_RW_FILES) |path| {
        try addPathRule(ruleset_fd, path, policy.FILE_ONLY_ACCESS);
    }

    // Operator-assigned binds, at the assigned mode. `catch continue` mirrors
    // bwrap's `-try` semantics: a path absent on THIS host is skipped and the
    // self-test reports it, rather than every lease failing on the runner.
    for (extra_binds) |b| {
        addPathRule(ruleset_fd, b.path, policy.accessForBindMode(b.mode)) catch |err| switch (err) {
            LandlockError.PathOpenFailed => continue,
            else => return err,
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

    addRuleForFd(ruleset_fd, fd, access) catch {
        // A refusal here is most likely a directory right offered on a regular
        // file (see `policy.FILE_ONLY_ACCESS`), so retry with the subset the kernel
        // accepts for one rather than lose the rule and leave the path bound
        // but unreadable. Narrowing the MASK, never the path set: a file that
        // only ever needed READ_FILE loses nothing, and a genuine failure
        // still surfaces below.
        //
        // Retried rather than decided from an `fstat`: this asks the kernel
        // what it will accept instead of predicting it, so it stays correct if
        // `ACCESS_FILE` gains a member, and it costs one syscall only on the
        // path that would otherwise have been silently dropped.
        const file_only = access & policy.FILE_ONLY_ACCESS;
        if (file_only == access) return LandlockError.RuleAddFailed;
        return addRuleForFd(ruleset_fd, fd, file_only);
    };
}

/// Attach one rule to `ruleset_fd` for an already-open `fd`. Split from
/// `addPathRule` so the mask-narrowing retry above reuses the exact same
/// syscall on the same descriptor — reopening the path would race a rename.
fn addRuleForFd(ruleset_fd: i32, fd: i32, access: u64) LandlockError!void {
    var rule_attr = LandlockPathBeneathAttr{
        .allowed_access = access,
        .parent_fd = fd,
    };

    const result = raw.syscall4(
        SYS_landlock_add_rule,
        @intCast(ruleset_fd),
        policy.LANDLOCK_RULE_PATH_BENEATH,
        @intFromPtr(&rule_attr),
        0,
    );
    if (result != 0) return LandlockError.RuleAddFailed;
}

test "applyPolicy returns UnsupportedPlatform on non-linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    try std.testing.expectError(LandlockError.UnsupportedPlatform, applyPolicy("/tmp/test", &.{}));
}
