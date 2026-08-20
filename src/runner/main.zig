//! `agentsfleet-runner` — host-resident runner daemon entrypoint. Boots from the
//! operator-installed `agt_r` (env `AGENTSFLEET_RUNNER_TOKEN`) straight into the
//! heartbeat/lease/execute/report/activity loop (`daemon/loop.zig`) — the host
//! never self-registers (Option B). This file owns process startup: arg
//! dispatch (child-execute mode), config load, the fail-closed `dev_none`
//! startup gate, and handing off to the loop.

const std = @import("std");
const clock = @import("common").clock;
const builtin = @import("builtin");
const logging = @import("log");

const Config = @import("daemon/config.zig");
const StorageHome = @import("daemon/StorageHome.zig");
const loop = @import("daemon/loop.zig");
const startup = @import("daemon/startup.zig");
const runner_deadline = @import("daemon/runner_deadline.zig");
const child_exec = @import("child_exec.zig");
const selftest_probe = @import("selftest_probe.zig");
const client_errors = @import("engine/client_errors.zig");
const version_cmd = @import("cmd/version.zig");
const registry = @import("cmd/registry.zig");

const log = logging.scoped(.fleet_runner);
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;
const ERR_EXEC_RUNNER_INVALID_CONFIG = client_errors.ERR_EXEC_RUNNER_INVALID_CONFIG;

pub const std_options: std.Options = .{
    .logFn = runnerLog,
};

fn runnerLog(
    comptime level: std.log.Level,
    comptime scope: @TypeOf(.enum_literal),
    comptime fmt: []const u8,
    args: anytype,
) void {
    const scope_str = comptime if (scope == .default) "default" else @tagName(scope);
    var msg_buf: [2048]u8 = undefined;
    const msg = std.fmt.bufPrint(&msg_buf, fmt, args) catch return;
    var line_buf: [4096]u8 = undefined;
    const line = logging.writeLogfmtEnvelope(&line_buf, clock.nowMillis(), @tagName(level), scope_str, msg);
    logging.writeStderrLine(line);
}

pub fn main(init: std.process.Init) void {
    const io = init.io;
    const env_map = init.environ_map;

    // Invariant 5: Debug keeps the leak-checking allocator; a release build uses
    // the fast thread-safe smp_allocator (the daemon must not run the
    // DebugAllocator in production). `deinit` runs unconditionally — a no-op when
    // nothing was allocated through it in release.
    var debug_gpa: std.heap.DebugAllocator(.{}) = .{};
    defer _ = debug_gpa.deinit();
    const alloc = switch (allocatorKind(builtin.mode)) {
        .debug_leak_checked => debug_gpa.allocator(),
        .release_smp => std.heap.smp_allocator,
    };

    // argv is resolved once into the process arena (cleaned automatically on
    // exit); operator subcommands and the child-execute dispatch read this
    // slice. Zig 0.16 removed `std.os.argv` — the entrypoint hands args in via
    // `Init`, alongside the `io` and environment block.
    const argv = init.minimal.args.toSlice(init.arena.allocator()) catch |err| {
        log.err("argv_read_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err) });
        std.process.exit(1);
    };

    // A CLI subcommand/flag (child-execute mode, --version, …) short-circuits
    // the daemon; a bare invocation (how the systemd unit starts us) returns
    // null and falls through to the loop.
    if (dispatchCli(argv, env_map, io, alloc)) |code| std.process.exit(code);

    const cfg = Config.load(env_map, alloc) catch |err| {
        log.err("config_load_failed", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .err = @errorName(err) });
        std.process.exit(1);
    };
    defer cfg.deinit();

    // The tier / egress / worker policy is not known here: it is ASSIGNED by
    // the control plane and arrives with the first heartbeat. The apply-time
    // gate (release-build dev_none refusal) runs in the loop when the
    // assignment lands — a failed gate refuses leases and keeps heartbeating,
    // so the dashboard shows why instead of a crash loop.
    log.info("server_started", .{ .storage_home = cfg.storage_home });

    // Before the loop, so a populated cgroup subtree is a post-condition of the
    // daemon being up rather than a race with the first assignment. Non-fatal —
    // see `daemon/startup.zig` for why.
    startup.enableResourceControl(io, alloc);

    std.Io.Dir.createDirAbsolute(io, cfg.storage_home, .default_dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => {
            log.err("storage_home_mkdir_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = cfg.storage_home, .err = @errorName(err) });
            std.process.exit(1);
        },
    };

    // Claim the storage home and reap the workspaces an unclean shutdown
    // orphaned — here, before the poll loop, "no lease is held" is a fact rather
    // than a hope. The claim is held for the process lifetime: releasing it
    // early would let a second daemon sweep this home while our leases are live.
    // An unclaimable home is logged and the daemon runs on without reaping.
    var storage = StorageHome.claimAndSweep(io, cfg.storage_home);
    defer if (storage.home) |*claimed| claimed.close(io);

    // The ONE process scheduler every outbound control-plane call arms against.
    // Registered after `cfg`, so it is torn down FIRST (LIFO): `runLoop` has
    // already joined every worker by then, so no thread can arm into storage
    // that is going away — stop → join network users → deinit scheduler.
    var deadlines: runner_deadline.Owned = .{};
    defer deadlines.deinit();
    const sched = deadlines.start(alloc);

    // Option B: the env-supplied `agt_r` (prefix-validated in Config.load) IS this
    // runner's identity. No register call — go straight to the loop.
    loop.installDrainHandlers();
    const exit_reason = loop.runLoop(io, alloc, sched, cfg, env_map);
    log.info("server_stopped", .{ .reason = @tagName(exit_reason) });
    // A rejected runner token can never self-heal — exit non-zero so systemd's
    // restart + the deploy health check surface it as a loud, named failure
    // (server_stopped reason=token_rejected in journald) instead of a runner that
    // stays "up" while never leasing work. Other exits are a clean stop.
    if (exit_reason == .token_rejected) std.process.exit(1);
}

/// Handle a CLI subcommand/flag if argv carries one, returning the process exit
/// code to use; returns null to fall through to the daemon loop (a bare
/// invocation — how the `agentsfleet-runner.service` unit starts the runner). The
/// single dispatch seam: operator subcommands (register/status/doctor) and
/// `--help` attach here alongside `__execute` and `--version`.
fn dispatchCli(argv: []const [:0]const u8, env_map: *const std.process.Environ.Map, io: std.Io, alloc: std.mem.Allocator) ?u8 {
    if (argv.len <= 1) return null;
    const a1 = argv[1];
    // The forked child re-execs us with `__execute` — run one lease from stdin
    // and exit (no daemon loop, no env config). Hot path, checked first.
    if (std.mem.eql(u8, a1, child_exec.SUBCOMMAND)) return child_exec.run(argv, env_map, alloc);
    // The self-test probe re-execs us inside a sandbox — answer three
    // connectivity questions and exit. Sits beside `__execute` (before the
    // operator registry) so it stays out of `--help`: `doctor` remains the one
    // host-side entry point, per the spec's UNCHANGED command surface.
    if (std.mem.eql(u8, a1, selftest_probe.SUBCOMMAND)) return selftest_probe.run(argv, env_map, io);
    if (std.mem.eql(u8, a1, "--version") or std.mem.eql(u8, a1, "-V")) return version_cmd.run();
    // Operator subcommands reach the control plane, so they need the process
    // scheduler — OWNED here (this branch always returns, so a process still
    // owns exactly one) but STARTED lazily by the handler that arms it: the
    // help/unknown/`<cmd> --help` paths never pay a worker spawn, and a spawn
    // failure can never eat a help request.
    var deadlines: runner_deadline.Owned = .{};
    defer deadlines.deinit();
    // register / status / doctor / --help, and unknown → help + non-zero.
    return registry.dispatch(argv, env_map, io, alloc, &deadlines, a1);
}

/// Allocator selected by build mode (M100, Invariant 5). Debug keeps the
/// leak-checking DebugAllocator; a release build uses the fast thread-safe
/// `smp_allocator` — the production daemon must not pay the DebugAllocator's
/// bookkeeping cost. Pure so the choice is unit-testable.
const AllocatorKind = enum { debug_leak_checked, release_smp };

fn allocatorKind(mode: std.builtin.OptimizeMode) AllocatorKind {
    return if (mode == .Debug) .debug_leak_checked else .release_smp;
}

test "release builds select the non-Debug allocator (Invariant 5)" {
    try std.testing.expectEqual(AllocatorKind.debug_leak_checked, allocatorKind(.Debug));
    try std.testing.expectEqual(AllocatorKind.release_smp, allocatorKind(.ReleaseSafe));
    try std.testing.expectEqual(AllocatorKind.release_smp, allocatorKind(.ReleaseFast));
    try std.testing.expectEqual(AllocatorKind.release_smp, allocatorKind(.ReleaseSmall));
}
