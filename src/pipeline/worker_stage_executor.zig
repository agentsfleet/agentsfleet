const std = @import("std");
const pg = @import("pg");
const types = @import("../types.zig");
const state = @import("../state/machine.zig");
const billing = @import("../state/billing.zig");
const agents = @import("agents.zig");
const git = @import("../git/ops.zig");
const github_auth = @import("../auth/github.zig");
const memory = @import("../memory/workspace.zig");
const backoff = @import("../reliability/backoff.zig");
const reliable = @import("../reliability/reliable_call.zig");
const topology = @import("topology.zig");
const worker_runtime = @import("worker_runtime.zig");
const worker_paths = @import("worker_paths.zig");
const worker_rate_limiter = @import("worker_rate_limiter.zig");
const worker_execute_run = @import("worker_execute_run.zig");
const metrics = @import("../observability/metrics.zig");
const posthog_events = @import("../observability/posthog_events.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const trace_mod = @import("../observability/trace.zig");
const scoring = @import("scoring.zig");
const worker_stage_types = @import("worker_stage_types.zig");
const worker_stage_helpers = @import("worker_stage_helpers.zig");
const worker_stage_outcomes = @import("worker_stage_outcomes.zig");
const worker_gate_loop = @import("worker_gate_loop.zig");
const sandbox_runtime = @import("sandbox_runtime.zig");
const executor_client = @import("../executor/client.zig");
const log = std.log.scoped(.worker);

pub const ExecuteConfig = worker_stage_types.ExecuteConfig;
pub const RunContext = worker_stage_types.RunContext;

const BareCloneRetryCtx = struct {
    alloc: std.mem.Allocator,
    cache_root: []const u8,
    workspace_id: []const u8,
    repo_url: []const u8,
};

fn opEnsureBareClone(ctx: BareCloneRetryCtx, _: u32) ![]const u8 {
    return git.ensureBareClone(ctx.alloc, ctx.cache_root, ctx.workspace_id, ctx.repo_url);
}

const WorktreeRetryCtx = struct {
    alloc: std.mem.Allocator,
    bare_path: []const u8,
    run_id: []const u8,
    base_branch: []const u8,
};

fn opCreateWorktree(ctx: WorktreeRetryCtx, _: u32) !git.WorktreeHandle {
    return git.createWorktree(ctx.alloc, ctx.bare_path, ctx.run_id, ctx.base_branch);
}

const StageRetryCtx = struct {
    alloc: std.mem.Allocator,
    binding: agents.RoleBinding,
    input: agents.RoleInput,
    executor: ?*executor_client.ExecutorClient = null,
    execution_id: ?[]const u8 = null,
};

fn opRunStage(ctx: StageRetryCtx, _: u32) !agents.AgentResult {
    // M12_003: When executor is configured, dispatch via executor sidecar.
    if (ctx.executor) |exec| {
        if (ctx.execution_id) |exec_id| {
            return dispatchViaExecutor(ctx.alloc, exec, exec_id, ctx.binding, ctx.input);
        }
    }
    // Fallback: direct in-process execution (dev mode, macOS).
    return agents.runByRole(ctx.alloc, ctx.binding, ctx.input);
}

/// Build StartStage payload from RoleBinding + RoleInput and dispatch
/// via the executor sidecar (M12_003 §4.2).
///
/// The executor is agent-agnostic — it receives the full config and runs
/// any dynamic agent. The worker assembles the payload; the executor
/// does not interpret roles, tool sets, or prompts.
fn dispatchViaExecutor(
    alloc: std.mem.Allocator,
    exec: *executor_client.ExecutorClient,
    execution_id: []const u8,
    binding: agents.RoleBinding,
    input: agents.RoleInput,
) !agents.AgentResult {
    // Build context object with all stage-specific content.
    // The executor passes these through to the agent via the composed message.
    var context = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer context.object.deinit();
    if (input.spec_content) |sc| if (sc.len > 0) try context.object.put("spec_content", .{ .string = sc });
    if (input.plan_content) |pc| if (pc.len > 0) try context.object.put("plan_content", .{ .string = pc });
    if (input.memory_context) |mc| if (mc.len > 0) try context.object.put("memory_context", .{ .string = mc });
    if (input.defects_content) |d| try context.object.put("defects_content", .{ .string = d });
    if (input.implementation_summary) |is| if (is.len > 0) try context.object.put("implementation_summary", .{ .string = is });

    // Resolve system prompt from the binding's kind and the prompts.
    // For custom agents, the custom_runner handles prompt resolution directly.
    const system_prompt = resolveSystemPrompt(binding, input.prompts);

    // Tools = null → executor uses allTools() (agent-agnostic, all tools available).
    // The executor process-level sandbox (Landlock + cgroups) enforces security.
    const exec_ctx = input.execution_context orelse sandbox_runtime.ToolExecutionContext{};
    const stage_result = try exec.startStage(execution_id, .{
        .stage_id = exec_ctx.stage_id,
        .role_id = exec_ctx.role_id,
        .skill_id = exec_ctx.skill_id,
        .agent_config = .{
            .system_prompt = system_prompt,
        },
        .message = system_prompt,
        .tools = null,
        .context = context,
    });

    return .{
        .content = stage_result.content,
        .token_count = stage_result.token_count,
        .wall_seconds = stage_result.wall_seconds,
        .exit_ok = stage_result.exit_ok,
    };
}

fn emitAgentSpan(
    trace_id: []const u8,
    actor_label: []const u8,
    result: agents.AgentResult,
    start_ns: u64,
) void {
    const end_ns: u64 = @intCast(std.time.nanoTimestamp());
    // Build a trace context for this span using the run's trace_id
    var tc: trace_mod.TraceContext = undefined;
    const tid_len = @min(trace_id.len, trace_mod.TRACE_ID_HEX_LEN);
    @memcpy(tc.trace_id[0..tid_len], trace_id[0..tid_len]);
    if (tid_len < trace_mod.TRACE_ID_HEX_LEN) {
        @memset(tc.trace_id[tid_len..], '0');
    }
    const child = trace_mod.TraceContext.generate();
    tc.span_id = child.span_id;
    tc.parent_span_id = null;

    var span = otel_traces.buildSpan(tc, "agent.call", start_ns, end_ns);
    _ = otel_traces.addAttr(&span, "agent.actor", actor_label);

    var tokens_buf: [20]u8 = undefined;
    const tokens_str = std.fmt.bufPrint(&tokens_buf, "{d}", .{result.token_count}) catch "0";
    _ = otel_traces.addAttr(&span, "agent.tokens", tokens_str);

    var dur_buf: [20]u8 = undefined;
    const dur_str = std.fmt.bufPrint(&dur_buf, "{d}", .{result.wall_seconds * 1000}) catch "0";
    _ = otel_traces.addAttr(&span, "agent.duration_ms", dur_str);

    _ = otel_traces.addAttr(&span, "agent.exit_ok", if (result.exit_ok) "true" else "false");

    otel_traces.enqueueSpan(span);
}

fn recordScoringFailure(scoring_state: *scoring.ScoringState, err: anyerror) anyerror {
    scoring_state.failure_error_name = @errorName(err);
    return err;
}

/// Resolve the system prompt for a binding from the prompt files.
/// For custom skills, the prompt is empty — the custom runner provides it.
fn resolveSystemPrompt(binding: agents.RoleBinding, prompts: *const agents.PromptFiles) []const u8 {
    return switch (binding.kind) {
        .echo => prompts.echo,
        .scout => prompts.scout,
        .warden => prompts.warden,
        .custom => "",
    };
}

fn resolveBinding(cfg: ExecuteConfig, role_id: []const u8, skill_id: []const u8) ?agents.RoleBinding {
    if (cfg.skill_registry) |registry| {
        if (agents.resolveRoleWithRegistry(registry, role_id, skill_id)) |binding| return binding;
    }
    return agents.resolveRole(role_id, skill_id);
}

pub fn executeRun(
    alloc: std.mem.Allocator,
    cfg: ExecuteConfig,
    running: *const std.atomic.Value(bool),
    prompts: *const agents.PromptFiles,
    profile: *const topology.Profile,
    conn: *pg.Conn,
    token_cache: *github_auth.TokenCache,
    ctx: RunContext,
    tenant_limiter: *worker_rate_limiter.TenantRateLimiter,
) !void {
    const deadline_ms = std.time.milliTimestamp() + @as(i64, @intCast(cfg.run_timeout_ms));
    try worker_runtime.ensureRunActive(running, deadline_ms);

    var run_arena = std.heap.ArenaAllocator.init(alloc);
    defer run_arena.deinit();
    const run_alloc = run_arena.allocator();

    var scoring_state = scoring.ScoringState{};
    var total_wall_seconds: u64 = 0;
    var total_tokens: u64 = 0;
    defer {
        scoring.scoreRunIfTerminal(
            conn,
            cfg.posthog,
            ctx.run_id,
            ctx.workspace_id,
            ctx.agent_id,
            ctx.requested_by,
            &scoring_state,
            total_wall_seconds,
        );
    }
    errdefer scoring_state.outcome = .error_propagation;

    if (!running.load(.acquire)) return worker_runtime.WorkerError.ShutdownRequested;

    const bare_path = try reliable.call([]const u8, BareCloneRetryCtx{
        .alloc = run_alloc,
        .cache_root = cfg.cache_root,
        .workspace_id = ctx.workspace_id,
        .repo_url = ctx.repo_url,
    }, opEnsureBareClone, worker_runtime.retryOptionsForRun(@constCast(running), deadline_ms, 2, 500, 5_000, "git_ensure_bare_clone"));

    var wt = try reliable.call(git.WorktreeHandle, WorktreeRetryCtx{
        .alloc = run_alloc,
        .bare_path = bare_path,
        .run_id = ctx.run_id,
        .base_branch = ctx.default_branch,
    }, opCreateWorktree, worker_runtime.retryOptionsForRun(@constCast(running), deadline_ms, 2, 500, 5_000, "git_create_worktree"));
    defer {
        git.removeWorktree(run_alloc, bare_path, wt.path);
        wt.deinit();
    }

    // M16_001 §4.1: Record base commit SHA for worktree isolation.
    const head_sha = git.getHeadSha(run_alloc, wt.path) catch |err| blk: {
        log.warn("pipeline.head_sha_fail err={s} run_id={s}", .{ @errorName(err), ctx.run_id });
        break :blk "";
    };
    if (head_sha.len > 0) {
        const now_ms = std.time.milliTimestamp();
        _ = conn.exec("UPDATE runs SET base_commit_sha = $1, updated_at = $2 WHERE run_id = $3", .{ head_sha, now_ms, ctx.run_id }) catch {};
    }

    const branch = try std.fmt.allocPrint(run_alloc, "zombie/run-{s}", .{ctx.run_id});

    const spec_abs = try worker_paths.resolveSpecPath(run_alloc, wt.path, ctx.spec_path);
    const spec_file = try std.fs.openFileAbsolute(spec_abs, .{});
    defer spec_file.close();
    const spec_content = try spec_file.readToEndAlloc(run_alloc, 512 * 1024);

    const workspace_memory_context = try memory.loadForEcho(run_alloc, conn, ctx.workspace_id, 20);
    const score_context = try worker_stage_helpers.loadScoreContextBestEffort(run_alloc, conn, ctx.workspace_id, ctx.agent_id);
    const memory_context = try std.fmt.allocPrint(
        run_alloc,
        "{s}{s}{s}",
        .{
            score_context,
            if (score_context.len > 0 and workspace_memory_context.len > 0) "\n\n" else "",
            workspace_memory_context,
        },
    );

    // M12_003 §4.1: Create executor session if executor client is configured.
    const exec_id: ?[]const u8 = if (cfg.executor) |exec| blk: {
        const eid = exec.createExecution(wt.path, .{
            .trace_id = ctx.trace_id,
            .run_id = ctx.run_id,
            .workspace_id = ctx.workspace_id,
            .stage_id = "",
            .role_id = "",
            .skill_id = "",
        }) catch |err| {
            log.warn("executor.session_create_failed err={s} — falling back to in-process", .{@errorName(err)});
            break :blk null;
        };
        break :blk eid;
    } else null;
    defer if (exec_id) |eid| {
        if (cfg.executor) |exec| exec.destroyExecution(eid) catch {};
    };

    const plan_stage = profile.stages[0];
    const plan_binding = resolveBinding(cfg, plan_stage.role_id, plan_stage.skill_id) orelse return worker_runtime.WorkerError.InvalidPipelineRole;

    try worker_runtime.ensureRunActive(running, deadline_ms);
    try tenant_limiter.acquireCancelable(ctx.tenant_id, plan_stage.skill_id, 1.0, running, deadline_ms);
    const plan_stage_start_ns: u64 = @intCast(std.time.nanoTimestamp());
    const plan_result = reliable.call(agents.AgentResult, StageRetryCtx{
        .alloc = run_alloc,
        .binding = plan_binding,
        .executor = if (exec_id != null) cfg.executor else null,
        .execution_id = exec_id,
        .input = .{
            .workspace_path = wt.path,
            .prompts = prompts,
            .spec_content = spec_content,
            .memory_context = memory_context,
            .execution_context = .{
                .cancel_flag = running,
                .deadline_ms = deadline_ms,
                .sandbox = cfg.sandbox,
                .run_id = ctx.run_id,
                .workspace_id = ctx.workspace_id,
                .request_id = ctx.request_id,
                .trace_id = ctx.trace_id,
                .stage_id = plan_stage.stage_id,
                .role_id = plan_stage.role_id,
                .skill_id = plan_stage.skill_id,
            },
        },
    }, opRunStage, worker_runtime.retryOptionsForRun(@constCast(running), deadline_ms, 1, 1_000, 8_000, plan_stage.skill_id)) catch |err| return recordScoringFailure(&scoring_state, err);
    metrics.incAgentEchoCalls();
    metrics.addAgentTokens(plan_result.token_count);
    metrics.observeAgentDurationSeconds(plan_result.wall_seconds);
    emitAgentSpan(ctx.trace_id, plan_binding.actor.label(), plan_result, plan_stage_start_ns);

    agents.emitNullclawRunEvent(
        ctx.run_id,
        ctx.request_id,
        ctx.trace_id,
        ctx.attempt,
        plan_stage.stage_id,
        plan_stage.role_id,
        plan_binding.actor,
        plan_result,
    );
    posthog_events.trackAgentCompleted(
        cfg.posthog,
        posthog_events.distinctIdOrSystem(ctx.requested_by),
        ctx.run_id,
        ctx.workspace_id,
        plan_binding.actor.label(),
        plan_result.token_count,
        plan_result.wall_seconds * 1000,
        if (plan_result.exit_ok) "ok" else "failed",
    );
    try billing.recordRuntimeStageUsage(
        conn,
        ctx.workspace_id,
        ctx.run_id,
        ctx.attempt,
        plan_stage.stage_id,
        plan_binding.actor,
        plan_result.token_count,
        plan_result.wall_seconds,
    );

    const plan_path = try std.fmt.allocPrint(run_alloc, "docs/runs/{s}/{s}", .{ ctx.run_id, plan_stage.artifact_name });
    try worker_stage_helpers.commitArtifact(run_alloc, conn, ctx, &wt, running, deadline_ms, plan_path, plan_result.content, plan_stage.commit_message, plan_binding.actor, ctx.attempt);

    if (profile.stages.len < 2) return worker_runtime.WorkerError.InvalidPipelineProfile;

    var attempt = ctx.attempt;
    var defects: ?[]const u8 = null;

    total_tokens = plan_result.token_count;
    total_wall_seconds = plan_result.wall_seconds;
    scoring_state.stages_total = 1;
    if (plan_result.exit_ok) scoring_state.stages_passed = 1;

    while (attempt <= cfg.max_attempts) : (attempt += 1) {
        try worker_runtime.ensureRunActive(running, deadline_ms);
        _ = try state.transition(conn, ctx.run_id, .PATCH_IN_PROGRESS, .orchestrator, .PATCH_STARTED, null);

        var latest_build_output: []const u8 = plan_result.content;
        var current_stage_index: usize = 1;
        var verification_started = false;
        var final_stage_output: []const u8 = plan_result.content;
        var final_stage_actor: types.Actor = .orchestrator;
        var terminal: worker_execute_run.StageTransition = .retry;

        while (true) {
            const stage = profile.stages[current_stage_index];
            const binding = resolveBinding(cfg, stage.role_id, stage.skill_id) orelse return worker_runtime.WorkerError.InvalidPipelineRole;

            if (stage.is_gate and !verification_started) {
                _ = try state.transition(conn, ctx.run_id, .PATCH_READY, .scout, .PATCH_COMMITTED, null);
                _ = try state.transition(conn, ctx.run_id, .VERIFICATION_IN_PROGRESS, .orchestrator, .PATCH_STARTED, null);
                verification_started = true;
            }

            log.info("pipeline.stage_start run_id={s} stage_id={s} role={s} attempt={d}", .{ ctx.run_id, stage.stage_id, stage.role_id, attempt });

            try tenant_limiter.acquireCancelable(ctx.tenant_id, stage.skill_id, 1.0, running, deadline_ms);
            try worker_runtime.ensureRunActive(running, deadline_ms);
            const stage_start_ns: u64 = @intCast(std.time.nanoTimestamp());
            const stage_result = reliable.call(agents.AgentResult, StageRetryCtx{
                .alloc = run_alloc,
                .binding = binding,
                .executor = if (exec_id != null) cfg.executor else null,
                .execution_id = exec_id,
                .input = .{
                    .workspace_path = wt.path,
                    .prompts = prompts,
                    .spec_content = spec_content,
                    .plan_content = plan_result.content,
                    .defects_content = defects,
                    .implementation_summary = latest_build_output,
                    .execution_context = .{
                        .cancel_flag = running,
                        .deadline_ms = deadline_ms,
                        .sandbox = cfg.sandbox,
                        .run_id = ctx.run_id,
                        .workspace_id = ctx.workspace_id,
                        .request_id = ctx.request_id,
                        .trace_id = ctx.trace_id,
                        .stage_id = stage.stage_id,
                        .role_id = stage.role_id,
                        .skill_id = stage.skill_id,
                    },
                },
            }, opRunStage, worker_runtime.retryOptionsForRun(@constCast(running), deadline_ms, 1, 1_000, 8_000, stage.skill_id)) catch |err| return recordScoringFailure(&scoring_state, err);

            switch (binding.actor) {
                .echo => metrics.incAgentEchoCalls(),
                .scout => metrics.incAgentScoutCalls(),
                .warden => metrics.incAgentWardenCalls(),
                .orchestrator => {},
            }
            metrics.addAgentTokens(stage_result.token_count);
            metrics.observeAgentDurationSeconds(stage_result.wall_seconds);
            emitAgentSpan(ctx.trace_id, binding.actor.label(), stage_result, stage_start_ns);
            agents.emitNullclawRunEvent(ctx.run_id, ctx.request_id, ctx.trace_id, attempt, stage.stage_id, stage.role_id, binding.actor, stage_result);
            posthog_events.trackAgentCompleted(
                cfg.posthog,
                posthog_events.distinctIdOrSystem(ctx.requested_by),
                ctx.run_id,
                ctx.workspace_id,
                binding.actor.label(),
                stage_result.token_count,
                stage_result.wall_seconds * 1000,
                if (stage_result.exit_ok) "ok" else "failed",
            );
            total_tokens += stage_result.token_count;
            total_wall_seconds += stage_result.wall_seconds;
            scoring_state.stages_total += 1;
            if (stage_result.exit_ok) scoring_state.stages_passed += 1;
            try billing.recordRuntimeStageUsage(
                conn,
                ctx.workspace_id,
                ctx.run_id,
                attempt,
                stage.stage_id,
                binding.actor,
                stage_result.token_count,
                stage_result.wall_seconds,
            );

            const stage_path = try std.fmt.allocPrint(run_alloc, "docs/runs/{s}/{s}", .{ ctx.run_id, stage.artifact_name });
            try worker_stage_helpers.commitArtifact(run_alloc, conn, ctx, &wt, running, deadline_ms, stage_path, stage_result.content, stage.commit_message, binding.actor, attempt);
            latest_build_output = stage_result.content;
            final_stage_output = stage_result.content;
            final_stage_actor = binding.actor;

            const passed = if (binding.actor == .warden) agents.parseWardenVerdict(stage_result.content) else true;
            if (binding.actor == .warden) {
                const observations = try agents.extractObservations(run_alloc, stage_result.content);
                if (observations.len > 0) {
                    _ = try memory.saveFromWarden(conn, ctx.workspace_id, ctx.run_id, observations);
                }
            }

            terminal = try worker_execute_run.resolveStageTransition(profile, current_stage_index, passed);
            switch (terminal) {
                .stage_index => |next_index| {
                    current_stage_index = next_index;
                    continue;
                },
                else => break,
            }
        }

        switch (terminal) {
            .done => {
                // M16_001: Run gate tools if profile defines them.
                if (profile.gate_tools.len > 0) {
                    // Resolve the implement stage for repair turns (profile-driven, not hardcoded).
                    const build_stages = profile.buildStages();
                    const repair_stage = if (build_stages.len > 0) build_stages[0] else profile.stages[1];
                    var gate_outcome = try worker_gate_loop.runGateLoop(.{
                        .alloc = run_alloc,
                        .conn = conn,
                        .run_id = ctx.run_id,
                        .workspace_id = ctx.workspace_id,
                        .wt_path = wt.path,
                        .running = running,
                        .deadline_ms = deadline_ms,
                        .executor = if (exec_id != null) cfg.executor else null,
                        .execution_id = exec_id,
                        .gate_tools = profile.gate_tools,
                        .max_repair_loops = profile.max_repair_loops,
                        .gate_tool_timeout_ms = cfg.gate_tool_timeout_ms,
                        .repair_stage_id = repair_stage.stage_id,
                        .repair_role_id = repair_stage.role_id,
                        .repair_skill_id = repair_stage.skill_id,
                    });
                    defer {
                        for (gate_outcome.results.items) |r| r.deinit(run_alloc);
                        gate_outcome.results.deinit(run_alloc);
                    }
                    if (!gate_outcome.all_passed) {
                        try worker_stage_outcomes.handleGateExhaustedOutcome(.{
                            .alloc = run_alloc, .conn = conn, .ctx = ctx, .cfg = cfg,
                            .gate_results = gate_outcome.results.items,
                            .total_repair_loops = gate_outcome.total_repair_loops,
                            .attempt = attempt, .total_wall_seconds = total_wall_seconds,
                            .scoring_state = &scoring_state,
                        });
                        return;
                    }
                    // Call handleDoneOutcome inside the if block so gate results
                    // are accessed before defer frees the ArrayList backing.
                    try worker_stage_outcomes.handleDoneOutcome(.{
                        .alloc = run_alloc, .conn = conn, .ctx = ctx, .cfg = cfg,
                        .wt = &wt, .branch = branch, .running = running,
                        .deadline_ms = deadline_ms, .token_cache = token_cache,
                        .tenant_limiter = tenant_limiter,
                        .final_stage_output = final_stage_output,
                        .final_stage_actor = final_stage_actor,
                        .attempt = attempt, .total_tokens = total_tokens,
                        .total_wall_seconds = total_wall_seconds,
                        .scoring_state = &scoring_state,
                        .gate_results = gate_outcome.results.items,
                        .gate_loop_count = gate_outcome.total_repair_loops,
                    });
                    return;
                }
                try worker_stage_outcomes.handleDoneOutcome(.{
                    .alloc = run_alloc, .conn = conn, .ctx = ctx, .cfg = cfg,
                    .wt = &wt, .branch = branch, .running = running,
                    .deadline_ms = deadline_ms, .token_cache = token_cache,
                    .tenant_limiter = tenant_limiter,
                    .final_stage_output = final_stage_output,
                    .final_stage_actor = final_stage_actor,
                    .attempt = attempt, .total_tokens = total_tokens,
                    .total_wall_seconds = total_wall_seconds,
                    .scoring_state = &scoring_state,
                });
                return;
            },
            .blocked => {
                try worker_stage_outcomes.handleBlockedOutcome(.{
                    .alloc = run_alloc,
                    .conn = conn,
                    .ctx = ctx,
                    .cfg = cfg,
                    .final_stage_output = final_stage_output,
                    .attempt = attempt,
                    .total_wall_seconds = total_wall_seconds,
                    .scoring_state = &scoring_state,
                });
                return;
            },
            .retry => {
                try billing.finalizeRunForBilling(
                    run_alloc,
                    conn,
                    ctx.workspace_id,
                    ctx.run_id,
                    attempt,
                    .non_billable,
                );
                _ = try state.transition(conn, ctx.run_id, .VERIFICATION_FAILED, final_stage_actor, .VALIDATION_FAILED, null);
                if (attempt >= cfg.max_attempts) break;

                const defects_path = try std.fmt.allocPrint(run_alloc, "docs/runs/{s}/attempt_{d}_defects.md", .{ ctx.run_id, attempt });
                try worker_stage_helpers.commitArtifact(run_alloc, conn, ctx, &wt, running, deadline_ms, defects_path, final_stage_output, "warden: add defects", final_stage_actor, attempt);

                defects = try run_alloc.dupe(u8, final_stage_output);
                _ = try state.incrementAttempt(conn, ctx.run_id);

                const retry_index = attempt - ctx.attempt;
                const delay_ms = backoff.expBackoffJitter(retry_index, 1_000, 30_000);
                metrics.incRunRetries();
                metrics.addBackoffWaitMs(delay_ms);
                try worker_runtime.sleepCooperative(delay_ms, running, deadline_ms);

                log.info("pipeline.retrying run_id={s} attempt={d}", .{ ctx.run_id, attempt + 1 });
            },
            .stage_index => unreachable,
        }
    }

    try worker_stage_outcomes.handleRetriesExhaustedOutcome(.{
        .alloc = run_alloc,
        .conn = conn,
        .ctx = ctx,
        .cfg = cfg,
        .defects = defects,
        .attempt = attempt,
        .total_wall_seconds = total_wall_seconds,
        .scoring_state = &scoring_state,
    });
}
