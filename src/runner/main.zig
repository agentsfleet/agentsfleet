//! `zombie-runner` — host-resident runner daemon entrypoint. Boots from the
//! operator-installed `zrn_` (env `ZOMBIE_RUNNER_TOKEN`) straight into the
//! heartbeat/lease/execute/report/activity loop (`daemon/loop.zig`) — the host
//! never self-registers (Option B). This file owns process startup: arg
//! dispatch (child-execute mode), config load, the fail-closed `dev_none`
//! startup gate, and handing off to the loop.

const std = @import("std");
const builtin = @import("builtin");
const logging = @import("log");
const contract = @import("contract");

const Config = @import("daemon/config.zig");
const loop = @import("daemon/loop.zig");
const child_exec = @import("child_exec.zig");

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
    const line = logging.writeLogfmtEnvelope(&line_buf, std.time.milliTimestamp(), @tagName(level), scope_str, msg);
    std.fs.File.stderr().writeAll(line) catch {};
}

pub fn main() void {
    var gpa: std.heap.GeneralPurposeAllocator(.{}) = .{};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    // Child-execute mode: a forked child re-execs us with `__execute` — run one
    // lease from stdin and exit (no daemon loop, no env config).
    if (std.os.argv.len > 1 and std.mem.eql(u8, std.mem.span(std.os.argv[1]), child_exec.SUBCOMMAND)) {
        std.process.exit(child_exec.run(alloc));
    }

    const cfg = Config.load(alloc) catch |err| {
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

    std.fs.makeDirAbsolute(cfg.workspace_base) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => {
            log.err("workspace_base_mkdir_failed", .{ .path = cfg.workspace_base, .err = @errorName(err) });
            std.process.exit(1);
        },
    };

    // Option B: the env-supplied `zrn_` (prefix-validated in Config.load) IS this
    // runner's identity. No register call — go straight to the loop.
    loop.installDrainHandlers();
    loop.runLoop(alloc, cfg);
    log.info("runner_exit", .{});
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

// ── Test aggregator ─────────────────────────────────────────────────────────
// `zig build --build-file build_runner.zig test` — daemon/ + engine/, no pg/redis.
test {
    _ = @import("daemon/control_plane_client.zig");
    _ = @import("daemon/config.zig");
    _ = @import("daemon/loop.zig");
    _ = @import("common");
    _ = @import("child_supervisor.zig");
    _ = @import("child_exec.zig");
    _ = @import("sandbox_args.zig");
    _ = @import("pipe_proto.zig");
    _ = @import("engine/runner.zig");
    _ = @import("engine/types.zig");
    _ = @import("engine/context_budget.zig");
    _ = @import("engine/tool_bridge.zig");
    _ = @import("engine/session.zig");
    _ = @import("engine/cgroup.zig");
    _ = @import("engine/landlock.zig");
    _ = @import("engine/network.zig");
}
