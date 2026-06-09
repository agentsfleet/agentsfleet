//! The host-resident runner's parent event-leasing loop and graceful-drain
//! signal handling. Boots from the operator-installed `zrn_` (Option B, no
//! self-register): `runLoop` goes straight to heartbeat → lease → execute →
//! report → activity. Transport errors back off without crashing; un-acked
//! leases re-deliver via reclaim. Each lease runs in a forked, sandboxed child
//! that streams live-tail `activity` frames, which the parent forwards on.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const logging = @import("log");
const contract = @import("contract");
const constants = common;

const Config = @import("config.zig");
const client_mod = @import("control_plane_client.zig");
const child_supervisor = @import("../child_supervisor.zig");
const worker_pool = @import("worker_pool.zig");
const RenewDriver = @import("renew_driver.zig").RenewDriver(client_mod);

const protocol = contract.protocol;
const log = logging.scoped(.zombie_runner);

/// Backoff (ms) on control-plane transport errors; lease polls use server-supplied retry_after_ms.
const TRANSPORT_ERROR_BACKOFF_MS: u64 = 2_000;
/// Consecutive heartbeat errors before escalating the sleep multiplier.
const HEARTBEAT_MAX_CONSECUTIVE_ERRORS: u32 = 5;

/// Set by the SIGTERM/SIGINT handler to request a graceful drain. The handler
/// does nothing but this atomic store (async-signal-safe); the loop reads it at
/// its boundary, finishes the in-flight lease, then exits.
pub var drain_requested = std.atomic.Value(bool).init(false);

/// SIGTERM/SIGINT → request graceful drain. Async-signal-safe: a lone atomic
/// store, nothing else. `systemctl stop` sends SIGTERM; the loop honors it at its
/// next boundary. The in-flight child is never interrupted — poll/read/waitpid in
/// the execute path all retry EINTR — so the leased NullClaw runs to completion
/// before the runner exits.
pub fn requestDrain(_: std.posix.SIG) callconv(.c) void {
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

/// Set by the control loop when the fleet returns a `.stop` heartbeat. Distinct
/// from `drain_requested` (signal / fleet `.drain`) only by origin; each worker
/// halts on either at its between-lease boundary, so both are graceful drains
/// (finish in-flight, take no new lease) per the locked design.
pub var stop_requested = std.atomic.Value(bool).init(false);

/// Control loop: the host's single thread heartbeats once per host on the
/// `HEARTBEAT_INTERVAL_MS` cadence, maps a `.stop`/`.drain` directive (and the
/// signal-set `drain_requested`) onto the shared atomics, and owns the worker
/// pool's spawn/join. Identity is `cfg.runner_token` (a pre-minted `zrn_`); the
/// loop never registers — its first contact is a heartbeat (Option B).
///
/// The pool is spawned lazily after the first `.ok` heartbeat, so the host's
/// first control-plane contact is always the heartbeat and a boot-time `.stop`
/// exits before a single lease is taken. Workers each run `pollAndProcess`
/// concurrently; `cfg.worker_count == 1` is behaviourally today's single daemon.
pub fn runLoop(io: std.Io, alloc: std.mem.Allocator, cfg: Config, env_map: *const std.process.Environ.Map) void {
    const cp = client_mod{ .base_url = cfg.control_plane_url, .io = io };
    const runner_token: []const u8 = cfg.runner_token;
    // Reset only `stop_requested` (set solely by this control loop). `drain_requested`
    // is set by the async SIGTERM/SIGINT handler and is DELIBERATELY not reset here:
    // a SIGTERM landing in the window between `installDrainHandlers` and this point
    // must NOT be dropped, or the daemon would ignore `systemctl stop` until SIGKILL.
    stop_requested.store(false, .seq_cst);

    var pool: ?worker_pool.Pool = null;
    // On any exit the workers see stop/drain (set below or by the signal handler),
    // finish their in-flight child, and are joined — no thread/child leak.
    defer if (pool) |p| p.join();

    var heartbeat_errors: u32 = 0;
    while (true) {
        if (drain_requested.load(.seq_cst)) {
            log.info("signal_drain", .{});
            break;
        }

        const hb = cp.heartbeat(alloc, runner_token) catch |err| {
            heartbeat_errors += 1;
            log.warn("heartbeat_failed", .{ .err = @errorName(err), .consecutive = heartbeat_errors });
            const mult: u64 = if (heartbeat_errors >= HEARTBEAT_MAX_CONSECUTIVE_ERRORS) heartbeat_errors else 1;
            sleepMs(io, TRANSPORT_ERROR_BACKOFF_MS * mult);
            continue;
        };
        heartbeat_errors = 0;

        switch (hb.status) {
            .stop => {
                log.info("fleet_stop", .{});
                stop_requested.store(true, .seq_cst);
                break;
            },
            .drain => {
                log.info("fleet_drain", .{});
                drain_requested.store(true, .seq_cst);
                break;
            },
            .ok => {},
        }

        // First OK heartbeat brings the pool up; later ones are liveness ticks.
        if (pool == null) {
            pool = worker_pool.spawn(io, alloc, cfg, env_map, &stop_requested, &drain_requested) catch |err| {
                log.err("worker_pool_spawn_failed", .{ .err = @errorName(err) });
                break;
            };
        }

        sleepMs(io, @intCast(constants.HEARTBEAT_INTERVAL_MS));
    }
}

/// Long-poll one lease; execute + report it when present, else back off the
/// server-supplied (or default) retry interval. Errors back off and return — the
/// caller's loop retries on the next iteration. Each pool worker calls this in a
/// loop with its own allocator + client (see `worker_pool.zig`).
pub fn pollAndProcess(io: std.Io, alloc: std.mem.Allocator, cp: client_mod, runner_token: []const u8, cfg: Config, env_map: *const std.process.Environ.Map) void {
    const lease_parsed = cp.lease(alloc, runner_token) catch |err| {
        log.warn("lease_failed", .{ .err = @errorName(err) });
        sleepMs(io, TRANSPORT_ERROR_BACKOFF_MS);
        return;
    };
    defer lease_parsed.deinit();

    const lease_resp = lease_parsed.value;
    if (lease_resp.lease == null) {
        const wait_ms: u64 = lease_resp.retry_after_ms orelse constants.NO_WORK_RETRY_AFTER_MS;
        log.info("no_work", .{ .retry_after_ms = wait_ms });
        sleepMs(io, wait_ms);
        return;
    }

    executeAndReport(io, alloc, cp, runner_token, cfg, env_map, lease_resp.lease.?);
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

/// POSTs each `.memory` capture frame the child writes to the control plane —
/// the daemon (not the child) holds the `zrn_` token, so capture rides the
/// trusted plane. The frame is a JSON array of deltas; the daemon wraps it with
/// the held lease's `lease_id` + `fencing_token` so the write is fenced. A blip
/// is logged and swallowed — the next capture re-sends the full set.
const MemoryForwarder = struct {
    alloc: std.mem.Allocator,
    cp: client_mod,
    runner_token: []const u8,
    zombie_id: []const u8,
    lease_id: []const u8,
    fencing_token: u64,

    fn forward(ctx: *anyopaque, payload: []const u8) void {
        const self: *MemoryForwarder = @ptrCast(@alignCast(ctx));
        const parsed = std.json.parseFromSlice([]protocol.MemoryDelta, self.alloc, payload, .{}) catch {
            log.warn("memory_frame_parse_failed", .{ .zombie_id = self.zombie_id });
            return;
        };
        defer parsed.deinit();
        const req = protocol.MemoryPushRequest{
            .lease_id = self.lease_id,
            .fencing_token = self.fencing_token,
            .memory = parsed.value,
        };
        self.cp.memoryCapture(self.alloc, self.runner_token, self.zombie_id, req) catch |err|
            log.warn("memory_capture_post_failed", .{ .zombie_id = self.zombie_id, .err = @errorName(err) });
    }
};

/// Execute one leased event in a sandboxed child and report the result to the
/// control plane, forwarding live-tail activity frames as the child streams them.
fn executeAndReport(
    io: std.Io,
    alloc: std.mem.Allocator,
    cp: client_mod,
    runner_token: []const u8,
    cfg: Config,
    env_map: *const std.process.Environ.Map,
    payload: protocol.LeasePayload,
) void {
    log.info("lease_acquired", .{
        .lease_id = payload.lease_id,
        .event_id = payload.event.event_id,
    });

    var ws_buf: [std.fs.max_path_bytes]u8 = undefined;
    // Back off on a workspace-prep failure before returning: otherwise a
    // persistent failure (e.g. an unwritable workspace base) hot-spins the
    // worker's poll loop — amplified ×worker_count under the pool.
    const workspace_path = prepareWorkspace(io, &ws_buf, cfg.workspace_base, payload.lease_id) orelse {
        sleepMs(io, TRANSPORT_ERROR_BACKOFF_MS);
        return;
    };
    defer cleanupWorkspace(io, workspace_path);

    var forwarder = ActivityForwarder{ .alloc = alloc, .cp = cp, .runner_token = runner_token, .lease_id = payload.lease_id };
    const sink = child_supervisor.ActivitySink{ .ctx = &forwarder, .forward = ActivityForwarder.forward };
    var driver = RenewDriver.init(alloc, cp, runner_token, payload);

    // Hydrate the zombie's prior memory over the trusted plane BEFORE the fork so
    // the child seeds its in-run store from it — the child makes no network call
    // and holds no token. A hydrate miss degrades to empty memory, never blocks.
    const hydrated = cp.memoryHydrate(alloc, runner_token, payload.event.zombie_id) catch |err| blk: {
        log.warn("memory_hydrate_failed", .{ .zombie_id = payload.event.zombie_id, .err = @errorName(err) });
        break :blk null;
    };
    defer if (hydrated) |h| h.deinit();
    const hydrated_memory: []const protocol.MemoryDelta = if (hydrated) |h| h.value.memory else &.{};

    var mem_forwarder = MemoryForwarder{
        .alloc = alloc,
        .cp = cp,
        .runner_token = runner_token,
        .zombie_id = payload.event.zombie_id,
        .lease_id = payload.lease_id,
        .fencing_token = payload.fencing_token,
    };
    const mem_sink = child_supervisor.MemorySink{ .ctx = &mem_forwarder, .forward = MemoryForwarder.forward };

    const start_ms = clock.nowMillis();
    const result = child_supervisor.run(io, alloc, cfg, env_map, workspace_path, payload, hydrated_memory, sink, mem_sink, driver.hook());
    const wall_ms: u64 = @intCast(@max(0, clock.nowMillis() - start_ms));
    defer if (result.content.len > 0) alloc.free(result.content);

    log.info("execute_done", .{ .lease_id = payload.lease_id, .exit_ok = result.exit_ok, .wall_ms = wall_ms });

    const outcome = outcomeFor(result.exit_ok);
    cp.report(alloc, runner_token, protocol.ReportRequest{
        .lease_id = payload.lease_id,
        .event_id = payload.event.event_id,
        .fencing_token = payload.fencing_token,
        .outcome = outcome,
        .failure_reason = result.failure,
        .response_text = result.content,
        .tokens = result.token_count,
        .telemetry = .{ .time_to_first_token_ms = 0, .wall_ms = wall_ms },
        .checkpoint = .{ .last_event_id = payload.event.event_id, .last_response = result.content },
    }) catch |err| {
        log.err("report_failed", .{ .lease_id = payload.lease_id, .err = @errorName(err) });
        sleepMs(io, TRANSPORT_ERROR_BACKOFF_MS); // back off so a down report endpoint can't hot-spin the pool
        return;
    };

    log.info("reported", .{ .lease_id = payload.lease_id, .outcome = @tagName(outcome) });
}

/// Create a per-lease workspace directory. Writes into caller-owned `buf`; returns
/// a slice into `buf` (valid for caller's stack frame) or null on error.
fn prepareWorkspace(io: std.Io, buf: *[std.fs.max_path_bytes]u8, base: []const u8, lease_id: []const u8) ?[]const u8 {
    const path = std.fmt.bufPrint(buf, "{s}/{s}", .{ base, lease_id }) catch {
        log.err("workspace_path_fmt_failed", .{ .lease_id = lease_id });
        return null;
    };
    std.Io.Dir.createDirAbsolute(io, path, .default_dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => {
            log.err("workspace_mkdir_failed", .{ .path = path, .err = @errorName(err) });
            return null;
        },
    };
    return path;
}

/// Delete the per-lease workspace directory tree; failure is logged and ignored.
fn cleanupWorkspace(io: std.Io, path: []const u8) void {
    std.Io.Dir.cwd().deleteTree(io, path) catch |err| {
        log.warn("workspace_cleanup_failed", .{ .path = path, .err = @errorName(err) });
    };
}

/// Map a child's clean-exit flag to the reported outcome. A failed execution
/// (incl. a fail-closed sandbox setup) is reported as `agent_error`.
pub fn outcomeFor(exit_ok: bool) protocol.Outcome {
    return if (exit_ok) .processed else .agent_error;
}

/// Sleep for `ms` milliseconds.
fn sleepMs(io: std.Io, ms: u64) void {
    io.sleep(std.Io.Duration.fromMilliseconds(@intCast(ms)), .awake) catch return;
}
