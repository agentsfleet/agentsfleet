//! sandbox_args.zig — argv for a forked child's exec.
//!
//! `dev_none`: exec the runner's `__execute` mode directly. Sandboxed tiers
//! wrap it in bubblewrap — `--unshare-all` (user/pid/net/ipc/uts/cgroup ns),
//! read-only system paths, read-write workspace, `--die-with-parent` (the
//! sandbox dies if the runner does), and network per `EXECUTOR_NETWORK_POLICY`.
//! Every argv entry is dup'd into the caller's allocator; the caller frees via
//! `freeArgv` after the fork.

const std = @import("std");
const builtin = @import("builtin");
const contract = @import("contract");

const daemon_config = @import("daemon/config.zig");
const network = @import("engine/network.zig");
const child_exec = @import("child_exec.zig");

/// The only tier without isolation — derived from the enum (RULE UFS).
const DEV_NONE = @tagName(contract.protocol.SandboxTier.dev_none);
const BWRAP_PATHS = [_][]const u8{ "/usr/bin/bwrap", "/usr/local/bin/bwrap" };
/// System paths bound read-only when present (`--ro-bind-try` tolerates absence).
const RO_SYSTEM_PATHS = [_][]const u8{ "/etc", "/lib", "/lib64", "/bin", "/sbin", "/opt" };
/// Used at two bind sites (RULE UFS); the rest are single-use bwrap flags whose
/// literal spelling IS bwrap's CLI contract.
const RO_BIND = "--ro-bind";
const SHARE_NET = "--share-net";

/// Build the child's exec argv. Sandboxed tiers prepend a bubblewrap wrapper.
/// Every entry is dup'd into `alloc`; free with `freeArgv`. Errors when a
/// sandboxed tier has no `bwrap` binary — the caller then fails the lease
/// closed (Invariant 7) rather than running unsandboxed.
pub fn buildArgv(alloc: std.mem.Allocator, cfg: daemon_config.Config, workspace_path: []const u8) ![]const []const u8 {
    var list: std.ArrayList([]const u8) = .{};
    errdefer freeList(alloc, &list);

    const self_exe = try std.fs.selfExePathAlloc(alloc);
    defer alloc.free(self_exe);

    const sandboxed = builtin.os.tag == .linux and !std.mem.eql(u8, cfg.sandbox_tier, DEV_NONE);
    if (sandboxed) try appendBwrap(alloc, &list, self_exe, workspace_path);

    try dup(alloc, &list, self_exe);
    try dup(alloc, &list, child_exec.SUBCOMMAND);
    if (sandboxed) try dup(alloc, &list, child_exec.SANDBOXED_FLAG);
    const ws_flag = try std.fmt.allocPrint(alloc, "{s}{s}", .{ child_exec.WORKSPACE_FLAG_PREFIX, workspace_path });
    try list.append(alloc, ws_flag); // already owned by alloc

    return list.toOwnedSlice(alloc);
}

/// Free an argv produced by `buildArgv`.
pub fn freeArgv(alloc: std.mem.Allocator, argv: []const []const u8) void {
    for (argv) |s| alloc.free(s);
    alloc.free(argv);
}

fn freeList(alloc: std.mem.Allocator, list: *std.ArrayList([]const u8)) void {
    for (list.items) |s| alloc.free(s);
    list.deinit(alloc);
}

fn dup(alloc: std.mem.Allocator, list: *std.ArrayList([]const u8), s: []const u8) !void {
    try list.append(alloc, try alloc.dupe(u8, s));
}

/// Append the bubblewrap wrapper: namespaces + ro system + rw workspace + the
/// runner binary ro-bound (so the sandbox can exec it) + network policy + `--`.
fn appendBwrap(alloc: std.mem.Allocator, list: *std.ArrayList([]const u8), self_exe: []const u8, workspace: []const u8) !void {
    const bwrap = bwrapPath() orelse return error.BwrapUnavailable;
    const base = [_][]const u8{
        bwrap,           "--die-with-parent", "--unshare-all",
        "--proc",        "/proc",             "--dev",
        "/dev",          "--tmpfs",           "/tmp",
        RO_BIND,         "/usr",              "/usr",
    };
    for (base) |a| try dup(alloc, list, a);
    for (RO_SYSTEM_PATHS) |p| {
        try dup(alloc, list, "--ro-bind-try");
        try dup(alloc, list, p);
        try dup(alloc, list, p);
    }
    try dup(alloc, list, "--bind");
    try dup(alloc, list, workspace);
    try dup(alloc, list, workspace);
    try dup(alloc, list, RO_BIND);
    try dup(alloc, list, self_exe);
    try dup(alloc, list, self_exe);
    try dup(alloc, list, "--chdir");
    try dup(alloc, list, workspace);
    // deny_all is covered by --unshare-all; registry_allowlist re-shares net.
    if (network.policyFromEnv(alloc) == .registry_allowlist) try dup(alloc, list, SHARE_NET);
    try dup(alloc, list, "--");
}

fn bwrapPath() ?[]const u8 {
    for (BWRAP_PATHS) |p| {
        std.fs.accessAbsolute(p, .{}) catch continue;
        return p;
    }
    return null;
}
