//! The host-resident runner's parent event-leasing loop and graceful-drain
//! signal handling. Boots from the operator-installed `zrn_` (Option B, no
//! self-register): `runLoop` goes straight to heartbeat → lease → execute →
//! report → activity. Transport errors back off without crashing; un-acked
//! leases re-deliver via reclaim. Each lease runs in a forked, sandboxed child
//! that streams live-tail `activity` frames, which the parent forwards on.

const std = @import("std");
const logging = @import("log");
const contract = @import("contract");
const constants = @import("common");

const Config = @import("config.zig");
const client_mod = @import("control_plane_client.zig");
const child_supervisor = @import("../child_supervisor.zig");

const protocol = contract.protocol;
const log = logging.scoped(.zombie_runner);

/// Backoff (ms) on control-plane transport errors; lease polls use server-supplied retry_after_ms.
const TRANSPORT_ERROR_BACKOFF_MS: u64 = 2_000;
/// Consecutive heartbeat errors before escalating the sleep multiplier.
const HEARTBEAT_MAX_CONSECUTIVE_ERRORS: u32 = 5;

/// Set by the SIGTERM/SIGINT handler to request a graceful drain. The handler
/// does nothing but this atomic store (async-signal-safe); the loop reads it at
/// its boundary, finishes the in-flight lease, then exits.
var drain_requested = std.atomic.Value(bool).init(false);

/// SIGTERM/SIGINT → request graceful drain. Async-signal-safe: a lone atomic
/// store, nothing else. `systemctl stop` sends SIGTERM; the loop honors it at its
/// next boundary. The in-flight child is never interrupted — poll/read/waitpid in
/// the execute path all retry EINTR — so the leased NullClaw runs to completion
/// before the runner exits.
fn requestDrain(_: i32) callconv(.c) void {
    drain_requested.store(true, .seq_cst);
}

/// Install the drain signal handlers (mirrors the daemon shutdown idiom).
pub fn installDrainHandlers() void {
    const action = std.posix.Sigaction{
        .handler = .{ .handler = requestDrain },
        .mask = std.posix.sigemptyset(),
        .flags = 0,
    };
    std.posix.sigaction(std.posix.SIG.TERM, &action, null);
    std.posix.sigaction(std.posix.SIG.INT, &action, null);
}

/// Heartbeat → lease → execute → report loop. Returns on stop or drain signal.
/// Identity is `cfg.runner_token` (a pre-minted `zrn_`); the loop never
/// registers — its first contact is a heartbeat.
pub fn runLoop(alloc: std.mem.Allocator, cfg: Config) void {
    const cp = client_mod{ .base_url = cfg.control_plane_url };
    const runner_token: []const u8 = cfg.runner_token;
    var draining = false;
    var heartbeat_errors: u32 = 0;

    outer: while (true) {
        if (drain_requested.load(.seq_cst) and !draining) {
            log.info("signal_drain", .{});
            draining = true;
        }
        if (draining) {
            log.info("drain_idle_exit", .{});
            break :outer;
        }

        const hb = cp.heartbeat(alloc, runner_token) catch |err| {
            heartbeat_errors += 1;
            log.warn("heartbeat_failed", .{ .err = @errorName(err), .consecutive = heartbeat_errors });
            const mult: u64 = if (heartbeat_errors >= HEARTBEAT_MAX_CONSECUTIVE_ERRORS) heartbeat_errors else 1;
            sleepMs(TRANSPORT_ERROR_BACKOFF_MS * mult);
            continue :outer;
        };
        heartbeat_errors = 0;

        switch (hb.status) {
            .stop => {
                log.info("fleet_stop", .{});
                break :outer;
            },
            .drain => {
                if (!draining) log.info("fleet_drain", .{});
                draining = true;
                continue :outer;
            },
            .ok => {},
        }

        pollAndProcess(alloc, cp, runner_token, cfg);
    }
}

/// Long-poll one lease; execute + report it when present, else back off the
/// server-supplied (or default) retry interval. Errors back off and return — the
/// caller's loop retries on the next iteration.
fn pollAndProcess(alloc: std.mem.Allocator, cp: client_mod, runner_token: []const u8, cfg: Config) void {
    const lease_parsed = cp.lease(alloc, runner_token) catch |err| {
        log.warn("lease_failed", .{ .err = @errorName(err) });
        sleepMs(TRANSPORT_ERROR_BACKOFF_MS);
        return;
    };
    defer lease_parsed.deinit();

    const lease_resp = lease_parsed.value;
    if (lease_resp.lease == null) {
        const wait_ms: u64 = lease_resp.retry_after_ms orelse constants.NO_WORK_RETRY_AFTER_MS;
        log.info("no_work", .{ .retry_after_ms = wait_ms });
        sleepMs(wait_ms);
        return;
    }

    executeAndReport(alloc, cp, runner_token, cfg, lease_resp.lease.?);
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
    cfg: Config,
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

    const outcome = outcomeFor(result.exit_ok);
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
        else => {
            log.err("workspace_mkdir_failed", .{ .path = path, .err = @errorName(err) });
            return null;
        },
    };
    return path;
}

/// Delete the per-lease workspace directory tree; failure is logged and ignored.
fn cleanupWorkspace(path: []const u8) void {
    std.fs.deleteTreeAbsolute(path) catch |err| {
        log.warn("workspace_cleanup_failed", .{ .path = path, .err = @errorName(err) });
    };
}

/// Map a child's clean-exit flag to the reported outcome. A failed execution
/// (incl. a fail-closed sandbox setup) is reported as `agent_error`.
fn outcomeFor(exit_ok: bool) protocol.Outcome {
    return if (exit_ok) .processed else .agent_error;
}

/// Sleep for `ms` milliseconds.
fn sleepMs(ms: u64) void {
    std.Thread.sleep(ms * std.time.ns_per_ms);
}

// ── Tests ────────────────────────────────────────────────────────────────────

// Records the first request line a one-shot loopback control plane observes, so
// the boot test can prove the daemon's first contact is a heartbeat (lease-loop
// entry), never a register call.
const BootProbe = struct {
    // SAFETY: written by serveOneStopHeartbeat before line_len is set; only
    // line_buf[0..line_len] is ever read.
    line_buf: [256]u8 = undefined,
    line_len: usize = 0,
};

// Accept one connection, capture its request line, reply `stop` so `runLoop`
// exits after a single heartbeat. The `stop` body must parse cleanly or the loop
// would back off and retry — hence a well-formed fixed HTTP/1.1 response.
fn serveOneStopHeartbeat(listener: *std.net.Server, probe: *BootProbe) void {
    const conn = listener.accept() catch return;
    defer conn.stream.close();

    var buf: [1024]u8 = undefined;
    var total: usize = 0;
    while (total < buf.len) {
        const n = conn.stream.read(buf[total..]) catch break;
        if (n == 0) break;
        total += n;
        if (std.mem.indexOf(u8, buf[0..total], "\r\n\r\n") != null) break;
    }
    const line_end = std.mem.indexOf(u8, buf[0..total], "\r\n") orelse total;
    probe.line_len = @min(line_end, probe.line_buf.len);
    @memcpy(probe.line_buf[0..probe.line_len], buf[0..probe.line_len]);

    conn.stream.writeAll(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n" ++
            "Content-Length: 17\r\nConnection: close\r\n\r\n{\"status\":\"stop\"}",
    ) catch {};
}

test "runner boots from a zrn_ token straight into the lease loop with no register call" {
    const alloc = std.testing.allocator;
    drain_requested.store(false, .seq_cst);

    const addr = try std.net.Address.parseIp4("127.0.0.1", 0);
    var listener = try addr.listen(.{ .reuse_address = true });
    defer listener.deinit();
    const port = listener.listen_address.getPort();

    var probe: BootProbe = .{};
    var server_thread = try std.Thread.spawn(.{}, serveOneStopHeartbeat, .{ &listener, &probe });

    const url = try std.fmt.allocPrint(alloc, "http://127.0.0.1:{d}", .{port});
    defer alloc.free(url);
    // Identity is the pre-minted zrn_ — Config is built directly here; the
    // env → Config parse (incl. the zrn_ prefix gate) is covered in config.zig.
    const cfg = Config{
        .control_plane_url = try alloc.dupe(u8, url),
        .runner_token = try alloc.dupe(u8, "zrn_" ++ "a" ** 64),
        .host_id = try alloc.dupe(u8, "boot-test-host"),
        .sandbox_tier = try alloc.dupe(u8, "dev_none"),
        .workspace_base = try alloc.dupe(u8, "/tmp/zombie-runner-boot-test"),
        .alloc = alloc,
    };
    defer cfg.deinit();

    runLoop(alloc, cfg); // returns on the `stop` heartbeat
    server_thread.join();

    // First (and only) control-plane contact is the heartbeat — not register.
    const observed = probe.line_buf[0..probe.line_len];
    const expected = "POST " ++ protocol.PATH_RUNNER_HEARTBEATS ++ " ";
    try std.testing.expect(std.mem.startsWith(u8, observed, expected));
    // The enrollment route is never touched on boot (Option B).
    try std.testing.expect(std.mem.indexOf(u8, observed, "POST " ++ protocol.PATH_RUNNERS ++ " ") == null);
}

test "drain signal handler requests a graceful drain" {
    defer drain_requested.store(false, .seq_cst);
    drain_requested.store(false, .seq_cst);
    try std.testing.expect(!drain_requested.load(.seq_cst));
    requestDrain(std.posix.SIG.TERM);
    try std.testing.expect(drain_requested.load(.seq_cst));
}

test "a failed execution reports agent_error; a clean one reports processed" {
    try std.testing.expectEqual(protocol.Outcome.agent_error, outcomeFor(false)); // the startup_posture path
    try std.testing.expectEqual(protocol.Outcome.processed, outcomeFor(true));
}
