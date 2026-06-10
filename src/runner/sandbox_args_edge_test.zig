//! Edge tests for sandbox_args.buildArgv — the forked child's exec argv.
//!
//! buildArgv only prepends the bubblewrap wrapper on Linux when the tier is
//! NOT dev_none (`builtin.os.tag == .linux and tier != dev_none`). On every
//! other host the child execs the runner's `__execute` mode directly, with no
//! bwrap prefix and no `--sandboxed` flag — the in-process sandbox (Landlock
//! on Linux, Seatbelt on macOS) is established later by child_exec.run itself,
//! not signalled through this argv on a non-Linux build. These tests assert the
//! REAL platform behaviour, gating the bwrap arms behind builtin.os.tag so they
//! exercise the wrapper only where it actually runs.

const std = @import("std");
const builtin = @import("builtin");
const contract = @import("contract");
const common = @import("common");

const sandbox_args = @import("sandbox_args.zig");
const child_exec = @import("child_exec.zig");
const Config = @import("daemon/config.zig");

const DEV_NONE = @tagName(contract.protocol.SandboxTier.dev_none);
const LANDLOCK_FULL = @tagName(contract.protocol.SandboxTier.landlock_full);
const CONTAINER_NESTED = @tagName(contract.protocol.SandboxTier.container_nested);
const WORKSPACE = "/tmp/zombie-ws-edge";

/// Build a daemon Config struct literal for argv tests. buildArgv reads only
/// `sandbox_tier`; the other slices are inert placeholders, never freed here
/// (no Config.deinit — these are static literals, not allocator-owned).
fn cfgWithTier(tier: []const u8) Config {
    return Config{
        .control_plane_url = "http://127.0.0.1:8080",
        .runner_token = "zrn_test",
        .host_id = "host-edge",
        .sandbox_tier = tier,
        .workspace_base = "/tmp/zombie-runner",
        .network_policy = .deny_all,
        .worker_count = 1,
        .registry_allowlist = &.{},
        .alloc = std.testing.allocator,
    };
}

/// True when this argv entry equals the runner self-exe path (the first non-flag
/// program token). Heuristic: dev_none argv[0] is the self-exe; we assert by
/// position rather than spelling so the test is host-path-independent.
fn indexOfStr(argv: []const []const u8, needle: []const u8) ?usize {
    for (argv, 0..) |s, i| {
        if (std.mem.eql(u8, s, needle)) return i;
    }
    return null;
}

test "should build dev_none argv without bwrap when tier is dev_none" {
    const alloc = std.testing.allocator;
    const argv = try sandbox_args.buildArgv(common.globalIo(), alloc, cfgWithTier(DEV_NONE), WORKSPACE, null);
    defer sandbox_args.freeArgv(alloc, argv);

    // dev_none: [ self_exe, __execute, --workspace=<ws> ] — exactly 3 entries,
    // no bwrap prefix, no --sandboxed flag.
    try std.testing.expectEqual(@as(usize, 3), argv.len);
    try std.testing.expectEqualStrings(child_exec.SUBCOMMAND, argv[1]);
    const ws_flag = argv[2];
    try std.testing.expect(std.mem.startsWith(u8, ws_flag, child_exec.WORKSPACE_FLAG_PREFIX));
    try std.testing.expectEqualStrings(WORKSPACE, ws_flag[child_exec.WORKSPACE_FLAG_PREFIX.len..]);
    // No bwrap, no --sandboxed anywhere.
    try std.testing.expect(indexOfStr(argv, "--unshare-all") == null);
    try std.testing.expect(indexOfStr(argv, child_exec.SANDBOXED_FLAG) == null);
}

test "should build landlock_full argv with bwrap wrapper on Linux when tier is required" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    // On Linux a missing bwrap binary makes buildArgv fail-closed; only assert
    // the wrapper shape when bwrap is actually present on this host.
    const argv = sandbox_args.buildArgv(common.globalIo(), alloc, cfgWithTier(LANDLOCK_FULL), WORKSPACE, null) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer sandbox_args.freeArgv(alloc, argv);

    // bwrap prefix: argv[0] is a bwrap path, namespaces are unshared, the child
    // dies with the parent.
    try std.testing.expect(std.mem.endsWith(u8, argv[0], "bwrap"));
    try std.testing.expect(indexOfStr(argv, "--die-with-parent") != null);
    try std.testing.expect(indexOfStr(argv, "--unshare-all") != null);
    // Workspace is bound read-write (--bind <ws> <ws>) and system paths ro.
    const bind_i = indexOfStr(argv, "--bind").?;
    try std.testing.expectEqualStrings(WORKSPACE, argv[bind_i + 1]);
    try std.testing.expectEqualStrings(WORKSPACE, argv[bind_i + 2]);
    try std.testing.expect(indexOfStr(argv, "--ro-bind") != null);
    // Tail: __execute + --sandboxed + --workspace=<ws> after the bwrap `--`.
    const sep = indexOfStr(argv, "--").?;
    try std.testing.expectEqualStrings(child_exec.SUBCOMMAND, argv[sep + 2]);
    try std.testing.expectEqualStrings(child_exec.SANDBOXED_FLAG, argv[sep + 3]);
    const ws_flag = argv[argv.len - 1];
    try std.testing.expect(std.mem.startsWith(u8, ws_flag, child_exec.WORKSPACE_FLAG_PREFIX));
}

test "should fail with BwrapUnavailable when required tier has no bwrap on Linux" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    // Only meaningful when this host genuinely lacks bwrap in both standard
    // paths — otherwise the wrapper builds and there is nothing to fail.
    const have_bwrap = blk: {
        std.Io.Dir.accessAbsolute(common.globalIo(), "/usr/bin/bwrap", .{}) catch {
            std.Io.Dir.accessAbsolute(common.globalIo(), "/usr/local/bin/bwrap", .{}) catch break :blk false;
            break :blk true;
        };
        break :blk true;
    };
    if (have_bwrap) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    try std.testing.expectError(
        error.BwrapUnavailable,
        sandbox_args.buildArgv(common.globalIo(), alloc, cfgWithTier(LANDLOCK_FULL), WORKSPACE, null),
    );
}

test "should skip bwrap on non-Linux even when tier is required" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    // On macOS/other a required tier (container_nested) still produces a bare
    // argv: bwrap is Linux-only, so no wrapper and no --sandboxed flag are
    // added here. The in-process sandbox is established by child_exec.run, not
    // signalled through this argv on a non-Linux build.
    const argv = try sandbox_args.buildArgv(common.globalIo(), alloc, cfgWithTier(CONTAINER_NESTED), WORKSPACE, null);
    defer sandbox_args.freeArgv(alloc, argv);

    try std.testing.expectEqual(@as(usize, 3), argv.len);
    try std.testing.expectEqualStrings(child_exec.SUBCOMMAND, argv[1]);
    try std.testing.expect(indexOfStr(argv, "--unshare-all") == null);
    try std.testing.expect(indexOfStr(argv, child_exec.SANDBOXED_FLAG) == null);
    const ws_flag = argv[2];
    try std.testing.expect(std.mem.startsWith(u8, ws_flag, child_exec.WORKSPACE_FLAG_PREFIX));
}

test "should omit --share-net under the default deny_all network policy on Linux" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    // buildArgv reads RUNNER_NETWORK_POLICY via network/Policy.fromMap; absent
    // or unset it resolves to deny_all, so the bwrap wrapper unshares the net
    // and adds NO --share-net (host network stays isolated). Skip if the test
    // host has explicitly opted into registry_allowlist via the env var, since
    // then --share-net is the correct output. The positive --share-net arm is
    // covered deterministically in network.zig against appendBwrapNetworkArgs
    // directly (no env coupling).
    const opted_in = blk: {
        const raw = common.env.testLiveValue("RUNNER_NETWORK_POLICY") orelse break :blk false;
        break :blk std.ascii.eqlIgnoreCase(raw, "registry_allowlist");
    };
    if (opted_in) return error.SkipZigTest;

    const argv = sandbox_args.buildArgv(common.globalIo(), alloc, cfgWithTier(LANDLOCK_FULL), WORKSPACE, null) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer sandbox_args.freeArgv(alloc, argv);

    // deny_all (default): --unshare-all isolates the net, no --share-net added.
    try std.testing.expect(indexOfStr(argv, "--unshare-all") != null);
    try std.testing.expect(indexOfStr(argv, "--share-net") == null);
}

/// Like `cfgWithTier` but re-shares the network (registry_allowlist) — used to
/// prove the tty-detach flag is emitted on every sandboxed tier, not just deny_all.
fn cfgAllowAll(tier: []const u8) Config {
    var c = cfgWithTier(tier);
    c.network_policy = .allow_all;
    return c;
}

fn cfgRegistryAllowlist(tier: []const u8) Config {
    var c = cfgWithTier(tier);
    c.network_policy = .registry_allowlist; // strict, kernel-enforced
    return c;
}

test "should detach the controlling terminal with --new-session in the bwrap prefix on Linux" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const argv = sandbox_args.buildArgv(common.globalIo(), alloc, cfgWithTier(LANDLOCK_FULL), WORKSPACE, null) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer sandbox_args.freeArgv(alloc, argv);

    // --new-session sits among the namespace flags, BEFORE the bwrap `--`
    // separator, so the agent has no controlling terminal (no TIOCSTI vector).
    const ns = indexOfStr(argv, "--new-session");
    try std.testing.expect(ns != null);
    const sep = indexOfStr(argv, "--").?;
    try std.testing.expect(ns.? < sep);
}

test "allow_all (default) re-shares host net; registry_allowlist (strict) does NOT" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    // allow_all (the default posture): re-shares the host netns (--share-net)
    // so the lease has full egress until enforcement lands (2.0.1).
    const open = sandbox_args.buildArgv(common.globalIo(), alloc, cfgAllowAll(LANDLOCK_FULL), WORKSPACE, null) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer sandbox_args.freeArgv(alloc, open);
    try std.testing.expect(indexOfStr(open, "--share-net") != null);
    try std.testing.expect(indexOfStr(open, "--unshare-all") != null);
    try std.testing.expect(indexOfStr(open, "--new-session") != null);

    // registry_allowlist (strict) keeps its own (filtered) netns — never re-shares.
    const strict = try sandbox_args.buildArgv(common.globalIo(), alloc, cfgRegistryAllowlist(LANDLOCK_FULL), WORKSPACE, null);
    defer sandbox_args.freeArgv(alloc, strict);
    try std.testing.expect(indexOfStr(strict, "--share-net") == null);
    try std.testing.expect(indexOfStr(strict, "--unshare-all") != null);
}

test "egress files are ro-bound over /etc/hosts + /etc/resolv.conf when supplied" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const egress = sandbox_args.EgressFiles{
        .hosts_path = "/run/uz/lease0/hosts",
        .resolv_path = "/run/uz/lease0/resolv.conf",
    };
    // Strict posture: resolver files bound + no --share-net (the option-D path).
    const argv = sandbox_args.buildArgv(common.globalIo(), alloc, cfgRegistryAllowlist(LANDLOCK_FULL), WORKSPACE, egress) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer sandbox_args.freeArgv(alloc, argv);

    // The host-side rendered file is the source; the in-sandbox target is the
    // canonical /etc path. Both binds precede the `--` exec separator.
    const sep = indexOfStr(argv, "--").?;
    const hosts_src = indexOfStr(argv, "/run/uz/lease0/hosts").?;
    const hosts_dst = indexOfStr(argv, "/etc/hosts").?;
    const resolv_dst = indexOfStr(argv, "/etc/resolv.conf").?;
    try std.testing.expect(hosts_src < sep);
    try std.testing.expect(hosts_dst < sep);
    try std.testing.expect(resolv_dst < sep);
    // still no --share-net even with egress files present.
    try std.testing.expect(indexOfStr(argv, "--share-net") == null);
}

test "should have no memory leaks freeing dev_none argv over many iterations" {
    const alloc = std.testing.allocator;
    // std.testing.allocator panics on any leak; 100 create-free cycles prove
    // every dup'd entry is reclaimed by freeArgv.
    for (0..100) |_| {
        const argv = try sandbox_args.buildArgv(common.globalIo(), alloc, cfgWithTier(DEV_NONE), WORKSPACE, null);
        sandbox_args.freeArgv(alloc, argv);
    }
}

test "should have no memory leaks freeing bwrap argv over many iterations" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    // The bwrap path allocates many more entries (namespaces + ro binds); prove
    // freeArgv reclaims all of them across 50 cycles. Skip if bwrap is absent.
    for (0..50) |_| {
        const argv = sandbox_args.buildArgv(common.globalIo(), alloc, cfgWithTier(LANDLOCK_FULL), WORKSPACE, null) catch |err| {
            try std.testing.expectEqual(error.BwrapUnavailable, err);
            return error.SkipZigTest;
        };
        sandbox_args.freeArgv(alloc, argv);
    }
}

test "should surface OutOfMemory under allocation failure without leaking" {
    // buildArgv allocates and can return an error; the failing-allocator harness
    // proves every partial allocation is unwound (errdefer freeList) on OOM.
    // checkAllAllocationFailures(backing, fn, extra_args): the injected failing
    // allocator is the fn's first param; no extra args beyond it.
    try std.testing.checkAllAllocationFailures(
        std.testing.allocator,
        struct {
            fn run(alloc: std.mem.Allocator) !void {
                const argv = sandbox_args.buildArgv(common.globalIo(), alloc, cfgWithTier(DEV_NONE), WORKSPACE, null) catch |err| {
                    if (err == error.OutOfMemory) return err;
                    return; // BwrapUnavailable etc. — not an allocation outcome
                };
                sandbox_args.freeArgv(alloc, argv);
            }
        }.run,
        .{},
    );
}
