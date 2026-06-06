//! sandbox_args.zig — argv + environment policy for a forked child's exec.
//!
//! `dev_none`: exec the runner's `__execute` mode directly. Sandboxed tiers
//! wrap it in bubblewrap — `--unshare-all` (user/pid/net/ipc/uts/cgroup ns),
//! `--new-session` (detach the controlling terminal), read-only system paths,
//! read-write workspace, `--die-with-parent` (the sandbox dies if the runner
//! does), and network per `RUNNER_NETWORK_POLICY`. Every argv entry is dup'd
//! into the caller's allocator; the caller frees via `freeArgv` after the fork.
//!
//! It also single-sources the child-environment policy (`ENV_PASSTHROUGH_ALLOWLIST`
//! + `ENV_DENY_PREFIX`) that `child_process.forkExec` applies at the spawn
//! boundary — the daemon environ (incl. `ZOMBIE_RUNNER_TOKEN`) never reaches the
//! sandboxed child; it inherits only the allowlist.

const std = @import("std");
const builtin = @import("builtin");
const contract = @import("contract");

const Config = @import("daemon/config.zig");
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

/// Daemon env-var prefix that must NEVER reach a sandboxed child — the
/// control-plane credentials live here (incl. `ZOMBIE_RUNNER_TOKEN`). The
/// allowlist below already excludes it by omission; `forkExec` asserts it absent
/// from the child's environ regardless of allowlist contents (defense-in-depth).
pub const ENV_DENY_PREFIX = "ZOMBIE_";

/// The ONLY environment variables forwarded into a sandboxed child's environ
/// (RULE UFS — single source, referenced by `child_process.forkExec` + tests).
/// Fail-closed: the child inherits EXACTLY these (each only when the daemon has
/// it set) and nothing else, so the daemon environ never leaks. Derived from a
/// verified enumeration of every in-child env read (our `runner_observer`, the
/// NullClaw engine, and tool subprocesses). `RUNNER_*` (parent-only daemon
/// config) and `NULLCLAW_PROVIDER`/`NULLCLAW_MODEL` (delivered on the lease, not
/// env) are deliberately excluded.
pub const ENV_PASSTHROUGH_ALLOWLIST = [_][]const u8{
    "HOME", // NullClaw config dir; absence → error.HomeDirNotFound (load-bearing)
    "PATH", // tool subprocess + wasmtime executable resolution (load-bearing)
    "NULLCLAW_OBSERVER", // optional observer-backend selector (safe default: log)
    "SSL_CERT_FILE", // TLS CA bundle override — pass-through-if-set
    "SSL_CERT_DIR", // TLS CA directory override — pass-through-if-set
    "LANG", // locale — pass-through-if-set
    "LC_ALL", // locale — pass-through-if-set
};

/// Build the child's exec argv. Sandboxed tiers prepend a bubblewrap wrapper.
/// Every entry is dup'd into `alloc`; free with `freeArgv`. Errors when a
/// sandboxed tier has no `bwrap` binary — the caller then fails the lease
/// closed (Invariant 7) rather than running unsandboxed.
pub fn buildArgv(io: std.Io, alloc: std.mem.Allocator, cfg: Config, workspace_path: []const u8) ![]const []const u8 {
    var list: std.ArrayList([]const u8) = .empty;
    errdefer freeList(alloc, &list);

    const self_exe = try std.process.executablePathAlloc(io, alloc);
    defer alloc.free(self_exe);

    const sandboxed = builtin.os.tag == .linux and !std.mem.eql(u8, cfg.sandbox_tier, DEV_NONE);
    if (sandboxed) try appendBwrap(io, alloc, &list, self_exe, workspace_path, cfg.network_policy);

    try dup(alloc, &list, self_exe);
    try dup(alloc, &list, child_exec.SUBCOMMAND);
    if (sandboxed) try dup(alloc, &list, child_exec.SANDBOXED_FLAG);
    const ws_flag = try std.fmt.allocPrint(alloc, "{s}{s}", .{ child_exec.WORKSPACE_FLAG_PREFIX, workspace_path });
    {
        // Scoped so a failed append frees ws_flag exactly once; once appended it
        // is owned by `list` (the outer freeList errdefer), so a later
        // toOwnedSlice failure must not double-free it.
        errdefer alloc.free(ws_flag);
        try list.append(alloc, ws_flag);
    }

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
    const copy = try alloc.dupe(u8, s);
    errdefer alloc.free(copy); // freed once here if append fails; else owned by list
    try list.append(alloc, copy);
}

/// Append the bubblewrap wrapper: namespaces + ro system + rw workspace + the
/// runner binary ro-bound (so the sandbox can exec it) + network policy + `--`.
fn appendBwrap(io: std.Io, alloc: std.mem.Allocator, list: *std.ArrayList([]const u8), self_exe: []const u8, workspace: []const u8, net_policy: network.PolicyMode) !void {
    const bwrap = bwrapPath(io) orelse return error.BwrapUnavailable;
    // `--new-session` detaches the controlling terminal (no TIOCSTI input
    // injection if a tty is ever attached); it sits with the other namespace
    // flags so every sandboxed tier gets it.
    const base = [_][]const u8{
        bwrap,           "--die-with-parent", "--unshare-all",
        "--new-session", "--proc",            "/proc",
        "--dev",         "/dev",              "--tmpfs",
        "/tmp",          RO_BIND,             "/usr",
        "/usr",
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
    if (net_policy == .registry_allowlist) try dup(alloc, list, SHARE_NET);
    try dup(alloc, list, "--");
}

fn bwrapPath(io: std.Io) ?[]const u8 {
    for (BWRAP_PATHS) |p| {
        std.Io.Dir.accessAbsolute(io, p, .{}) catch continue;
        return p;
    }
    return null;
}
