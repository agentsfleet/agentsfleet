const std = @import("std");
const pg = @import("pg");
const posthog = @import("posthog");
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
const worker_pr_flow = @import("worker_pr_flow.zig");
const metrics = @import("../observability/metrics.zig");
const events = @import("../events/bus.zig");
const trace = @import("../observability/trace.zig");
const langfuse = @import("../observability/langfuse.zig");
const posthog_events = @import("../observability/posthog_events.zig");
const obs_log = @import("../observability/logging.zig");
const log = std.log.scoped(.worker);

pub const ExecuteConfig = struct {
    cache_root: []const u8,
    max_attempts: u32,
    run_timeout_ms: u64,
    skill_registry: ?*const agents.SkillRegistry = null,
    posthog: ?*posthog.PostHogClient = null,
};

pub const RunContext = struct {
    run_id: []const u8,
    request_id: []const u8,
    trace_id: []const u8,
    workspace_id: []const u8,
    spec_id: []const u8,
    tenant_id: []const u8,
    requested_by: []const u8,
    repo_url: []const u8,
    default_branch: []const u8,
    spec_path: []const u8,
    attempt: u32,
};

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

const CommitRetryCtx = struct {
    alloc: std.mem.Allocator,
    wt_path: []const u8,
    rel_path: []const u8,
    content: []const u8,
    msg: []const u8,
};

fn opCommitArtifact(ctx: CommitRetryCtx, _: u32) !void {
    return git.commitFile(ctx.alloc, ctx.wt_path, ctx.rel_path, ctx.content, ctx.msg, "UseZombie Bot", "bot@usezombie.dev");
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

    const memory_context = try memory.loadForEcho(run_alloc, conn, ctx.workspace_id, 20);

    const plan_stage = profile.stages[0];
    const plan_binding = resolveBinding(cfg, plan_stage.role_id, plan_stage.skill_id) orelse return worker_runtime.WorkerError.InvalidPipelineRole;

    try worker_runtime.ensureRunActive(running, deadline_ms);
    try tenant_limiter.acquireCancelable(ctx.tenant_id, plan_stage.skill_id, 1.0, running, deadline_ms);
    const plan_result = try reliable.call(agents.AgentResult, StageRetryCtx{
        .alloc = run_alloc,
        .binding = plan_binding,
        .input = .{
            .workspace_path = wt.path,
            .prompts = prompts,
            .spec_content = spec_content,
            .memory_context = memory_context,
        },
    }, opRunStage, worker_runtime.retryOptionsForRun(@constCast(running), deadline_ms, 1, 1_000, 8_000, plan_stage.skill_id));
    metrics.incAgentEchoCalls();
    metrics.addAgentTokens(plan_result.token_count);
    metrics.observeAgentDurationSeconds(plan_result.wall_seconds);
    emitLangfuseTrace(run_alloc, ctx, plan_stage.stage_id, plan_stage.role_id, plan_result);

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
    try commitArtifact(run_alloc, conn, ctx, &wt, running, deadline_ms, plan_path, plan_result.content, plan_stage.commit_message, plan_binding.actor, ctx.attempt);

    if (profile.stages.len < 2) return worker_runtime.WorkerError.InvalidPipelineProfile;

    var attempt = ctx.attempt;
    var defects: ?[]const u8 = null;

    var total_tokens: u64 = plan_result.token_count;
    var total_wall_seconds: u64 = plan_result.wall_seconds;

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
            const stage_result = try reliable.call(agents.AgentResult, StageRetryCtx{
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
            }, opRunStage, worker_runtime.retryOptionsForRun(@constCast(running), deadline_ms, 1, 1_000, 8_000, stage.skill_id));

            switch (binding.actor) {
                .echo => metrics.incAgentEchoCalls(),
                .scout => metrics.incAgentScoutCalls(),
                .warden => metrics.incAgentWardenCalls(),
                .orchestrator => {},
            }
            metrics.addAgentTokens(stage_result.token_count);
            metrics.observeAgentDurationSeconds(stage_result.wall_seconds);
            emitLangfuseTrace(run_alloc, ctx, stage.stage_id, stage.role_id, stage_result);

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
            try commitArtifact(run_alloc, conn, ctx, &wt, running, deadline_ms, stage_path, stage_result.content, stage.commit_message, binding.actor, attempt);
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
                try billing.finalizeRunForBilling(
                    run_alloc,
                    conn,
                    ctx.workspace_id,
                    ctx.run_id,
                    attempt,
                    .completed,
                );
                _ = try state.transition(conn, ctx.run_id, .PR_PREPARED, final_stage_actor, .VALIDATION_PASSED, null);

                const pr_final = try worker_pr_flow.ensurePrForRun(
                    run_alloc,
                    conn,
                    token_cache,
                    tenant_limiter,
                    running,
                    deadline_ms,
                    wt.path,
                    .{
                        .run_id = ctx.run_id,
                        .workspace_id = ctx.workspace_id,
                        .tenant_id = ctx.tenant_id,
                        .spec_id = ctx.spec_id,
                        .repo_url = ctx.repo_url,
                        .default_branch = ctx.default_branch,
                        .branch = branch,
                        .final_stage_output = final_stage_output,
                    },
                );

                {
                    const now_ms = std.time.milliTimestamp();
                    var r = try conn.query("UPDATE runs SET pr_url = $1, updated_at = $2 WHERE run_id = $3", .{ pr_final, now_ms, ctx.run_id });
                    r.deinit();
                }

                _ = try state.transition(conn, ctx.run_id, .PR_OPENED, .orchestrator, .PR_CREATED, pr_final);
                _ = try state.transition(conn, ctx.run_id, .NOTIFIED, .orchestrator, .NOTIFICATION_SENT, null);
                _ = try state.transition(conn, ctx.run_id, .DONE, .orchestrator, .NOTIFICATION_SENT, null);

                const summary_content = std.fmt.allocPrint(
                    run_alloc,
                    "# Run Summary\n\n" ++
                        "- **run_id**: {s}\n" ++
                        "- **spec_id**: {s}\n" ++
                        "- **final_state**: DONE\n" ++
                        "- **attempt**: {d}\n" ++
                        "- **pr_url**: {s}\n" ++
                        "- **total_tokens**: {d}\n" ++
                        "- **total_wall_seconds**: {d}\n" ++
                        "\n## Artifacts\n\n" ++
                        "- plan.json (echo)\n" ++
                        "- implementation.md (scout)\n" ++
                        "- validation.md (warden)\n" ++
                        "- run_summary.md (orchestrator)\n",
                    .{ ctx.run_id, ctx.spec_id, attempt, pr_final, total_tokens, total_wall_seconds },
                ) catch |err| {
                    obs_log.logWarnErr(.worker, err, "run_summary.md alloc failed (non-fatal) run_id={s}", .{ctx.run_id});
                    try billing.finalizeRunForBilling(
                        run_alloc,
                        conn,
                        ctx.workspace_id,
                        ctx.run_id,
                        attempt,
                        .completed,
                    );
                    log.info("run completed run_id={s} pr_url={s}", .{ ctx.run_id, pr_final });
                    var done_detail: [160]u8 = undefined;
                    const done_detail_slice = std.fmt.bufPrint(&done_detail, "request_id={s} trace_id={s} state=done total_wall_seconds={d}", .{ ctx.request_id, ctx.trace_id, total_wall_seconds }) catch "run_done";
                    events.emit("run_done", ctx.run_id, done_detail_slice);
                    posthog_events.trackRunCompleted(
                        cfg.posthog,
                        posthog_events.distinctIdOrSystem(ctx.requested_by),
                        ctx.run_id,
                        ctx.workspace_id,
                        "passed",
                        total_wall_seconds * 1000,
                    );
                    metrics.observeRunTotalWallSeconds(total_wall_seconds);
                    metrics.incRunsCompleted();
                    return;
                };

                const summary_path = try std.fmt.allocPrint(run_alloc, "docs/runs/{s}/run_summary.md", .{ctx.run_id});
                commitArtifact(run_alloc, conn, ctx, &wt, running, deadline_ms, summary_path, summary_content, "orchestrator: add run_summary.md", .orchestrator, attempt) catch |err| {
                    obs_log.logWarnErr(.worker, err, "run_summary.md commit failed (non-fatal) run_id={s}", .{ctx.run_id});
                };

                try billing.finalizeRunForBilling(
                    run_alloc,
                    conn,
                    ctx.workspace_id,
                    ctx.run_id,
                    attempt,
                    .completed,
                );
                log.info("run completed run_id={s} pr_url={s}", .{ ctx.run_id, pr_final });
                var done_detail: [160]u8 = undefined;
                const done_detail_slice = std.fmt.bufPrint(&done_detail, "request_id={s} trace_id={s} state=done total_wall_seconds={d}", .{ ctx.request_id, ctx.trace_id, total_wall_seconds }) catch "run_done";
                events.emit("run_done", ctx.run_id, done_detail_slice);
                posthog_events.trackRunCompleted(
                    cfg.posthog,
                    posthog_events.distinctIdOrSystem(ctx.requested_by),
                    ctx.run_id,
                    ctx.workspace_id,
                    "passed",
                    total_wall_seconds * 1000,
                );
                metrics.observeRunTotalWallSeconds(total_wall_seconds);
                metrics.incRunsCompleted();
                return;
            },
            .blocked => {
                try billing.finalizeRunForBilling(
                    run_alloc,
                    conn,
                    ctx.workspace_id,
                    ctx.run_id,
                    attempt,
                    .non_billable,
                );
                _ = try state.transition(conn, ctx.run_id, .BLOCKED, .orchestrator, .VALIDATION_FAILED, "blocked by stage transition graph");
                _ = try state.transition(conn, ctx.run_id, .NOTIFIED_BLOCKED, .orchestrator, .NOTIFICATION_SENT, null);
                posthog_events.trackRunFailed(
                    cfg.posthog,
                    posthog_events.distinctIdOrSystem(ctx.requested_by),
                    ctx.run_id,
                    ctx.workspace_id,
                    "blocked",
                    total_wall_seconds * 1000,
                );
                metrics.observeRunTotalWallSeconds(total_wall_seconds);
                metrics.incRunsBlocked();
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
                try commitArtifact(run_alloc, conn, ctx, &wt, running, deadline_ms, defects_path, final_stage_output, "warden: add defects", final_stage_actor, attempt);

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

    try billing.finalizeRunForBilling(
        run_alloc,
        conn,
        ctx.workspace_id,
        ctx.run_id,
        attempt,
        .non_billable,
    );
    _ = try state.transition(conn, ctx.run_id, .BLOCKED, .orchestrator, .RETRIES_EXHAUSTED, null);
    _ = try state.transition(conn, ctx.run_id, .NOTIFIED_BLOCKED, .orchestrator, .NOTIFICATION_SENT, null);

    log.warn("run blocked (retries exhausted) run_id={s}", .{ctx.run_id});
    var blocked_detail: [176]u8 = undefined;
    const blocked_detail_slice = std.fmt.bufPrint(
        &blocked_detail,
        "request_id={s} trace_id={s} state=blocked reason=retries_exhausted total_wall_seconds={d}",
        .{ ctx.request_id, ctx.trace_id, total_wall_seconds },
    ) catch "run_blocked";
    events.emit("run_blocked", ctx.run_id, blocked_detail_slice);
    posthog_events.trackRunFailed(
        cfg.posthog,
        posthog_events.distinctIdOrSystem(ctx.requested_by),
        ctx.run_id,
        ctx.workspace_id,
        "retries_exhausted",
        total_wall_seconds * 1000,
    );
    metrics.observeRunTotalWallSeconds(total_wall_seconds);
    metrics.incRunsBlocked();
}

fn commitArtifact(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    ctx: RunContext,
    wt: *git.WorktreeHandle,
    running: *const std.atomic.Value(bool),
    deadline_ms: i64,
    rel_path: []const u8,
    content: []const u8,
    msg: []const u8,
    actor: types.Actor,
    attempt: u32,
) !void {
    try reliable.call(void, CommitRetryCtx{
        .alloc = alloc,
        .wt_path = wt.path,
        .rel_path = rel_path,
        .content = content,
        .msg = msg,
    }, opCommitArtifact, worker_runtime.retryOptionsForRun(@constCast(running), deadline_ms, 1, 300, 2_000, "git_commit_artifact"));

    const checksum = sha256Hex(content);
    const object_key = try std.fmt.allocPrint(alloc, "docs/runs/{s}/{s}", .{ ctx.run_id, std.fs.path.basename(rel_path) });

    const name = std.fs.path.basename(rel_path);
    try state.registerArtifact(conn, ctx.run_id, attempt, name, object_key, &checksum, actor);
}

/// Best-effort Langfuse trace emission. Reads config from env on each call
/// (cheap: returns null immediately when LANGFUSE_HOST is unset).
fn emitLangfuseTrace(
    alloc: std.mem.Allocator,
    ctx: RunContext,
    stage_id: []const u8,
    role_id: []const u8,
    result: agents.AgentResult,
) void {
    const cfg = langfuse.configFromEnv(alloc) orelse return;
    defer {
        alloc.free(cfg.host);
        alloc.free(cfg.public_key);
        alloc.free(cfg.secret_key);
    }
    langfuse.emitTrace(alloc, cfg, .{
        .trace_id = ctx.trace_id,
        .run_id = ctx.run_id,
        .stage_id = stage_id,
        .role_id = role_id,
        .token_count = result.token_count,
        .wall_seconds = result.wall_seconds,
        .exit_ok = result.exit_ok,
        .timestamp_ms = std.time.milliTimestamp(),
    });
}

fn sha256Hex(data: []const u8) [64]u8 {
    var digest: [32]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(data, &digest, .{});
    return std.fmt.bytesToHex(digest, .lower);
}
