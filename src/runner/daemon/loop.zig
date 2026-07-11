//! The host-resident runner's parent event-leasing loop and graceful-drain
//! signal handling. Boots from the operator-installed `agt_r` (Option B, no
//! self-register): `runLoop` goes straight to heartbeat → lease → execute →
//! report → activity. Transport errors back off without crashing; un-acked
//! leases re-deliver via reclaim. Each lease runs in a forked, sandboxed child
//! that streams live-tail `activity` frames, which the parent forwards on.

const std = @import("std");
const common = @import("common");
const logging = @import("log");
const contract = @import("contract");
const constants = common;

const Config = @import("config.zig");
const client_mod = @import("control_plane_client.zig");
const client_errors = @import("../engine/client_errors.zig");
const worker_pool = @import("worker_pool.zig");
const renew_driver = @import("renew_driver.zig");
const lease_run = @import("lease_run.zig");

const protocol = contract.protocol;
const log = logging.scoped(.fleet_runner);
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;

/// One event for a graceful daemon stop; the `reason` field discriminates the
/// trigger (signal drain, fleet stop, fleet drain). Named per RULE UFS (3 sites).
const EVENT_SERVER_STOPPED = "server_stopped";

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
/// pool's spawn/join. Identity is `cfg.runner_token` (a pre-minted `agt_r`); the
/// loop never registers — its first contact is a heartbeat (Option B).
///
/// The pool is spawned lazily after the first `.ok` heartbeat, so the host's
/// first control-plane contact is always the heartbeat and a boot-time `.stop`
/// exits before a single lease is taken. Workers each run `pollAndProcess`
/// concurrently; `cfg.worker_count == 1` is behaviourally today's single daemon.
pub fn runLoop(io: std.Io, alloc: std.mem.Allocator, cfg: Config, env_map: *const std.process.Environ.Map) void {
    var cp = client_mod.init(alloc, io, cfg.control_plane_url);
    defer cp.deinit();
    const runner_token: []const u8 = cfg.runner_token;
    // Reset only `stop_requested` (set solely by this control loop). `drain_requested`
    // is set by the async SIGTERM/SIGINT handler and is DELIBERATELY not reset here:
    // a SIGTERM landing in the window between `installDrainHandlers` and this point
    // must NOT be dropped, or the daemon would ignore `systemctl stop` until SIGKILL.
    stop_requested.store(false, .seq_cst);

    var pool: ?worker_pool.Pool = null;
    // On any exit the workers see stop/drain (set below or by the signal handler),
    // finish their in-flight child, and are joined — no thread/child leak. A
    // per-worker leak verdict is already logged at `err` inside join; the daemon
    // is on its shutdown path, so we record the swallow and let exit proceed.
    defer if (pool) |p| p.join() catch |err|
        log.warn(logging.EVENT_IGNORED_ERROR, .{ .op = "worker_pool_join", .err = @errorName(err) });

    var heartbeat_errors: u32 = 0;
    while (true) {
        if (drain_requested.load(.seq_cst)) {
            log.info(EVENT_SERVER_STOPPED, .{ .reason = "signal_drain" });
            break;
        }

        const hb = cp.heartbeat(alloc, runner_token, cfg.cp_deadlines.default_ms) catch |err| {
            heartbeat_errors += 1;
            log.warn("heartbeat_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err), .consecutive = heartbeat_errors });
            // Bounded+jittered backoff (M100): exponential in the consecutive
            // error count, capped at MAX_BACKOFF_MS — never the old unbounded
            // `2s * heartbeat_errors` ramp. attempt is 0-based (first error → ~base).
            sleepMs(io, constants.backoff.ms(heartbeat_errors - 1));
            continue;
        };
        heartbeat_errors = 0;

        switch (hb.status) {
            .stop => {
                log.info(EVENT_SERVER_STOPPED, .{ .reason = "fleet_stop" });
                stop_requested.store(true, .seq_cst);
                break;
            },
            .drain => {
                log.info(EVENT_SERVER_STOPPED, .{ .reason = "fleet_drain" });
                drain_requested.store(true, .seq_cst);
                break;
            },
            .ok => {},
        }

        // First OK heartbeat brings the pool up; later ones are liveness ticks.
        if (pool == null) {
            pool = worker_pool.spawn(io, alloc, cfg, env_map, &stop_requested, &drain_requested) catch |err| {
                log.err("worker_pool_spawn_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err) });
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
pub fn pollAndProcess(io: std.Io, alloc: std.mem.Allocator, cp: *client_mod, runner_token: []const u8, cfg: Config, env_map: *const std.process.Environ.Map) void {
    const lease_parsed = cp.lease(alloc, runner_token, cfg.cp_deadlines.default_ms) catch |err| {
        log.warn("lease_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
        sleepMs(io, constants.backoff.ms(0));
        return;
    };
    defer lease_parsed.deinit();

    const lease_resp = lease_parsed.value;
    if (lease_resp.lease == null) {
        const wait_ms: u64 = lease_resp.retry_after_ms orelse constants.NO_WORK_RETRY_AFTER_MS;
        log.debug("lease_poll_empty", .{ .retry_after_ms = wait_ms });
        sleepMs(io, wait_ms);
        return;
    }

    lease_run.executeAndReport(io, alloc, cp, runner_token, cfg, env_map, lease_resp.lease.?);
}

/// Map a child's clean-exit flag to the reported outcome. A failed execution
/// (incl. a fail-closed sandbox setup) is reported as `fleet_error`.
pub fn outcomeFor(exit_ok: bool) protocol.Outcome {
    return if (exit_ok) .processed else .fleet_error;
}

/// Saturate the final ExecutionResult's u64 cumulative splits onto the report's
/// wire-frozen u32 fields. Returns the explicit `TokenSplits` carrier (not the
/// renew HTTP-body type) so the report path never borrows the renew contract as
/// a value bag; one wire-width policy lives in `renew_driver.wireSplits` (RULE
/// NDC). The report fills its three fields from this beside the unchanged legacy
/// `tokens` total.
pub fn splitFields(result: contract.execution_result.ExecutionResult) renew_driver.TokenSplits {
    return renew_driver.wireSplits(result.input_tokens, result.cached_input_tokens, result.output_tokens);
}

/// Sleep for `ms` milliseconds.
fn sleepMs(io: std.Io, ms: u64) void {
    io.sleep(std.Io.Duration.fromMilliseconds(@intCast(ms)), .awake) catch return;
}
