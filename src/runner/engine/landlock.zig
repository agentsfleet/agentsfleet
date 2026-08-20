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

/// The rights landlock accepts on a NON-directory (`ACCESS_FILE` in the
/// kernel's `fs/landlock/syscalls.c`). A rule whose access carries anything
/// outside this set is refused WHOLESALE with `EINVAL` when the target is not
/// a directory — the rule never lands, so the path stays bound and unreadable.
///
/// This matters because the narrowed baseline is the first read set to name a
/// regular FILE: `/etc/hosts`. Every earlier entry (`/etc`, `/usr`, `/lib`,
/// `/run/systemd/resolve`) was a directory, so `READ_DIR` rode along harmlessly
/// and this constraint never fired.
const FILE_ONLY_ACCESS: u64 = LANDLOCK_ACCESS_FS_READ_FILE |
    LANDLOCK_ACCESS_FS_WRITE_FILE |
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

/// Read-only paths landlock needs beyond the bind list's baseline: the sandbox
/// floor bwrap constructs (devtmpfs, proc) rather than bind-declares, so the
/// shared lists do not carry them. The writable tmpfs floor is NOT here — it
/// takes `WORKSPACE_ACCESS` below, from the same shared list bwrap mounts it
/// from.
///
/// `/usr` is NOT listed here even though a lease reads it: it arrives through
/// `BASELINE_RO_PATHS` instead, so the mount layer and the policy layer take it
/// from the same source. A second entry here would be the exact drift this
/// derivation exists to prevent.
const LANDLOCK_FLOOR_RO_PATHS = [_][]const u8{ "/dev", "/proc" };

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

    // Add system readonly paths. The two failure kinds are NOT the same event
    // and no longer share an arm: a path absent on this host is tolerated
    // (bwrap skips it too, via `-try`, so mount and policy still agree), while
    // a rule the KERNEL refused means the path IS present, IS bound, and would
    // be unreadable — the mount-vs-policy divergence this list exists to
    // prevent. Refusing the lease is the fail-closed reading (Invariant 7);
    // swallowing it is how a lease runs with a dead resolver and a green panel.
    for (SYSTEM_READONLY_PATHS) |path| {
        addPathRule(ruleset_fd, path, SYSTEM_READONLY_ACCESS) catch |err| switch (err) {
            LandlockError.PathOpenFailed => continue,
            else => return err,
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
        addPathRule(ruleset_fd, b.path, access) catch |err| switch (err) {
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
        // file (see `FILE_ONLY_ACCESS`), so retry with the subset the kernel
        // accepts for one rather than lose the rule and leave the path bound
        // but unreadable. Narrowing the MASK, never the path set: a file that
        // only ever needed READ_FILE loses nothing, and a genuine failure
        // still surfaces below.
        //
        // Retried rather than decided from an `fstat`: this asks the kernel
        // what it will accept instead of predicting it, so it stays correct if
        // `ACCESS_FILE` gains a member, and it costs one syscall only on the
        // path that would otherwise have been silently dropped.
        const file_only = access & FILE_ONLY_ACCESS;
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

test "the system read mask carries a directory right a regular file cannot take" {
    // Why `addPathRule` retries at all. Landlock refuses a rule on a
    // NON-directory whose access carries any right outside the kernel's
    // ACCESS_FILE set, and `READ_DIR` is outside it — so the full read mask is
    // rejected WHOLESALE (EINVAL) on a regular file. That refusal was being
    // swallowed by `catch continue`, leaving `/etc/hosts` bind-mounted and
    // unreadable with every list test still green.
    //
    // All three halves matter. Drop the first and the retry is dead code; drop
    // the second and the retry cannot succeed; drop the third and the retry
    // lands a rule that grants nothing, which is unreadable by another name.
    try std.testing.expect(SYSTEM_READONLY_ACCESS & LANDLOCK_ACCESS_FS_READ_DIR != 0);
    try std.testing.expectEqual(@as(u64, 0), FILE_ONLY_ACCESS & LANDLOCK_ACCESS_FS_READ_DIR);
    try std.testing.expect(FILE_ONLY_ACCESS & LANDLOCK_ACCESS_FS_READ_FILE != 0);
}

test "the baseline read set names at least one regular file" {
    // The pair to the mask test above: the retry exists because the read set
    // contains FILES, not only directories. `/etc/hosts` was the first, and
    // before it every entry was a directory — which is exactly why the mask
    // was wrong for years without failing anything.
    var has_regular_file = false;
    for (protocol.BASELINE_RO_PATHS) |p| {
        if (std.mem.eql(u8, p, "/etc/hosts")) has_regular_file = true;
        if (std.mem.eql(u8, p, "/etc/nsswitch.conf")) has_regular_file = true;
    }
    try std.testing.expect(has_regular_file);
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
