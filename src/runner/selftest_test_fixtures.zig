//! Shared harness for the self-test probe's real-sandbox suite: the fixture
//! config, the argv builders, and the raw spawn path the tests use to mutate
//! a sandbox before running the probe inside it. Split from
//! `selftest_integration_test.zig` on the 350-line bound (RULE FLL), along the
//! harness/assertion seam — this file makes probes runnable, the test file
//! says what a run must prove.

const std = @import("std");
const contract = @import("contract");
const logging = @import("log");

const child_process = @import("child_process.zig");
const Config = @import("daemon/config.zig");
const sandbox_args = @import("sandbox_args.zig");
const selftest = @import("selftest.zig");
const selftest_probe = @import("selftest_probe.zig");

const log = logging.scoped(.runner_selftest);

pub const WORKSPACE = "/tmp/agentsfleet-selftest-probe-test";

pub fn baseCfg() Config {
    return .{
        .control_plane_url = "http://127.0.0.1:8080",
        .runner_token = "agt_rtest",
        .sandbox_tier = contract.protocol.SandboxTier.landlock_full,
        .storage_home = "/tmp/agentsfleet-runner",
        .network_policy = .allow_all,
        .worker_count = 1,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        // SAFETY: the probe path takes its allocator as a parameter and never
        // reads `cfg.alloc`; every test passes `std.testing.allocator`
        // explicitly, so this field is never observed.
        .alloc = undefined,
    };
}

pub fn spawnIo(threaded: *std.Io.Threaded) std.Io {
    threaded.* = .init(std.testing.allocator, .{});
    return threaded.io();
}

pub fn makeWorkspace(io: std.Io) !void {
    std.Io.Dir.createDirAbsolute(io, WORKSPACE, .default_dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => return err,
    };
}

/// What the faked resolver file says — the systemd-resolved stub listener, the
/// same address the real `stub-resolv.conf` names.
const RESOLVER_FIXTURE_CONTENT = "nameserver 127.0.0.53\n";

/// A resolver layout the resolver-grading tests can run against, and the
/// undo for anything this fixture planted to get one.
pub const ResolverFixture = struct {
    /// True only when THIS fixture wrote the target file. A host that already
    /// had a resolver layout is never modified and never cleaned up.
    planted: bool,

    /// Remove the planted file. Load-bearing, not tidiness: the path is real
    /// host state that production reads — the sandbox emits
    /// `--symlink RESOLV_LINK_TARGET /etc/resolv.conf` and the self-test grades
    /// whether it resolves. Left behind on a box that also runs a runner
    /// daemon, a fabricated `nameserver` line would make the resolver check
    /// report healthy on a host whose leases have no working DNS — the exact
    /// green-probe/dead-sandbox reading this milestone exists to remove.
    /// Directories are left alone: `/run` is shared, and an empty
    /// `/run/systemd/resolve` misleads nobody.
    pub fn deinit(self: ResolverFixture, io: std.Io) void {
        if (!self.planted) return;
        std.Io.Dir.deleteFileAbsolute(io, contract.protocol.RESOLV_LINK_TARGET) catch |err|
            log.warn("selftest_fixture_resolver_cleanup_failed", .{ .err = @errorName(err) });
    }
};

/// Give the caller a resolver layout to grade against, or `null` when this host
/// cannot provide one.
///
/// On a systemd-resolved host the real layout is used untouched. Otherwise the
/// layout is faked — in the disposable kernel-lane container this process is
/// root, and faking is exactly what lets the symlink-into-a-granted-directory
/// mechanics be proven somewhere. `null` means neither path is open (an
/// unprivileged host with a different resolver layout), which is a harness
/// fact: callers skip rather than fail.
///
/// Callers MUST `defer fixture.deinit(io)` — see `ResolverFixture.deinit`.
pub fn ensureResolverTarget(io: std.Io) ?ResolverFixture {
    const target_path = contract.protocol.RESOLV_LINK_TARGET;
    if (std.Io.Dir.accessAbsolute(io, target_path, .{})) |_| {
        return .{ .planted = false };
    } else |_| {}
    const dir_path = std.fs.path.dirname(target_path) orelse return null;
    // Create every ancestor directory of the target, tolerating ones that
    // already exist ("/run" always does).
    var end: usize = 1;
    while (end <= dir_path.len) {
        const next = std.mem.indexOfScalarPos(u8, dir_path, end, '/') orelse dir_path.len;
        std.Io.Dir.createDirAbsolute(io, dir_path[0..next], .default_dir) catch |err| switch (err) {
            error.PathAlreadyExists => {},
            else => return null,
        };
        end = next + 1;
    }
    std.Io.Dir.cwd().writeFile(io, .{ .sub_path = target_path, .data = RESOLVER_FIXTURE_CONTENT }) catch return null;
    return .{ .planted = true };
}

/// Index of the probe's own tail (`<self_exe> __selftest_probe ...`) in a built
/// argv — everything before it is bwrap's option list.
pub fn probeTailIndex(argv: []const []const u8) ?usize {
    for (argv, 0..) |a, i| {
        if (std.mem.eql(u8, a, selftest_probe.SUBCOMMAND)) return i - 1;
    }
    return null;
}

/// Did the probe actually execute here? A silent probe means the child exe this
/// build baked in (`stub_runner_exe_path`) does not resolve on THIS filesystem
/// — true when a cross-compiled binary is run somewhere other than where it was
/// built. That is a harness fact, not a verdict.
///
/// Every real-execution test gates on this. Without the gate,
/// `test_probe_detects_a_dangling_resolver` passes when the probe never ran at
/// all: an empty line parses to every check failing, which looks exactly like a
/// correctly detected dangling resolver. A test that green-lights on a probe
/// that did not run is the same false confidence the milestone exists to
/// remove — so it is checked, not assumed.
pub fn probeRanHere(io: std.Io, alloc: std.mem.Allocator) !bool {
    const argv = try buildArgvOrSkip(io, alloc);
    defer sandbox_args.freeArgv(alloc, argv);
    var buf: [128]u8 = undefined;
    const line = try runProbeArgv(io, alloc, argv, &buf);
    return line.len > 0;
}

/// Build the probe argv, or skip: a host without bubblewrap is a harness fact,
/// not a verdict. Caller frees the result with `sandbox_args.freeArgv`.
pub fn buildArgvOrSkip(io: std.Io, alloc: std.mem.Allocator) ![]const []const u8 {
    return selftest.buildProbeArgv(io, alloc, baseCfg(), WORKSPACE) catch |err| {
        try std.testing.expectEqual(error.BwrapUnavailable, err);
        return error.SkipZigTest;
    };
}

/// Spawn `command` and report how it ended, under the production lease environ
/// — `buildChildEnviron`'s filtered map, so a command that reads `PATH` or
/// `HOME` sees what a lease sees rather than what the test runner has.
///
/// `null` means the run never reported an exit code: the spawn itself failed,
/// or the child was signalled. That is a HARNESS fact, and a caller must not
/// read it as a verdict in either direction.
fn spawnAndWait(io: std.Io, alloc: std.mem.Allocator, command: []const []const u8) !?u8 {
    var daemon_env: std.process.Environ.Map = .init(alloc);
    defer daemon_env.deinit();
    var child_env = try child_process.buildChildEnviron(alloc, &daemon_env);
    defer child_env.deinit();

    var child = std.process.spawn(io, .{
        .argv = command,
        .stdin = .ignore,
        .stdout = .ignore,
        .stderr = .ignore,
        .pgid = 0,
        .environ_map = &child_env,
    }) catch return null;
    const term = child.wait(io) catch return null;
    return switch (term) {
        .exited => |code| code,
        else => null,
    };
}

/// Run `command` on the HOST, outside any sandbox. The control arm for every
/// in-lease proof: without it a non-zero exit inside a lease means "the bind
/// set is wrong" and "this host never had the binary" equally, and the test
/// passes vacuously on the second reading.
pub fn runOnHost(io: std.Io, alloc: std.mem.Allocator, command: []const []const u8) !?u8 {
    return spawnAndWait(io, alloc, command);
}

/// Run `command` inside a REAL lease sandbox and report how it ended.
///
/// Every bind, tmpfs, and symlink stays byte-identical to a production lease —
/// only the probe's own tail is replaced. Composed from `probeTailIndex` rather
/// than by patching the last element: the tail ends in `--workspace=…`, so
/// overwriting one argument runs the REAL probe and grades its verdict instead
/// of the command asked for, which is how the first version of the credential
/// test asserted nothing.
pub fn runInLease(
    io: std.Io,
    alloc: std.mem.Allocator,
    argv: []const []const u8,
    tail: usize,
    command: []const []const u8,
) !?u8 {
    var spliced: std.ArrayList([]const u8) = .empty;
    defer spliced.deinit(alloc);
    try spliced.appendSlice(alloc, argv[0..tail]);
    try spliced.appendSlice(alloc, command);
    return spawnAndWait(io, alloc, spliced.items);
}

/// Spawn a probe argv directly and return its verdict line (into `buf`).
/// Bypasses `selftest_exec.run` deliberately: the tests need to MUTATE the
/// sandbox the probe runs in, which the production path rightly does not allow.
/// The environ is still the production one — the filtered lease map — so the
/// probe grades `CHILD_HOME`, never the test runner's own HOME.
pub fn runProbeArgv(io: std.Io, alloc: std.mem.Allocator, argv: []const []const u8, buf: []u8) ![]const u8 {
    var daemon_env: std.process.Environ.Map = .init(alloc);
    defer daemon_env.deinit();
    var child_env = try child_process.buildChildEnviron(alloc, &daemon_env);
    defer child_env.deinit();
    var child = try std.process.spawn(io, .{
        .argv = argv,
        .stdin = .ignore,
        .stdout = .pipe,
        .stderr = .ignore,
        .pgid = 0,
        .environ_map = &child_env,
    });
    const out = child.stdout orelse return error.NoProbeStdout;
    var fr = out.reader(io, &.{});
    const len = fr.interface.readSliceShort(buf) catch 0;
    // Best-effort reap: the verdict line is already in hand, and a failed wait
    // leaves at worst one zombie inside a short-lived test process — there is
    // nothing for the caller to assert on it.
    _ = child.wait(io) catch |err|
        log.warn("selftest_fixture_wait_failed", .{ .err = @errorName(err) });
    return buf[0..len];
}
