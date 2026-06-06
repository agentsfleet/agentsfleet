//! `zombie-runner` — host-resident runner daemon entrypoint. Boots from the
//! operator-installed `zrn_` (env `ZOMBIE_RUNNER_TOKEN`) straight into the
//! heartbeat/lease/execute/report/activity loop (`daemon/loop.zig`) — the host
//! never self-registers (Option B). This file owns process startup: arg
//! dispatch (child-execute mode), config load, the fail-closed `dev_none`
//! startup gate, and handing off to the loop.

const std = @import("std");
const clock = @import("common").clock;
const builtin = @import("builtin");
const logging = @import("log");
const contract = @import("contract");

const Config = @import("daemon/config.zig");
const loop = @import("daemon/loop.zig");
const child_exec = @import("child_exec.zig");
const version_cmd = @import("cmd/version.zig");
const registry = @import("cmd/registry.zig");

const protocol = contract.protocol;

const log = logging.scoped(.zombie_runner);

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

    var gpa: std.heap.DebugAllocator(.{}) = .{};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    // argv is resolved once into the process arena (cleaned automatically on
    // exit); operator subcommands and the child-execute dispatch read this
    // slice. Zig 0.16 removed `std.os.argv` — the entrypoint hands args in via
    // `Init`, alongside the `io` and environment block.
    const argv = init.minimal.args.toSlice(init.arena.allocator()) catch |err| {
        log.err("argv_read_failed", .{ .err = @errorName(err) });
        std.process.exit(1);
    };

    // A CLI subcommand/flag (child-execute mode, --version, …) short-circuits
    // the daemon; a bare invocation (how the systemd unit starts us) returns
    // null and falls through to the loop.
    if (dispatchCli(argv, env_map, io, alloc)) |code| std.process.exit(code);

    const cfg = Config.load(env_map, alloc) catch |err| {
        log.err("config_load_failed", .{ .err = @errorName(err) });
        std.process.exit(1);
    };
    defer cfg.deinit();

    log.info("runner_boot", .{
        .host_id = cfg.host_id,
        .sandbox_tier = cfg.sandbox_tier,
    });

    // Fail-closed (Invariant 7): a release build is a real deployment, so refuse
    // the no-isolation `dev_none` tier (or any unrecognized tier) at startup
    // rather than let it become the production default. Debug builds keep
    // dev_none for local development. `builtin.mode` matches zombied's dev gate.
    if (devNoneForbidden(builtin.mode, sandboxTierFromStr(cfg.sandbox_tier))) {
        log.err("dev_none_rejected_in_release_build", .{ .sandbox_tier = cfg.sandbox_tier });
        std.process.exit(1);
    }

    std.Io.Dir.createDirAbsolute(io, cfg.workspace_base, .default_dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => {
            log.err("workspace_base_mkdir_failed", .{ .path = cfg.workspace_base, .err = @errorName(err) });
            std.process.exit(1);
        },
    };

    // Option B: the env-supplied `zrn_` (prefix-validated in Config.load) IS this
    // runner's identity. No register call — go straight to the loop.
    loop.installDrainHandlers();
    loop.runLoop(io, alloc, cfg, env_map);
    log.info("runner_exit", .{});
}

/// Handle a CLI subcommand/flag if argv carries one, returning the process exit
/// code to use; returns null to fall through to the daemon loop (a bare
/// invocation — how the `zombie-runner.service` unit starts the runner). The
/// single dispatch seam: operator subcommands (register/status/doctor) and
/// `--help` attach here alongside `__execute` and `--version`.
fn dispatchCli(argv: []const [:0]const u8, env_map: *const std.process.Environ.Map, io: std.Io, alloc: std.mem.Allocator) ?u8 {
    if (argv.len <= 1) return null;
    const a1 = argv[1];
    // The forked child re-execs us with `__execute` — run one lease from stdin
    // and exit (no daemon loop, no env config). Hot path, checked first.
    if (std.mem.eql(u8, a1, child_exec.SUBCOMMAND)) return child_exec.run(argv, env_map, alloc);
    if (std.mem.eql(u8, a1, "--version") or std.mem.eql(u8, a1, "-V")) return version_cmd.run();
    // register / status / doctor / --help, and unknown → help + non-zero.
    return registry.dispatch(argv, env_map, io, alloc, a1);
}

/// Parse sandbox tier from env string; defaults to `.dev_none` for unknown
/// values. Single-sourced off the enum (RULE UFS) — no re-spelled tier literals.
fn sandboxTierFromStr(s: []const u8) protocol.SandboxTier {
    return std.meta.stringToEnum(protocol.SandboxTier, s) orelse .dev_none;
}

/// Startup security gate (Invariant 7): a release build refuses the no-isolation
/// `dev_none` tier so it can never be the production default. Debug builds allow
/// it for local development. Pure so the matrix is unit-testable.
fn devNoneForbidden(mode: std.builtin.OptimizeMode, tier: protocol.SandboxTier) bool {
    return mode != .Debug and tier == .dev_none;
}

test "release build forbids dev_none and unknown tiers; Debug allows dev_none" {
    try std.testing.expect(devNoneForbidden(.ReleaseSafe, .dev_none));
    try std.testing.expect(devNoneForbidden(.ReleaseFast, .dev_none));
    try std.testing.expect(devNoneForbidden(.ReleaseSafe, sandboxTierFromStr("garbage"))); // unknown → dev_none
    try std.testing.expect(!devNoneForbidden(.Debug, .dev_none)); // dev convenience
    try std.testing.expect(!devNoneForbidden(.ReleaseSafe, .landlock_full)); // a real tier is fine in prod
}
