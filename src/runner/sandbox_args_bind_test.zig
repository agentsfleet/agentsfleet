//! Bind-contract tests for sandbox_args — the daemon-owned baseline of
//! read-only host paths, and the operator-assigned additions composed onto it.
//!
//! Split from sandbox_args_edge_test.zig on the 350-line bound (RULE FLL); the
//! edge file keeps the argv-shape and platform-arm tests, this one owns which
//! host paths reach a lease and at what mode.
//!
//! The composition tests are deliberately platform-INDEPENDENT: additive-only
//! is the security property of the operator-editable surface, so it is proven
//! on every host via the pure `composeRoBinds`, not only on a Linux runner
//! where `appendBwrap` actually emits flags. The argv-level tests below stay
//! Linux-gated because bwrap flags exist only there.

const std = @import("std");
const builtin = @import("builtin");
const contract = @import("contract");
const common = @import("common");

const sandbox_args = @import("sandbox_args.zig");
const Config = @import("daemon/config.zig");

const LANDLOCK_FULL = contract.protocol.SandboxTier.landlock_full;
const WORKSPACE = "/tmp/fleet-ws-bind";

/// Daemon Config for bind tests; buildArgv reads only the policy fields.
fn cfgWithBinds(extra: []const []const u8) Config {
    return Config{
        .control_plane_url = "http://127.0.0.1:8080",
        .runner_token = "agt_rtest",
        .sandbox_tier = LANDLOCK_FULL,
        .storage_home = "/tmp/agentsfleet-runner",
        .network_policy = .deny_all_egress,
        .worker_count = 1,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        .extra_binds = extra,
        .alloc = std.testing.allocator,
    };
}

fn indexOfStr(argv: []const []const u8, needle: []const u8) ?usize {
    for (argv, 0..) |s, i| {
        if (std.mem.eql(u8, s, needle)) return i;
    }
    return null;
}

/// Index of the `<path> <path>` pair that follows a `--ro-bind-try`, or null.
/// Matching the FLAG plus both operands is what distinguishes a real bind from
/// the path merely appearing somewhere in the argv.
fn roBindTryIndex(argv: []const []const u8, path: []const u8) ?usize {
    if (argv.len < 3) return null;
    for (argv[0 .. argv.len - 2], 0..) |s, i| {
        if (!std.mem.eql(u8, s, "--ro-bind-try")) continue;
        if (std.mem.eql(u8, argv[i + 1], path) and std.mem.eql(u8, argv[i + 2], path)) return i;
    }
    return null;
}

test "test_composed_binds_are_additive_and_baseline_first" {
    // The composition layer — platform-independent, so
    // the additive-only guarantee is proven on every host rather than only on
    // a Linux runner where the bwrap arm actually emits flags.
    var buf: [sandbox_args.MAX_RO_BINDS][]const u8 = undefined;
    const baseline = sandbox_args.RO_SYSTEM_PATHS;

    // No assignment: exactly the baseline, in order.
    const none = sandbox_args.composeRoBinds(&buf, &.{});
    try std.testing.expectEqual(baseline.len, none.len);
    for (baseline, none) |want, got| try std.testing.expectEqualStrings(want, got);

    // With additions: baseline unchanged and still first, operator appended.
    const with = sandbox_args.composeRoBinds(&buf, &.{ "/srv/models", "/usr/share/zoneinfo" });
    try std.testing.expectEqual(baseline.len + 2, with.len);
    for (baseline, with[0..baseline.len]) |want, got| try std.testing.expectEqualStrings(want, got);
    try std.testing.expectEqualStrings("/srv/models", with[baseline.len]);
    try std.testing.expectEqualStrings("/usr/share/zoneinfo", with[baseline.len + 1]);
}

test "test_composed_binds_cannot_drop_a_baseline_path" {
    // The security property: NO assignment shape removes a baseline path. An
    // operator naming exactly the baseline entries gets them twice (harmless —
    // a repeated ro-bind of the same path), never a substitution that could
    // un-bind the resolver and re-create the incident behind this milestone.
    var buf: [sandbox_args.MAX_RO_BINDS][]const u8 = undefined;
    const shadowed = sandbox_args.composeRoBinds(&buf, &.{"/etc"});
    for (sandbox_args.RO_SYSTEM_PATHS) |baseline_path| {
        var seen = false;
        for (shadowed) |got| {
            if (std.mem.eql(u8, got, baseline_path)) seen = true;
        }
        try std.testing.expect(seen);
    }

    // An over-long list (unreachable past extraBindsValid) degrades to the
    // baseline alone — fail closed, never a truncated half-applied set.
    var over: [contract.protocol.MAX_EXTRA_BINDS + 1][]const u8 = undefined;
    for (&over) |*slot| slot.* = "/srv/models";
    const clamped = sandbox_args.composeRoBinds(&buf, &over);
    try std.testing.expectEqual(sandbox_args.RO_SYSTEM_PATHS.len, clamped.len);
}

test "test_operator_bind_reaches_the_argv_read_only" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    // An assigned extra path lands as a read-only bind.
    const cfg = cfgWithBinds(&.{ "/srv/models", "/usr/share/zoneinfo" });
    const argv = sandbox_args.buildArgv(common.globalIo(), alloc, cfg, WORKSPACE, null) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer sandbox_args.freeArgv(alloc, argv);

    try std.testing.expect(roBindTryIndex(argv, "/srv/models") != null);
    try std.testing.expect(roBindTryIndex(argv, "/usr/share/zoneinfo") != null);
    // Read-only ONLY: the workspace stays the single writable bind, so no
    // operator entry can be mounted `--bind`.
    const bind_i = indexOfStr(argv, "--bind").?;
    try std.testing.expectEqualStrings(WORKSPACE, argv[bind_i + 1]);
    for (argv, 0..) |s, i| {
        if (!std.mem.eql(u8, s, "--bind")) continue;
        try std.testing.expectEqualStrings(WORKSPACE, argv[i + 1]);
    }
}

test "test_operator_list_cannot_remove_a_contract_path" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    // The operator list is ADDITIVE. Even an assignment that
    // names nothing, or names only unrelated paths, leaves every baseline path
    // bound: there is no assignment shape that un-binds the resolver and
    // re-creates the incident this milestone came from.
    const cfg = cfgWithBinds(&.{"/srv/models"});
    const argv = sandbox_args.buildArgv(common.globalIo(), alloc, cfg, WORKSPACE, null) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer sandbox_args.freeArgv(alloc, argv);

    for (sandbox_args.RO_SYSTEM_PATHS) |baseline| {
        try std.testing.expect(roBindTryIndex(argv, baseline) != null);
    }
    // And the baseline precedes the operator's additions, so the composition
    // order is "daemon first, operator appended" rather than interleaved.
    const last_baseline = roBindTryIndex(argv, sandbox_args.RO_SYSTEM_PATHS[sandbox_args.RO_SYSTEM_PATHS.len - 1]).?;
    try std.testing.expect(roBindTryIndex(argv, "/srv/models").? > last_baseline);
}

test "should ro-bind the systemd-resolved stub directory so DNS resolves under any network policy" {
    if (builtin.os.tag != .linux) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    // /etc/resolv.conf symlinks to /run/systemd/resolve/stub-resolv.conf on a
    // systemd-resolved host; without this ro-bind the symlink dangles inside
    // the sandbox's own (always-unshared) mount namespace regardless of
    // --share-net, and every outbound DNS lookup fails HostResolutionFailed.
    const argv = sandbox_args.buildArgv(common.globalIo(), alloc, cfgWithBinds(&.{}), WORKSPACE, null) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
    defer sandbox_args.freeArgv(alloc, argv);

    const path_i = indexOfStr(argv, "/run/systemd/resolve").?;
    try std.testing.expectEqualStrings("--ro-bind-try", argv[path_i - 1]);
    try std.testing.expectEqualStrings("/run/systemd/resolve", argv[path_i + 1]);
}
