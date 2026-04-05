//! Gate tool execution loop (M16_001).
//!
//! After the agent pipeline completes successfully, this module runs
//! deterministic gate commands (make lint, make test, make build) in the
//! worktree. On failure it invokes a repair turn via the executor and
//! retries, bounded by max_repair_loops.

const std = @import("std");
const pg = @import("pg");
const git = @import("../git/ops.zig");
const topology = @import("topology.zig");
const executor_client = @import("../executor/client.zig");
const metrics = @import("../observability/metrics.zig");
const id_format = @import("../types/id_format.zig");
const codes = @import("../errors/codes.zig");
const worker_runtime = @import("worker_runtime.zig");
const queue_redis = @import("../queue/redis.zig");
const queue_consts = @import("../queue/constants.zig");
const state_machine = @import("../state/machine.zig");
const types = @import("../types.zig");

const log = std.log.scoped(.gate_loop);

/// Maximum bytes captured per stdout/stderr stream.
pub const MAX_OUTPUT_BYTES: usize = 4096;

pub const GateToolResult = struct {
    gate_name: []const u8,
    exit_code: u32,
    stdout: []const u8, // owned when from executeGateCommand
    stderr: []const u8, // owned when from executeGateCommand
    wall_ms: u64,
    passed: bool,
    owned: bool = false,

    /// Free owned stdout/stderr. No-op for borrowed (test-constructed) results.
    pub fn deinit(self: GateToolResult, alloc: std.mem.Allocator) void {
        if (self.owned) {
            if (self.stdout.len > 0) alloc.free(self.stdout);
            if (self.stderr.len > 0) alloc.free(self.stderr);
        }
    }
};

pub const GateLoopOutcome = struct {
    all_passed: bool,
    results: std.ArrayList(GateToolResult),
    total_repair_loops: u32,
    exhausted: bool,
    /// True when the gate loop itself wrote the terminal state transition
    /// (limit exceeded or cancelled). Caller must skip outcome handling.
    state_written: bool = false,
};

pub const GateLoopConfig = struct {
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    run_id: []const u8,
    workspace_id: []const u8,
    /// M21_001 A09: agent identity for observability (interrupt + gate logs).
    agent_id: []const u8 = "",
    wt_path: []const u8,
    running: *const std.atomic.Value(bool),
    deadline_ms: i64,
    executor: ?*executor_client.ExecutorClient,
    execution_id: ?[]const u8,
    gate_tools: []const topology.GateTool,
    max_repair_loops: u32,
    gate_tool_timeout_ms: u64,
    repair_stage_id: []const u8,
    repair_role_id: []const u8,
    repair_skill_id: []const u8,
    redis: ?*queue_redis.Client = null,
    // M17_001 §1.2: per-run limits (0 = unlimited)
    max_tokens: u64 = 0,
    max_wall_time_seconds: u64 = 0,
    run_created_at_ms: i64 = 0,
    attempt: u32 = 1,
};

/// Check token budget: query usage_ledger for sum of tokens used this run.
/// Returns true if the limit is breached (transition written, metric incremented).
fn checkTokenBudget(cfg: GateLoopConfig) !bool {
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
fn checkWallTime(cfg: GateLoopConfig) !bool {
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
fn checkCancelSignal(cfg: GateLoopConfig) !bool {
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

/// M21_001 §2.1: Check Redis for a pending interrupt message (GETDEL).
fn checkInterruptSignal(cfg: GateLoopConfig) ?[]const u8 {
    const redis = cfg.redis orelse return null;
    const key = std.fmt.allocPrint(cfg.alloc, queue_consts.interrupt_key_prefix ++ "{s}", .{cfg.run_id}) catch return null;
    defer cfg.alloc.free(key);
    const argv = [_][]const u8{ "GETDEL", key };
    const resp = redis.command(&argv) catch return null;
    return switch (resp) {
        .bulk_string => |s| s,
        else => null,
    };
}

/// After each gate loop iteration: check cancel signal, token budget, wall time.
/// Returns true if any limit was breached and `state_written` should be set.
fn checkPostIterationLimits(cfg: GateLoopConfig) !bool {
    if (try checkCancelSignal(cfg)) return true;
    if (try checkTokenBudget(cfg)) return true;
    if (try checkWallTime(cfg)) return true;
    return false;
}

pub fn runGateLoop(cfg: GateLoopConfig) !GateLoopOutcome {
    var results = std.ArrayList(GateToolResult){};

    var repair: u32 = 0;
    while (repair < cfg.max_repair_loops) : (repair += 1) {
        try worker_runtime.ensureRunActive(cfg.running, cfg.deadline_ms);

        // M21_001 §2.1: poll for pending interrupt before running gates.
        if (checkInterruptSignal(cfg)) |interrupt_msg| {
            log.info("gate_loop.interrupt_received run_id={s} workspace_id={s} agent_id={s} msg_len={d}", .{ cfg.run_id, cfg.workspace_id, cfg.agent_id, interrupt_msg.len });
            if (cfg.executor) |exec| {
                if (cfg.execution_id) |exec_id| {
                    _ = exec.startStage(exec_id, .{
                        .stage_id = cfg.repair_stage_id,
                        .role_id = cfg.repair_role_id,
                        .skill_id = cfg.repair_skill_id,
                        .message = interrupt_msg,
                    }) catch |err| {
                        log.warn("gate_loop.interrupt_inject_fail err={s} run_id={s}", .{ @errorName(err), cfg.run_id });
                    };
                }
            }
        }

        var failed_result: ?GateToolResult = null;
        for (cfg.gate_tools) |gate| {
            const timeout = effectiveTimeout(gate.timeout_ms, cfg.gate_tool_timeout_ms, cfg.deadline_ms);
            const result = try executeGateCommand(cfg.alloc, cfg.wt_path, gate.name, gate.command, timeout);
            try results.append(cfg.alloc, result);

            publishGateEvent(cfg, result, repair);

            if (!result.passed) {
                failed_result = result;
                break;
            }
        }

        // M17_001 §1.2 / §3.2: check limits and cancel signal after each iteration.
        if (try checkPostIterationLimits(cfg)) {
            persistGateResults(cfg.alloc, cfg.conn, cfg.run_id, results.items) catch |err| {
                log.warn("gate_loop.persist_failed error_code={s} err={s} run_id={s}", .{ codes.ERR_GATE_PERSIST_FAILED, @errorName(err), cfg.run_id });
            };
            return .{
                .all_passed = false,
                .results = results,
                .total_repair_loops = repair,
                .exhausted = false,
                .state_written = true,
            };
        }

        if (failed_result == null) {
            // All gates passed.
            persistGateResults(cfg.alloc, cfg.conn, cfg.run_id, results.items) catch |err| {
                log.warn("gate_loop.persist_failed error_code={s} err={s} run_id={s}", .{ codes.ERR_GATE_PERSIST_FAILED, @errorName(err), cfg.run_id });
            };
            return .{
                .all_passed = true,
                .results = results,
                .total_repair_loops = repair,
                .exhausted = false,
            };
        }

        // Gate failed — attempt repair if loops remain.
        if (repair + 1 < cfg.max_repair_loops) {
            const fr = failed_result.?;
            log.info("gate_loop.repair_start error_code={s} run_id={s} gate={s} attempt={d} exit_code={d}", .{
                codes.ERR_GATE_COMMAND_FAILED, cfg.run_id, fr.gate_name, repair + 1, fr.exit_code,
            });
            metrics.incGateRepairLoops();

            if (cfg.executor) |exec| {
                if (cfg.execution_id) |exec_id| {
                    const repair_msg = try buildRepairMessage(cfg.alloc, fr);
                    defer cfg.alloc.free(repair_msg);
                    _ = exec.startStage(exec_id, .{
                        .stage_id = cfg.repair_stage_id,
                        .role_id = cfg.repair_role_id,
                        .skill_id = cfg.repair_skill_id,
                        .message = repair_msg,
                    }) catch |err| {
                        log.warn("gate_loop.repair_stage_failed err={s} run_id={s}", .{ @errorName(err), cfg.run_id });
                    };
                }
            }
        }
    }

    // Repair loops exhausted — transition directly to CANCELLED so the caller
    // skips handleGateExhaustedOutcome (which would land the run in BLOCKED).
    log.warn("gate_loop.exhausted error_code={s} run_id={s} attempts={d}", .{
        codes.ERR_GATE_REPAIR_EXHAUSTED, cfg.run_id, cfg.max_repair_loops,
    });
    metrics.incGateRepairExhausted();
    metrics.incRunLimitRepairLoopsExhausted();
    _ = state_machine.transition(cfg.conn, cfg.run_id, .CANCELLED, .orchestrator, .REPAIR_LOOPS_EXHAUSTED, "gate repair loops exhausted") catch |err| {
        log.warn("gate_loop.exhausted_transition_fail err={s} run_id={s}", .{ @errorName(err), cfg.run_id });
    };

    persistGateResults(cfg.alloc, cfg.conn, cfg.run_id, results.items) catch |err| {
        log.warn("gate_loop.persist_failed error_code={s} err={s} run_id={s}", .{ codes.ERR_GATE_PERSIST_FAILED, @errorName(err), cfg.run_id });
    };

    return .{
        .all_passed = false,
        .results = results,
        .total_repair_loops = cfg.max_repair_loops,
        .exhausted = true,
        .state_written = true,
    };
}

pub fn effectiveTimeout(gate_timeout: u64, global_timeout: u64, deadline_ms: i64) u64 {
    const base = @min(gate_timeout, global_timeout);
    if (deadline_ms <= 0) return base; // no deadline configured
    const now_ms: u64 = @intCast(@max(0, std.time.milliTimestamp()));
    const dl: u64 = @intCast(deadline_ms);
    const remaining: u64 = if (dl > now_ms) dl - now_ms else 0;
    return @min(base, remaining);
}

pub fn executeGateCommand(
    alloc: std.mem.Allocator,
    wt_path: []const u8,
    gate_name: []const u8,
    command_str: []const u8,
    timeout_ms: u64,
) !GateToolResult {
    const start_ms: u64 = @intCast(@max(0, std.time.milliTimestamp()));

    // Split command string into argv for child process.
    // Gate commands are trusted (from profile config), so simple split is safe.
    var argv_list: std.ArrayList([]const u8) = .{};
    defer argv_list.deinit(alloc);
    var it = std.mem.tokenizeScalar(u8, command_str, ' ');
    while (it.next()) |token| {
        try argv_list.append(alloc, token);
    }
    if (argv_list.items.len == 0) {
        return .{
            .gate_name = gate_name,
            .exit_code = 1,
            .stdout = "",
            .stderr = "empty command",
            .wall_ms = 0,
            .passed = false,
        };
    }

    var resources = git.CommandResources.init(alloc, argv_list.items, wt_path) catch |err| {
        const end_ms: u64 = @intCast(@max(0, std.time.milliTimestamp()));
        return .{
            .gate_name = gate_name,
            .exit_code = 1,
            .stdout = "",
            .stderr = @errorName(err),
            .wall_ms = end_ms - start_ms,
            .passed = false,
        };
    };
    defer resources.deinit();

    // Enforce timeout: spawn a timer thread that kills the child if it exceeds the deadline.
    // Main thread blocks on wait(); timer thread sleeps then kills.
    // Both sides use CAS to atomically claim the "done" flag — exactly one side wins.
    // This prevents SIGKILL on a recycled PID after waitpid() has reaped the child.
    var timed_out = std.atomic.Value(bool).init(false);
    const timer_thread = std.Thread.spawn(.{}, struct {
        fn run(child: *std.process.Child, timeout_ns: u64, flag: *std.atomic.Value(bool)) void {
            std.Thread.sleep(timeout_ns);
            // CAS: only kill if we successfully change false → true (we win the race).
            if (flag.cmpxchgWeak(false, true, .acq_rel, .acquire) == null) {
                _ = child.kill() catch {};
            }
        }
    }.run, .{ &resources.child, timeout_ms * std.time.ns_per_ms, &timed_out }) catch null;

    const term = resources.child.wait() catch |err| {
        // Atomically claim the flag so timer thread won't kill a reaped PID.
        _ = timed_out.swap(true, .acq_rel);
        if (timer_thread) |t| t.join();
        const end_ms: u64 = @intCast(@max(0, std.time.milliTimestamp()));
        return .{
            .gate_name = gate_name,
            .exit_code = 1,
            .stdout = try alloc.dupe(u8, ""),
            .stderr = try alloc.dupe(u8, @errorName(err)),
            .wall_ms = end_ms - start_ms,
            .passed = false,
            .owned = true,
        };
    };

    // Atomically claim the flag: swap returns the old value.
    // If old=false, we won — child exited naturally. If old=true, timer won — it was a timeout.
    const was_timed_out = timed_out.swap(true, .acq_rel);
    if (timer_thread) |t| t.join();

    if (was_timed_out) {
        log.warn("gate_loop.command_timeout error_code={s} gate={s} timeout_ms={d}", .{
            codes.ERR_GATE_COMMAND_TIMEOUT, gate_name, timeout_ms,
        });
        const end_ms: u64 = @intCast(@max(0, std.time.milliTimestamp()));
        return .{
            .gate_name = gate_name,
            .exit_code = 124,
            .stdout = try alloc.dupe(u8, ""),
            .stderr = try alloc.dupe(u8, "gate command timed out"),
            .wall_ms = end_ms - start_ms,
            .passed = false,
            .owned = true,
        };
    }

    resources.readOutput() catch {};
    const end_ms: u64 = @intCast(@max(0, std.time.milliTimestamp()));

    const exit_code: u32 = switch (term) {
        .Exited => |code| code,
        else => 128,
    };

    // Dupe stdout/stderr before resources.deinit() fires (defer frees the originals).
    const stdout_owned = try alloc.dupe(u8, truncateOutput(resources.stdout orelse "", MAX_OUTPUT_BYTES));
    errdefer alloc.free(stdout_owned);
    const stderr_owned = try alloc.dupe(u8, truncateOutput(resources.stderr orelse "", MAX_OUTPUT_BYTES));

    return .{
        .gate_name = gate_name,
        .exit_code = exit_code,
        .stdout = stdout_owned,
        .stderr = stderr_owned,
        .wall_ms = end_ms - start_ms,
        .passed = exit_code == 0,
        .owned = true,
    };
}

pub fn truncateOutput(s: []const u8, max: usize) []const u8 {
    return if (s.len > max) s[s.len - max ..] else s;
}

pub fn buildRepairMessage(alloc: std.mem.Allocator, result: GateToolResult) ![]const u8 {
    return std.fmt.allocPrint(alloc,
        \\Gate '{s}' failed (exit code {d}).
        \\
        \\stdout:
        \\{s}
        \\
        \\stderr:
        \\{s}
        \\
        \\Fix the issue and re-run the gate.
    , .{ result.gate_name, result.exit_code, result.stdout, result.stderr });
}

fn publishGateEvent(cfg: GateLoopConfig, result: GateToolResult, loop: u32) void {
    const redis_client = cfg.redis orelse return;
    const channel = std.fmt.allocPrint(cfg.alloc, "run:{s}:events", .{cfg.run_id}) catch return;
    defer cfg.alloc.free(channel);
    const outcome_str = if (result.passed) "PASS" else "FAIL";
    const event_json = std.fmt.allocPrint(cfg.alloc,
        \\{{"gate_name":"{s}","outcome":"{s}","exit_code":{d},"loop":{d},"wall_ms":{d}}}
    , .{ result.gate_name, outcome_str, result.exit_code, loop, result.wall_ms }) catch return;
    defer cfg.alloc.free(event_json);
    redis_client.publish(channel, event_json) catch |err| {
        log.warn("gate_loop.pubsub_fail err={s} run_id={s}", .{ @errorName(err), cfg.run_id });
    };
}

fn persistGateResults(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    run_id: []const u8,
    results: []const GateToolResult,
) !void {
    const now_ms = std.time.milliTimestamp();
    for (results, 0..) |r, i| {
        const gate_id = try id_format.generateGateResultId(alloc);
        defer alloc.free(gate_id);
        _ = try conn.exec(
            "INSERT INTO gate_results (id, run_id, gate_name, attempt, exit_code, stdout_tail, stderr_tail, wall_ms, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            .{ gate_id, run_id, r.gate_name, @as(i32, @intCast(i + 1)), @as(i32, @intCast(r.exit_code)), r.stdout, r.stderr, @as(i64, @intCast(r.wall_ms)), now_ms },
        );
    }
}

/// Format a markdown scorecard table from gate results.
pub fn formatScorecard(
    alloc: std.mem.Allocator,
    results: []const GateToolResult,
    total_repair_loops: u32,
    run_id: []const u8,
) ![]const u8 {
    var buf: std.ArrayList(u8) = .{};
    const w = buf.writer(alloc);
    try w.writeAll("## Gate Results\n\n");
    try w.writeAll("| Gate | Result | Wall Time |\n");
    try w.writeAll("|------|--------|-----------|\n");
    var total_wall: u64 = 0;
    for (results) |r| {
        total_wall += r.wall_ms;
        try w.print("| {s} | {s} | {d}ms |\n", .{
            r.gate_name,
            if (r.passed) "PASS" else "FAIL",
            r.wall_ms,
        });
    }
    try w.print("\n**Repair loops:** {d} | **Total wall time:** {d}ms | **Run ID:** {s}\n", .{
        total_repair_loops, total_wall, run_id,
    });
    return buf.toOwnedSlice(alloc);
}

// Tests in worker_gate_loop_test.zig
