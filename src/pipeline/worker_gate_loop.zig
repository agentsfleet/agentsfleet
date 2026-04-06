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
const trace_mod = @import("../observability/trace.zig");
const gate_spans = @import("worker_gate_spans.zig");
const id_format = @import("../types/id_format.zig");
const codes = @import("../errors/codes.zig");
const worker_runtime = @import("worker_runtime.zig");
const queue_redis = @import("../queue/redis.zig");
const queue_consts = @import("../queue/constants.zig");
const state_machine = @import("../state/machine.zig");
const types = @import("../types.zig");
const helpers = @import("worker_gate_helpers.zig");
const limits = @import("worker_gate_limits.zig");

const log = std.log.scoped(.gate_loop);

/// Maximum bytes captured per stdout/stderr stream.
pub const MAX_OUTPUT_BYTES: usize = 4096;

/// M21_002 §2.4: Interrupt poll interval inside gate commands (Option B).
const INTERRUPT_POLL_INTERVAL_MS: u64 = 2000;

/// M21_002 §3.3: Grace period after SIGTERM before SIGKILL.
const SIGTERM_GRACE_MS: u64 = 5000;

/// M21_002: Exit reason for gate child process.
const ExitReason = enum(u8) { running = 0, timed_out = 1, interrupted = 2, done = 3 };

pub const GateToolResult = struct {
    gate_name: []const u8,
    exit_code: u32,
    stdout: []const u8, // owned when from executeGateCommand
    stderr: []const u8, // owned when from executeGateCommand
    wall_ms: u64,
    passed: bool,
    owned: bool = false,
    /// M21_002 §3.3: true when gate was killed by an interrupt signal.
    interrupted: bool = false,

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
    // M28_001 §1.4: root span ID for gate tool child spans.
    root_span_id: [trace_mod.SPAN_ID_HEX_LEN]u8 = [_]u8{0} ** trace_mod.SPAN_ID_HEX_LEN,
    trace_id: []const u8 = "",
};

/// M21_001 §2.1: Check Redis for a pending interrupt message (GETDEL).
fn checkInterruptSignal(cfg: GateLoopConfig) ?[]const u8 {
    const redis = cfg.redis orelse return null;
    const key = std.fmt.allocPrint(cfg.alloc, queue_consts.interrupt_key_prefix ++ "{s}", .{cfg.run_id}) catch return null;
    defer cfg.alloc.free(key);
    const argv = [_][]const u8{ "GETDEL", key };
    var resp = redis.command(&argv) catch return null;
    return switch (resp) {
        .bulk => |s| s,
        else => blk: {
            resp.deinit(cfg.alloc);
            break :blk null;
        },
    };
}

/// M21_002 §2.1: Check Redis for interrupt existence (EXISTS, non-destructive).
/// Used by the timer thread during gate command execution.
fn interruptExists(redis: *queue_redis.Client, run_id: []const u8, alloc: std.mem.Allocator) bool {
    const key = std.fmt.allocPrint(alloc, queue_consts.interrupt_key_prefix ++ "{s}", .{run_id}) catch return false;
    defer alloc.free(key);
    return redis.exists(key) catch false;
}

pub fn runGateLoop(cfg: GateLoopConfig) !GateLoopOutcome {
    var results = std.ArrayList(GateToolResult){};

    var repair: u32 = 0;
    while (repair < cfg.max_repair_loops) : (repair += 1) {
        try worker_runtime.ensureRunActive(cfg.running, cfg.deadline_ms);

        // M21_001 §2.1: poll for pending interrupt before running gates.
        if (checkInterruptSignal(cfg)) |interrupt_msg| {
            log.info("gate_loop.interrupt_received run_id={s} workspace_id={s} agent_id={s} msg_len={d}", .{ cfg.run_id, cfg.workspace_id, cfg.agent_id, interrupt_msg.len });
            metrics.incInterruptQueued();
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
            const result = try executeGateCommand(cfg.alloc, cfg.wt_path, gate.name, gate.command, timeout, cfg.redis, cfg.run_id);
            try results.append(cfg.alloc, result);

            helpers.publishGateEvent(cfg, result, repair);
            gate_spans.emit(
                .{ .run_id = cfg.run_id, .workspace_id = cfg.workspace_id, .trace_id = cfg.trace_id, .root_span_id = cfg.root_span_id },
                .{ .gate_name = result.gate_name, .exit_code = result.exit_code, .wall_ms = result.wall_ms, .passed = result.passed },
                repair,
            );

            if (!result.passed) {
                failed_result = result;
                break;
            }
        }

        // M17_001 §1.2 / §3.2: check limits and cancel signal after each iteration.
        if (try limits.checkPostIterationLimits(cfg)) {
            helpers.persistGateResults(cfg.alloc, cfg.conn, cfg.run_id, results.items) catch |err| {
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
            helpers.persistGateResults(cfg.alloc, cfg.conn, cfg.run_id, results.items) catch |err| {
                log.warn("gate_loop.persist_failed error_code={s} err={s} run_id={s}", .{ codes.ERR_GATE_PERSIST_FAILED, @errorName(err), cfg.run_id });
            };
            return .{
                .all_passed = true,
                .results = results,
                .total_repair_loops = repair,
                .exhausted = false,
            };
        }

        // M21_002 §3.3: If the gate was killed by an interrupt, skip repair.
        // The next iteration's checkInterruptSignal will GETDEL and inject the message.
        if (failed_result.?.interrupted) {
            log.info("gate_loop.interrupt_killed_gate run_id={s} workspace_id={s} agent_id={s} delivery_mode=instant wall_ms={d}", .{
                cfg.run_id, cfg.workspace_id, cfg.agent_id, failed_result.?.wall_ms,
            });
            // M21_002 §4.1: Only count as instant when timer thread actually killed the gate.
            metrics.incInterruptInstant();
            // M21_002 §4.3: Record delivery latency (gate wall time until interrupt).
            metrics.observeInterruptDeliveryLatencyMs(failed_result.?.wall_ms);
            continue;
        }

        // Gate failed — attempt repair if loops remain.
        if (repair + 1 < cfg.max_repair_loops) {
            const fr = failed_result.?;
            log.info("gate_loop.repair_start error_code={s} run_id={s} gate={s} attempt={d} exit_code={d}", .{
                codes.ERR_GATE_COMMAND_FAILED, cfg.run_id, fr.gate_name, repair + 1, fr.exit_code,
            });
            metrics.incGateRepairLoops();
            metrics.wsIncGateRepairLoops(cfg.workspace_id);

            if (cfg.executor) |exec| {
                if (cfg.execution_id) |exec_id| {
                    const repair_msg = try helpers.buildRepairMessage(cfg.alloc, fr);
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

    helpers.persistGateResults(cfg.alloc, cfg.conn, cfg.run_id, results.items) catch |err| {
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

/// M21_002 §2.1 (Option B): Timer thread context for interrupt-aware gate execution.
/// Polls Redis every 2s for interrupt signals while enforcing the gate timeout.
/// On interrupt: SIGTERM → SIGKILL after 5s grace period.
/// On timeout: SIGKILL (existing behavior).
const TimerContext = struct {
    child: *std.process.Child,
    timeout_ms: u64,
    exit_reason: *std.atomic.Value(u8),
    redis: ?*queue_redis.Client,
    run_id: []const u8,
    alloc: std.mem.Allocator,

    fn run(ctx: TimerContext) void {
        const start_ms: i64 = std.time.milliTimestamp();
        const deadline_ms: i64 = start_ms + @as(i64, @intCast(ctx.timeout_ms));
        while (std.time.milliTimestamp() < deadline_ms) {
            std.Thread.sleep(INTERRUPT_POLL_INTERVAL_MS * std.time.ns_per_ms);

            // M21_002: Check for interrupt signal in Redis.
            if (ctx.redis) |redis| {
                if (interruptExists(redis, ctx.run_id, ctx.alloc)) {
                    // CAS: claim exit reason as interrupted (0 → 2).
                    // Use cmpxchgStrong — spurious failure from Weak would
                    // silently drop the interrupt delivery.
                    if (ctx.exit_reason.cmpxchgStrong(
                        @intFromEnum(ExitReason.running),
                        @intFromEnum(ExitReason.interrupted),
                        .acq_rel,
                        .acquire,
                    ) == null) {
                        killWithEscalation(ctx.child, ctx.exit_reason);
                    }
                    return;
                }
            }
        }
        // Timeout: CAS 0 → 1.
        if (ctx.exit_reason.cmpxchgWeak(
            @intFromEnum(ExitReason.running),
            @intFromEnum(ExitReason.timed_out),
            .acq_rel,
            .acquire,
        ) == null) {
            _ = ctx.child.kill() catch {};
        }
    }
};

/// M21_002 §3.3: Send SIGTERM, poll exit_reason during grace period, then SIGKILL.
/// Polls every 50ms so t.join() unblocks promptly when the child exits after SIGTERM,
/// keeping total delivery latency within the 5s spec.
fn killWithEscalation(child: *std.process.Child, exit_reason: *std.atomic.Value(u8)) void {
    std.posix.kill(child.id, std.posix.SIG.TERM) catch {
        _ = child.kill() catch {};
        return;
    };
    const grace_deadline = std.time.milliTimestamp() + @as(i64, @intCast(SIGTERM_GRACE_MS));
    while (std.time.milliTimestamp() < grace_deadline) {
        std.Thread.sleep(50 * std.time.ns_per_ms);
        // Main thread sets done after child.wait() returns — exit early.
        if (exit_reason.load(.acquire) == @intFromEnum(ExitReason.done)) return;
    }
    _ = child.kill() catch {};
}

pub fn executeGateCommand(
    alloc: std.mem.Allocator,
    wt_path: []const u8,
    gate_name: []const u8,
    command_str: []const u8,
    timeout_ms: u64,
    redis: ?*queue_redis.Client,
    run_id: []const u8,
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

    // M21_002: Interrupt-aware timer thread. Polls Redis every 2s for interrupt
    // signals while enforcing the gate timeout. Replaces the simple sleep-then-kill
    // timer from M16_001.
    var exit_reason = std.atomic.Value(u8).init(@intFromEnum(ExitReason.running));
    const timer_thread = std.Thread.spawn(.{}, TimerContext.run, .{TimerContext{
        .child = &resources.child,
        .timeout_ms = timeout_ms,
        .exit_reason = &exit_reason,
        .redis = redis,
        .run_id = run_id,
        .alloc = alloc,
    }}) catch |err| blk: {
        log.warn("gate_loop.timer_spawn_failed err={s} gate={s}", .{ @errorName(err), gate_name });
        break :blk null;
    };

    const term = resources.child.wait() catch |err| {
        // Claim done so timer thread won't kill a reaped PID.
        _ = exit_reason.swap(@intFromEnum(ExitReason.done), .acq_rel);
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

    // Swap to done — old value tells us what happened.
    const old_reason = exit_reason.swap(@intFromEnum(ExitReason.done), .acq_rel);
    if (timer_thread) |t| t.join();

    const was_timed_out = old_reason == @intFromEnum(ExitReason.timed_out);
    const was_interrupted = old_reason == @intFromEnum(ExitReason.interrupted);

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

    if (was_interrupted) {
        log.info("gate_loop.command_interrupted gate={s} run_id={s}", .{ gate_name, run_id });
        const end_ms: u64 = @intCast(@max(0, std.time.milliTimestamp()));
        return .{
            .gate_name = gate_name,
            .exit_code = 130,
            .stdout = try alloc.dupe(u8, ""),
            .stderr = try alloc.dupe(u8, "gate command interrupted by user"),
            .wall_ms = end_ms - start_ms,
            .passed = false,
            .owned = true,
            .interrupted = true,
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

// Re-export helpers for backward compatibility with tests and external callers.
pub const buildRepairMessage = helpers.buildRepairMessage;
pub const formatScorecard = helpers.formatScorecard;

// Tests in worker_gate_loop_test.zig
