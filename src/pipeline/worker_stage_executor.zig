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
const crypto_store = @import("../secrets/crypto_store.zig");
const error_codes = @import("../errors/codes.zig");
const log = std.log.scoped(.worker);

pub const ExecuteConfig = worker_stage_types.ExecuteConfig;
pub const RunContext = worker_stage_types.RunContext;

// M28_001: OTel span attribute values for run.outcome.
const RUN_OUTCOME_DONE = "done";
const RUN_OUTCOME_BLOCKED = "blocked";
const RUN_OUTCOME_CANCELLED = "cancelled";
const RUN_OUTCOME_ERROR = "error";

/// Resolve the LLM API key for a run (M16_004).
/// Resolution order:
///   1. Workspace BYOK: {provider}_api_key from vault.secrets.
///   2. Platform default: platform_llm_keys → admin workspace vault.secrets.
///   3. CredentialDenied — no env fallback.
fn resolveLlmApiKey(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8) ![]const u8 {
    const provider: []const u8 = crypto_store.load(alloc, conn, workspace_id, "llm_provider_preference") catch |e| p: {
        if (e != error.NotFound) return e;
        break :p "anthropic";
    };
    const key_name = try std.fmt.allocPrint(alloc, "{s}_api_key", .{provider});

    // Step 1: workspace BYOK
    if (crypto_store.load(alloc, conn, workspace_id, key_name)) |ws_key| {
        log.info("worker.llm_key_resolved api_key_source=workspace provider={s} workspace_id={s}", .{ provider, workspace_id });
        return ws_key;
    } else |ws_err| if (ws_err != error.NotFound) return ws_err;

    // Step 2: platform default
    const maybe_src: ?[]u8 = plat: {
        var pq = conn.query(
            "SELECT source_workspace_id FROM platform_llm_keys WHERE provider = $1 AND active = true LIMIT 1",
            .{provider},
        ) catch |qe| {
            log.warn("worker.platform_key_query_failed provider={s} err={s}", .{ provider, @errorName(qe) });
            break :plat null;
        };
        defer pq.deinit();
        const prow = (pq.next() catch null) orelse {
            pq.drain() catch {};
            break :plat null;
        };
        const raw = prow.get([]u8, 0) catch {
            pq.drain() catch {};
            break :plat null;
        };
        const dup = alloc.dupe(u8, raw) catch {
            pq.drain() catch {};
            break :plat null;
        };
        pq.drain() catch {};
        break :plat dup;
    };
    if (maybe_src) |src_ws| {
        if (crypto_store.load(alloc, conn, src_ws, key_name)) |platform_key| {
            log.info("worker.llm_key_resolved api_key_source=platform provider={s} source_workspace_id={s}", .{ provider, src_ws });
            return platform_key;
        } else |pe| if (pe != error.NotFound) return pe;
    }

    log.err("worker.llm_key_missing workspace_id={s} provider={s} error_code={s}", .{ workspace_id, provider, error_codes.ERR_CRED_PLATFORM_KEY_MISSING });
    return worker_runtime.WorkerError.CredentialDenied;
}

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
    /// LLM API key fetched from vault.secrets for this workspace (M16_003 §1).
    llm_api_key: []const u8 = "",
    /// GitHub installation token for this run (M16_003 §2).
    github_token: []const u8 = "",
};

fn opRunStage(ctx: StageRetryCtx, _: u32) !agents.AgentResult {
    // M12_003: When executor is configured, dispatch via executor sidecar.
    if (ctx.executor) |exec| {
        if (ctx.execution_id) |exec_id| {
            return dispatchViaExecutor(
                ctx.alloc,
                exec,
                exec_id,
                ctx.binding,
                ctx.input,
                ctx.llm_api_key,
                ctx.github_token,
            );
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
    llm_api_key: []const u8,
    github_token: []const u8,
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
            // M16_003 §1: workspace LLM API key from vault.secrets.
            .api_key = llm_api_key,
            // M16_003 §2: GitHub installation token for git push / PR creation.
            .github_token = github_token,
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

/// M28_001 §1.2: Emit a child span under the root run span.
fn emitAgentSpan(
    run_ctx: worker_stage_types.RunContext,
    root_span_id: [trace_mod.SPAN_ID_HEX_LEN]u8,
    stage_id: []const u8,
    actor_label: []const u8,
    result: agents.AgentResult,
    start_ns: u64,
) void {
    const end_ns: u64 = @intCast(std.time.nanoTimestamp());
    var tc: trace_mod.TraceContext = undefined;
    const tid_len = @min(run_ctx.trace_id.len, trace_mod.TRACE_ID_HEX_LEN);
    @memcpy(tc.trace_id[0..tid_len], run_ctx.trace_id[0..tid_len]);
    if (tid_len < trace_mod.TRACE_ID_HEX_LEN) @memset(tc.trace_id[tid_len..], '0');
    const child = trace_mod.TraceContext.generate();
    tc.span_id = child.span_id;
    // M28_001 §1.2: parent is the root run span, not null.
    tc.parent_span_id = root_span_id;

    var span = otel_traces.buildSpan(tc, "agent.call", start_ns, end_ns);
    // M27_002 obs: run/workspace/agent/stage context for Tempo drill-down.
    _ = otel_traces.addAttr(&span, "run.id", run_ctx.run_id);
    _ = otel_traces.addAttr(&span, "workspace.id", run_ctx.workspace_id);
    _ = otel_traces.addAttr(&span, "agent.id", run_ctx.agent_id);
    _ = otel_traces.addAttr(&span, "stage.id", stage_id);
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

/// Resolve the system prompt for a binding.
/// Custom skills may carry a pre-loaded prompt_content — use it first.
/// Default skills (echo/scout/warden) fall through to actor-dispatch on PromptFiles.
fn resolveSystemPrompt(binding: agents.RoleBinding, prompts: *const agents.PromptFiles) []const u8 {
    if (binding.prompt_content) |pc| return pc;
    return switch (binding.actor) {
        .echo => prompts.echo,
        .scout => prompts.scout,
        .warden => prompts.warden,
        .orchestrator => "",
    };
}

fn resolveBinding(cfg: ExecuteConfig, role_id: []const u8, skill_id: []const u8) ?agents.RoleBinding {
    return agents.resolveRoleWithRegistry(cfg.skill_registry, role_id, skill_id);
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

    // M28_001 §1.1: Root span for the entire run — all agent/gate spans are children.
    const run_start_ns: u64 = @intCast(std.time.nanoTimestamp());
    const root_tc = trace_mod.TraceContext.generate();
    var run_outcome_label: []const u8 = RUN_OUTCOME_ERROR;

    var run_arena = std.heap.ArenaAllocator.init(alloc);
    defer run_arena.deinit();
    const run_alloc = run_arena.allocator();

    var scoring_state = scoring.ScoringState{};
    var total_wall_seconds: u64 = 0;
    var total_tokens: u64 = 0;
    // M28_001 §1.3: Close root span on every exit path with final attrs.
    defer {
        const run_end_ns: u64 = @intCast(std.time.nanoTimestamp());
        var root_span = otel_traces.buildSpan(root_tc, "run.execute", run_start_ns, run_end_ns);
        _ = otel_traces.addAttr(&root_span, "run.id", ctx.run_id);
        _ = otel_traces.addAttr(&root_span, "workspace.id", ctx.workspace_id);
        _ = otel_traces.addAttr(&root_span, "agent.id", ctx.agent_id);
        var attempt_buf: [10]u8 = undefined;
        const attempt_str = std.fmt.bufPrint(&attempt_buf, "{d}", .{ctx.attempt}) catch "0";
        _ = otel_traces.addAttr(&root_span, "run.attempt", attempt_str);
        _ = otel_traces.addAttr(&root_span, "run.outcome", run_outcome_label);
        var tok_buf: [20]u8 = undefined;
        const tok_str = std.fmt.bufPrint(&tok_buf, "{d}", .{total_tokens}) catch "0";
        _ = otel_traces.addAttr(&root_span, "run.total_tokens", tok_str);
        var wall_buf: [20]u8 = undefined;
        const wall_str = std.fmt.bufPrint(&wall_buf, "{d}", .{total_wall_seconds}) catch "0";
        _ = otel_traces.addAttr(&root_span, "run.total_wall_seconds", wall_str);
        otel_traces.enqueueSpan(root_span);
    }
    defer {
        // M27_001: populate resource metrics from executor before scoring.
        scoring_state.resource_metrics.wall_ms = total_wall_seconds * 1000;
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

    // M16_004: Multi-step LLM API key resolution (workspace BYOK → platform default → CredentialDenied).
    const llm_api_key = try resolveLlmApiKey(run_alloc, conn, ctx.workspace_id);

    // M16_003 §2: Fetch GitHub installation token before stages that need git access.
    // Token is per-run, held in memory, never written to Postgres.
    // On failure, classify as policy_deny (UZ-CRED-002) and transition run to BLOCKED.
    const github_token: []const u8 = if (ctx.github_installation_id.len > 0) blk: {
        break :blk token_cache.getInstallationToken(run_alloc, ctx.github_installation_id) catch |err| {
            log.err(
                "worker.github_token_failed workspace_id={s} installation_id={s} err={s} error_code={s}",
                .{ ctx.workspace_id, ctx.github_installation_id, @errorName(err), error_codes.ERR_CRED_GITHUB_TOKEN_FAILED },
            );
            return worker_runtime.WorkerError.CredentialDenied;
        };
    } else "";

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
        // M27_001: extract resource metrics from executor session before destroy.
        if (cfg.executor) |exec| {
            if (exec.getUsage(eid)) |usage| {
                scoring_state.resource_metrics.peak_memory_bytes = usage.memory_peak_bytes;
                scoring_state.resource_metrics.cpu_throttled_ms = usage.cpu_throttled_ms;
                scoring_state.resource_metrics.memory_limit_bytes = usage.memory_limit_bytes;
            } else |_| {}
            exec.destroyExecution(eid) catch {};
        }
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
        .llm_api_key = llm_api_key,
        .github_token = github_token,
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
    metrics.addAgentTokensByActor(plan_binding.actor, plan_result.token_count);
    metrics.wsAddTokens(ctx.workspace_id, plan_result.token_count);
    metrics.observeAgentDurationSeconds(plan_result.wall_seconds);
    emitAgentSpan(ctx, root_tc.span_id, plan_stage.stage_id, plan_binding.actor.label(), plan_result, plan_stage_start_ns);

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
                .llm_api_key = llm_api_key,
                .github_token = github_token,
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
            metrics.addAgentTokensByActor(binding.actor, stage_result.token_count);
            metrics.wsAddTokens(ctx.workspace_id, stage_result.token_count);
            metrics.observeAgentDurationSeconds(stage_result.wall_seconds);
            emitAgentSpan(ctx, root_tc.span_id, stage.stage_id, binding.actor.label(), stage_result, stage_start_ns);
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
                run_outcome_label = RUN_OUTCOME_DONE;
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
                        // M17_001 §1.2: repair loop cap from DB claim (overrides profile default).
                        .max_repair_loops = ctx.max_repair_loops,
                        .gate_tool_timeout_ms = cfg.gate_tool_timeout_ms,
                        .repair_stage_id = repair_stage.stage_id,
                        .repair_role_id = repair_stage.role_id,
                        .repair_skill_id = repair_stage.skill_id,
                        // M17_001 §3.2: Redis client for cancel signal polling.
                        .redis = cfg.redis,
                        // M17_001 §1.2: per-run limits
                        .max_tokens = ctx.max_tokens,
                        .max_wall_time_seconds = ctx.max_wall_time_seconds,
                        .run_created_at_ms = ctx.run_created_at_ms,
                        .attempt = ctx.attempt,
                        .root_span_id = root_tc.span_id,
                        .trace_id = ctx.trace_id,
                    });
                    defer {
                        for (gate_outcome.results.items) |r| r.deinit(run_alloc);
                        gate_outcome.results.deinit(run_alloc);
                    }
                    if (!gate_outcome.all_passed and gate_outcome.state_written) {
                        run_outcome_label = RUN_OUTCOME_CANCELLED;
                        scoring_state.outcome = .cancelled;
                        billing.finalizeRunForBilling(
                            run_alloc,
                            conn,
                            ctx.workspace_id,
                            ctx.run_id,
                            ctx.attempt,
                            .non_billable,
                        ) catch |err| {
                            log.warn("pipeline.billing_finalize_fail run_id={s} err={s}", .{ ctx.run_id, @errorName(err) });
                        };
                        return;
                    }
                    if (!gate_outcome.all_passed) {
                        run_outcome_label = RUN_OUTCOME_BLOCKED;
                        try worker_stage_outcomes.handleGateExhaustedOutcome(.{
                            .alloc = run_alloc,
                            .conn = conn,
                            .ctx = ctx,
                            .cfg = cfg,
                            .gate_results = gate_outcome.results.items,
                            .total_repair_loops = gate_outcome.total_repair_loops,
                            .attempt = attempt,
                            .total_wall_seconds = total_wall_seconds,
                            .scoring_state = &scoring_state,
                        });
                        return;
                    }
                    // Call handleDoneOutcome inside the if block so gate results
                    // are accessed before defer frees the ArrayList backing.
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
                        .gate_results = gate_outcome.results.items,
                        .gate_loop_count = gate_outcome.total_repair_loops,
                    });
                    return;
                }
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
                run_outcome_label = RUN_OUTCOME_BLOCKED;
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

    run_outcome_label = RUN_OUTCOME_BLOCKED;
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

// ── M16_004 integration tests — resolveLlmApiKey ─────────────────────────────
// Three tests covering the complete key resolution chain:
//   1. Workspace BYOK key found → return it.
//   2. No workspace key, platform_llm_keys row exists → return admin workspace key.
//   3. No key anywhere → CredentialDenied.
//
// Require DATABASE_URL (with full schema applied). Skipped when not available.
// ENCRYPTION_MASTER_KEY is set to a deterministic test value by setupTestKek().
//
// All DB writes are wrapped in BEGIN/ROLLBACK so nothing is committed.
// vault.secrets FK (→ core.workspaces) is satisfied by insertTestFixtures().
// platform_llm_keys is a TEMP TABLE that shadows the permanent table for the
// duration of the test connection (unqualified name, pg_temp is first in path).

const _test_tenant_id = "00000000-0000-4000-8000-000000000001";
const _test_ws_id = "00000000-0000-4000-8000-000000000002";
const _test_admin_ws_id = "00000000-0000-4000-8000-000000000003";

fn testOpenConn(alloc: std.mem.Allocator) !?struct { pool: *pg.Pool, conn: *pg.Conn } {
    const db = @import("../db/pool.zig");
    const url = std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const opts = db.parseUrl(arena.allocator(), url) catch return null;
    const pool = pg.Pool.init(alloc, opts) catch return null;
    errdefer pool.deinit();
    const conn = pool.acquire() catch {
        pool.deinit();
        return null;
    };
    return .{ .pool = pool, .conn = conn };
}

fn testSetKek() void {
    const c = @cImport(@cInclude("stdlib.h"));
    _ = c.setenv("ENCRYPTION_MASTER_KEY", "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20", 1);
}

fn testInsertFixtures(conn: *pg.Conn) !void {
    _ = try conn.exec(
        "INSERT INTO core.tenants (tenant_id, name, api_key_hash, created_at) VALUES ($1, 'test-tenant', 'x', 0) ON CONFLICT (tenant_id) DO NOTHING",
        .{_test_tenant_id},
    );
    _ = try conn.exec(
        \\INSERT INTO core.workspaces (workspace_id, tenant_id, repo_url, default_branch, created_at, updated_at)
        \\VALUES ($1, $2, 'https://example.com/test', 'main', 0, 0) ON CONFLICT (workspace_id) DO NOTHING
    , .{ _test_ws_id, _test_tenant_id });
    _ = try conn.exec(
        \\INSERT INTO core.workspaces (workspace_id, tenant_id, repo_url, default_branch, created_at, updated_at)
        \\VALUES ($1, $2, 'https://example.com/admin', 'main', 0, 0) ON CONFLICT (workspace_id) DO NOTHING
    , .{ _test_admin_ws_id, _test_tenant_id });
}

fn testCreatePlatformKeysTable(conn: *pg.Conn) !void {
    // Temp table shadows core.platform_llm_keys for unqualified queries in this session.
    _ = try conn.exec(
        \\CREATE TEMP TABLE IF NOT EXISTS platform_llm_keys (
        \\    provider TEXT NOT NULL,
        \\    source_workspace_id TEXT NOT NULL,
        \\    active BOOLEAN NOT NULL
        \\) ON COMMIT DELETE ROWS
    , .{});
}

test "M16_004: resolveLlmApiKey returns workspace BYOK key" {
    const alloc = std.testing.allocator;
    const db_ctx = (try testOpenConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    testSetKek();

    const conn = db_ctx.conn;
    _ = try conn.exec("BEGIN", .{});
    defer _ = conn.exec("ROLLBACK", .{}) catch {};

    try testInsertFixtures(conn);
    try testCreatePlatformKeysTable(conn);
    try crypto_store.store(alloc, conn, _test_ws_id, "anthropic_api_key", "sk-byok-test", 1);

    const key = try resolveLlmApiKey(alloc, conn, _test_ws_id);
    try std.testing.expectEqualStrings("sk-byok-test", key);
}

test "M16_004: resolveLlmApiKey falls through to platform default" {
    const alloc = std.testing.allocator;
    const db_ctx = (try testOpenConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    testSetKek();

    const conn = db_ctx.conn;
    _ = try conn.exec("BEGIN", .{});
    defer _ = conn.exec("ROLLBACK", .{}) catch {};

    try testInsertFixtures(conn);
    try testCreatePlatformKeysTable(conn);
    // No workspace key; platform row points to admin workspace.
    _ = try conn.exec(
        "INSERT INTO platform_llm_keys (provider, source_workspace_id, active) VALUES ('anthropic', $1, true)",
        .{_test_admin_ws_id},
    );
    try crypto_store.store(alloc, conn, _test_admin_ws_id, "anthropic_api_key", "sk-platform-test", 1);

    const key = try resolveLlmApiKey(alloc, conn, _test_ws_id);
    try std.testing.expectEqualStrings("sk-platform-test", key);
}

test "M16_004: resolveLlmApiKey returns CredentialDenied with no key" {
    const alloc = std.testing.allocator;
    const db_ctx = (try testOpenConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    testSetKek();

    const conn = db_ctx.conn;
    _ = try conn.exec("BEGIN", .{});
    defer _ = conn.exec("ROLLBACK", .{}) catch {};

    try testCreatePlatformKeysTable(conn);
    // No fixtures, no vault entry, no platform row.
    try std.testing.expectError(
        worker_runtime.WorkerError.CredentialDenied,
        resolveLlmApiKey(alloc, conn, _test_ws_id),
    );
}
