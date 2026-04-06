//! Post-iteration limit checks for the gate loop — extracted from
//! worker_gate_loop.zig for the 500-line limit (M21_002).

const std = @import("std");
const pg = @import("pg");
const metrics = @import("../observability/metrics.zig");
const codes = @import("../errors/codes.zig");
const queue_redis = @import("../queue/redis.zig");
const queue_consts = @import("../queue/constants.zig");
const state_machine = @import("../state/machine.zig");
const executor_client = @import("../executor/client.zig");
const gate_loop = @import("worker_gate_loop.zig");

const log = std.log.scoped(.gate_loop);

/// Check token budget: query usage_ledger for sum of tokens used this run.
/// Returns true if the limit is breached (transition written, metric incremented).
pub fn checkTokenBudget(cfg: gate_loop.GateLoopConfig) !bool {
    if (cfg.max_tokens == 0) return false;
    var q = try cfg.conn.query(
        \\SELECT COALESCE(SUM(token_count), 0) FROM billing.usage_ledger
        \\WHERE run_id = $1 AND attempt = $2
    , .{ cfg.run_id, @as(i32, @intCast(cfg.attempt)) });
    defer q.deinit();
    const row = (try q.next()) orelse return false;
    const used: i64 = row.get(i64, 0) catch 0;
    try q.drain();
    if (@as(u64, @intCast(@max(0, used))) < cfg.max_tokens) return false;
    log.warn("gate_loop.token_budget_exceeded error_code={s} run_id={s} used={d} max={d}", .{
        codes.ERR_RUN_TOKEN_BUDGET_EXCEEDED, cfg.run_id, used, cfg.max_tokens,
    });
    _ = state_machine.transition(cfg.conn, cfg.run_id, .CANCELLED, .orchestrator, .TOKEN_BUDGET_EXCEEDED, "token budget exceeded during gate loop") catch |err| {
        log.warn("gate_loop.token_budget_transition_fail err={s} run_id={s}", .{ @errorName(err), cfg.run_id });
    };
    metrics.incRunLimitTokenBudgetExceeded();
    return true;
}

/// Check wall-time limit. Returns true if breached (transition written).
pub fn checkWallTime(cfg: gate_loop.GateLoopConfig) !bool {
    if (cfg.max_wall_time_seconds == 0 or cfg.run_created_at_ms == 0) return false;
    const elapsed_ms = std.time.milliTimestamp() - cfg.run_created_at_ms;
    if (elapsed_ms < 0) return false;
    const elapsed_s: u64 = @intCast(@divTrunc(elapsed_ms, 1000));
    if (elapsed_s < cfg.max_wall_time_seconds) return false;
    log.warn("gate_loop.wall_time_exceeded error_code={s} run_id={s} elapsed_s={d} max_s={d}", .{
        codes.ERR_RUN_WALL_TIME_EXCEEDED, cfg.run_id, elapsed_s, cfg.max_wall_time_seconds,
    });
    _ = state_machine.transition(cfg.conn, cfg.run_id, .CANCELLED, .orchestrator, .WALL_TIME_EXCEEDED, "wall-time limit exceeded during gate loop") catch |err| {
        log.warn("gate_loop.wall_time_transition_fail err={s} run_id={s}", .{ @errorName(err), cfg.run_id });
    };
    metrics.incRunLimitWallTimeExceeded();
    return true;
}

/// Check Redis cancellation signal. Returns true if signal found (transition written).
pub fn checkCancelSignal(cfg: gate_loop.GateLoopConfig) !bool {
    const redis = cfg.redis orelse return false;
    const key = try std.fmt.allocPrint(cfg.alloc, queue_consts.cancel_key_prefix ++ "{s}", .{cfg.run_id});
    defer cfg.alloc.free(key);
    const found = redis.exists(key) catch false;
    if (!found) return false;
    log.info("gate_loop.cancel_signal run_id={s}", .{cfg.run_id});
    if (cfg.executor) |exec| {
        if (cfg.execution_id) |exec_id| {
            exec.destroyExecution(exec_id) catch |err| {
                log.warn("gate_loop.destroy_execution_fail err={s} run_id={s}", .{ @errorName(err), cfg.run_id });
            };
        }
    }
    _ = state_machine.transition(cfg.conn, cfg.run_id, .CANCELLED, .orchestrator, .RUN_CANCELLED, "operator cancel signal received") catch |err| {
        log.warn("gate_loop.cancel_transition_fail err={s} run_id={s}", .{ @errorName(err), cfg.run_id });
    };
    return true;
}

/// After each gate loop iteration: check cancel signal, token budget, wall time.
/// Returns true if any limit was breached and `state_written` should be set.
pub fn checkPostIterationLimits(cfg: gate_loop.GateLoopConfig) !bool {
    if (try checkCancelSignal(cfg)) return true;
    if (try checkTokenBudget(cfg)) return true;
    if (try checkWallTime(cfg)) return true;
    return false;
}
