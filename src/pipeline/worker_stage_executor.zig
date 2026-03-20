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
const scoring = @import("scoring.zig");
const worker_stage_types = @import("worker_stage_types.zig");
const worker_stage_helpers = @import("worker_stage_helpers.zig");
const worker_stage_outcomes = @import("worker_stage_outcomes.zig");
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
};

fn opRunStage(ctx: StageRetryCtx, _: u32) !agents.AgentResult {
    return agents.runByRole(ctx.alloc, ctx.binding, ctx.input);
}

fn recordScoringFailure(scoring_state: *scoring.ScoringState, err: anyerror) anyerror {
    scoring_state.failure_error_name = @errorName(err);
    return err;
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

    const plan_stage = profile.stages[0];
    const plan_binding = resolveBinding(cfg, plan_stage.role_id, plan_stage.skill_id) orelse return worker_runtime.WorkerError.InvalidPipelineRole;

    try worker_runtime.ensureRunActive(running, deadline_ms);
    try tenant_limiter.acquireCancelable(ctx.tenant_id, plan_stage.skill_id, 1.0, running, deadline_ms);
    const plan_result = reliable.call(agents.AgentResult, StageRetryCtx{
        .alloc = run_alloc,
        .binding = plan_binding,
        .input = .{
            .workspace_path = wt.path,
            .prompts = prompts,
            .spec_content = spec_content,
            .memory_context = memory_context,
        },
    }, opRunStage, worker_runtime.retryOptionsForRun(@constCast(running), deadline_ms, 1, 1_000, 8_000, plan_stage.skill_id)) catch |err| return recordScoringFailure(&scoring_state, err);
    metrics.incAgentEchoCalls();
    metrics.addAgentTokens(plan_result.token_count);
    metrics.observeAgentDurationSeconds(plan_result.wall_seconds);
    worker_stage_helpers.emitLangfuseTrace(run_alloc, ctx, plan_stage.stage_id, plan_stage.role_id, plan_result);

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

            log.info("stage start run_id={s} stage_id={s} role={s} attempt={d}", .{ ctx.run_id, stage.stage_id, stage.role_id, attempt });

            try tenant_limiter.acquireCancelable(ctx.tenant_id, stage.skill_id, 1.0, running, deadline_ms);
            try worker_runtime.ensureRunActive(running, deadline_ms);
            const stage_result = reliable.call(agents.AgentResult, StageRetryCtx{
                .alloc = run_alloc,
                .binding = binding,
                .input = .{
                    .workspace_path = wt.path,
                    .prompts = prompts,
                    .spec_content = spec_content,
                    .plan_content = plan_result.content,
                    .defects_content = defects,
                    .implementation_summary = latest_build_output,
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
            worker_stage_helpers.emitLangfuseTrace(run_alloc, ctx, stage.stage_id, stage.role_id, stage_result);

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
                try worker_stage_outcomes.handleDoneOutcome(.{
                    .alloc = run_alloc,
                    .conn = conn,
                    .ctx = ctx,
                    .cfg = cfg,
                    .wt = &wt,
                    .branch = branch,
                    .running = running,
                    .deadline_ms = deadline_ms,
                    .token_cache = token_cache,
                    .tenant_limiter = tenant_limiter,
                    .final_stage_output = final_stage_output,
                    .final_stage_actor = final_stage_actor,
                    .attempt = attempt,
                    .total_tokens = total_tokens,
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

                log.info("retrying run_id={s} attempt={d}", .{ ctx.run_id, attempt + 1 });
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
