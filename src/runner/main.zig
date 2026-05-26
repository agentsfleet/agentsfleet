//! `zombie-runner` — host-resident runner daemon, parent event-leasing loop.
//! register() once → heartbeat/lease/execute/report/activity loop. Transport
//! errors back off without crashing; un-acked leases re-deliver via reclaim.
//! Each lease runs in a forked, sandboxed child; the child streams live-tail
//! `activity` frames over the pipe, which the parent forwards to the control plane.

const std = @import("std");
const logging = @import("log");
const contract = @import("contract");
const constants = @import("common");

const daemon_config = @import("daemon/config.zig");
const client_mod = @import("daemon/control_plane_client.zig");
const child_supervisor = @import("child_supervisor.zig");
const child_exec = @import("child_exec.zig");

const protocol = contract.protocol;

const log = logging.scoped(.zombie_runner);

/// Backoff (ms) on control-plane transport errors; lease polls use server-supplied retry_after_ms.
const TRANSPORT_ERROR_BACKOFF_MS: u64 = 2_000;
/// Consecutive heartbeat errors before escalating the sleep multiplier.
const HEARTBEAT_MAX_CONSECUTIVE_ERRORS: u32 = 5;

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

    const cfg = daemon_config.Config.load(alloc) catch |err| {
        log.err("config_load_failed", .{ .err = @errorName(err) });
        std.process.exit(1);
    };
    defer cfg.deinit();

    log.info("runner_boot", .{
        .host_id = cfg.host_id,
        .sandbox_tier = cfg.sandbox_tier,
    });

    std.fs.makeDirAbsolute(cfg.workspace_base) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => {
            log.err("workspace_base_mkdir_failed", .{ .path = cfg.workspace_base, .err = @errorName(err) });
            std.process.exit(1);
        },
    };

    const cp = client_mod{ .base_url = cfg.control_plane_url, .register_token = cfg.register_token };
    const runner_token: []u8 = registerWithRetry(alloc, cp, cfg);
    defer alloc.free(runner_token);

    runLoop(alloc, cp, runner_token, cfg);
    log.info("runner_exit", .{});
}

/// POST register with infinite retry + backoff until the control plane responds.
fn registerWithRetry(alloc: std.mem.Allocator, cp: client_mod, cfg: daemon_config.Config) []u8 {
    const reg_req = protocol.RegisterRequest{
        .host_id = cfg.host_id,
        .sandbox_tier = sandboxTierFromStr(cfg.sandbox_tier),
        .labels = cfg.labels,
    };
    while (true) {
        const tok = cp.register(alloc, reg_req) catch |err| {
            log.err("register_failed", .{ .err = @errorName(err) });
            sleepMs(TRANSPORT_ERROR_BACKOFF_MS);
            continue;
        };
        log.info("registered", .{});
        return tok;
    }
}

/// Heartbeat → lease → execute → report loop. Returns on stop or drain signal.
fn runLoop(alloc: std.mem.Allocator, cp: client_mod, runner_token: []const u8, cfg: daemon_config.Config) void {
    var draining = false;
    var heartbeat_errors: u32 = 0;

    outer: while (true) {
        const hb = cp.heartbeat(alloc, runner_token) catch |err| {
            heartbeat_errors += 1;
            log.warn("heartbeat_failed", .{ .err = @errorName(err), .consecutive = heartbeat_errors });
            const mult: u64 = if (heartbeat_errors >= HEARTBEAT_MAX_CONSECUTIVE_ERRORS) heartbeat_errors else 1;
            sleepMs(TRANSPORT_ERROR_BACKOFF_MS * mult);
            continue :outer;
        };
        heartbeat_errors = 0;

        switch (hb.status) {
            .stop => { log.info("fleet_stop", .{}); break :outer; },
            .drain => { if (!draining) log.info("fleet_drain", .{}); draining = true; },
            .ok => {},
        }

        if (draining) { log.info("drain_idle_exit", .{}); break :outer; }

        const lease_parsed = cp.lease(alloc, runner_token) catch |err| {
            log.warn("lease_failed", .{ .err = @errorName(err) });
            sleepMs(TRANSPORT_ERROR_BACKOFF_MS);
            continue :outer;
        };
        defer lease_parsed.deinit();

        const lease_resp = lease_parsed.value;
        if (lease_resp.lease == null) {
            const wait_ms: u64 = lease_resp.retry_after_ms orelse constants.NO_WORK_RETRY_AFTER_MS;
            log.info("no_work", .{ .retry_after_ms = wait_ms });
            sleepMs(wait_ms);
            continue :outer;
        }

        executeAndReport(alloc, cp, runner_token, cfg, lease_resp.lease.?);
    }
}

/// Forwards each `activity` frame the sandboxed child streams to the control
/// plane's `activity` verb. Best-effort by contract — `cp.activity` swallows
/// transport errors, so a dropped live-tail frame never disturbs execution.
const ActivityForwarder = struct {
    alloc: std.mem.Allocator,
    cp: client_mod,
    runner_token: []const u8,
    lease_id: []const u8,

    fn forward(ctx: *anyopaque, frame: contract.activity.ActivityFrame) void {
        const self: *ActivityForwarder = @ptrCast(@alignCast(ctx));
        self.cp.activity(self.alloc, self.runner_token, self.lease_id, &.{frame});
    }
};

/// Execute one leased event in a sandboxed child and report the result to the
/// control plane, forwarding live-tail activity frames as the child streams them.
fn executeAndReport(
    alloc: std.mem.Allocator,
    cp: client_mod,
    runner_token: []const u8,
    cfg: daemon_config.Config,
    payload: protocol.LeasePayload,
) void {
    log.info("lease_acquired", .{
        .lease_id = payload.lease_id,
        .event_id = payload.event.event_id,
    });

    var ws_buf: [std.fs.max_path_bytes]u8 = undefined;
    const workspace_path = prepareWorkspace(&ws_buf, cfg.workspace_base, payload.lease_id) orelse return;
    defer cleanupWorkspace(workspace_path);

    var forwarder = ActivityForwarder{ .alloc = alloc, .cp = cp, .runner_token = runner_token, .lease_id = payload.lease_id };
    const sink = child_supervisor.ActivitySink{ .ctx = &forwarder, .forward = ActivityForwarder.forward };

    const start_ms = std.time.milliTimestamp();
    const result = child_supervisor.run(alloc, cfg, workspace_path, payload, sink);
    const wall_ms: u64 = @intCast(@max(0, std.time.milliTimestamp() - start_ms));
    defer if (result.content.len > 0) alloc.free(result.content);

    log.info("execute_done", .{ .lease_id = payload.lease_id, .exit_ok = result.exit_ok, .wall_ms = wall_ms });

    const outcome: protocol.Outcome = if (result.exit_ok) .processed else .agent_error;
    cp.report(alloc, runner_token, protocol.ReportRequest{
        .lease_id = payload.lease_id,
        .event_id = payload.event.event_id,
        .fencing_token = payload.fencing_token,
        .outcome = outcome,
        .response_text = result.content,
        .tokens = result.token_count,
        .telemetry = .{ .time_to_first_token_ms = 0, .wall_ms = wall_ms },
        .checkpoint = .{ .last_event_id = payload.event.event_id, .last_response = result.content },
    }) catch |err| {
        log.err("report_failed", .{ .lease_id = payload.lease_id, .err = @errorName(err) });
        return;
    };

    log.info("reported", .{ .lease_id = payload.lease_id, .outcome = @tagName(outcome) });
}

/// Create a per-lease workspace directory. Writes into caller-owned `buf`; returns
/// a slice into `buf` (valid for caller's stack frame) or null on error.
fn prepareWorkspace(buf: *[std.fs.max_path_bytes]u8, base: []const u8, lease_id: []const u8) ?[]const u8 {
    const path = std.fmt.bufPrint(buf, "{s}/{s}", .{ base, lease_id }) catch {
        log.err("workspace_path_fmt_failed", .{ .lease_id = lease_id });
        return null;
    };
    std.fs.makeDirAbsolute(path) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => { log.err("workspace_mkdir_failed", .{ .path = path, .err = @errorName(err) }); return null; },
    };
    return path;
}

/// Delete the per-lease workspace directory tree; failure is logged and ignored.
fn cleanupWorkspace(path: []const u8) void {
    std.fs.deleteTreeAbsolute(path) catch |err| {
        log.warn("workspace_cleanup_failed", .{ .path = path, .err = @errorName(err) });
    };
}

/// Parse sandbox tier from env string; defaults to `.dev_none` for unknown
/// values. Single-sourced off the enum (RULE UFS) — no re-spelled tier literals.
fn sandboxTierFromStr(s: []const u8) protocol.SandboxTier {
    return std.meta.stringToEnum(protocol.SandboxTier, s) orelse .dev_none;
}

/// Sleep for `ms` milliseconds.
fn sleepMs(ms: u64) void {
    std.Thread.sleep(ms * std.time.ns_per_ms);
}

// ── Test aggregator ─────────────────────────────────────────────────────────
// `zig build --build-file build_runner.zig test` — daemon/ + engine/, no pg/redis.
test {
    _ = @import("daemon/control_plane_client.zig");
    _ = @import("daemon/config.zig");
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
