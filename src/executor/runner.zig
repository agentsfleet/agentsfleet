//! NullClaw runner module — bridges the executor handler to agent execution.
//!
//! The runner is agent-agnostic: it receives a NullClaw config, tool spec,
//! and message from the RPC layer, builds the runtime, executes the agent,
//! and returns an ExecutionResult. It does NOT know about echo/scout/warden.
//!
//! Sandbox enforcement: the executor process applies Landlock (filesystem) and
//! cgroups (memory/CPU) at the process level. NullClaw's tools run within this
//! sandboxed boundary. SandboxShellTool (bubblewrap) integration is deferred —
//! the process-level sandbox provides equivalent enforcement for the sidecar.
//!
//! Call chain:
//!   handler.handleStartStage()
//!     -> runner.execute(session, agent_config, tools_spec, message, context)
//!       -> Config from params + env defaults
//!       -> build tool set from spec
//!       -> Agent.fromConfig()
//!       -> agent.runSingle(composed_message)
//!       -> capture result
//!     <- ExecutionResult

const std = @import("std");
const nullclaw = @import("nullclaw");

const Config = nullclaw.config.Config;
const Agent = nullclaw.agent.Agent;
const providers = nullclaw.providers;
const tools_mod = nullclaw.tools;
const memory_mod = nullclaw.memory;
const observability = nullclaw.observability;

const build_options = @import("build_options");

const json = @import("json_helpers.zig");
const types = @import("types.zig");
const executor_metrics = @import("executor_metrics.zig");
const tool_bridge = @import("tool_bridge.zig");
const runner_credentials = @import("runner_credentials.zig");
const zombie_memory = @import("zombie_memory.zig");
const runner_helpers = @import("runner_helpers.zig");
const runner_progress = @import("runner_progress.zig");
const runner_harness = @import("runner_harness.zig");
const progress_writer_mod = @import("progress_writer.zig");
const context_budget = @import("context_budget.zig");

const log = std.log.scoped(.executor_runner);

// Runner-specific error codes and docs base URL.
// Canonical source: src/errors/error_registry.zig (ERR_EXEC_RUNNER_*, ERROR_DOCS_BASE).
// Duplicated here because the executor binary is a separate build module —
// it cannot import files outside src/executor/.
const ERROR_DOCS_BASE = "https://docs.usezombie.com/error-codes#";
const ERR_EXEC_RUNNER_AGENT_INIT = "UZ-EXEC-012";
const ERR_EXEC_RUNNER_AGENT_RUN = "UZ-EXEC-013";
const ERR_EXEC_RUNNER_INVALID_CONFIG = "UZ-EXEC-014";

pub const RunnerError = error{
    InvalidConfig,
    AgentInitFailed,
    AgentRunFailed,
    Timeout,
    OutOfMemory,
};

/// Observer runtime for NullClaw — selects backend from env.
const ObserverRuntime = struct {
    backend: ObserverBackend,
    noop: observability.NoopObserver = .{},
    log_observer: observability.LogObserver = .{},
    verbose_observer: observability.VerboseObserver = .{},

    const ObserverBackend = enum { log_backend, noop, verbose };

    fn init(alloc: std.mem.Allocator) ObserverRuntime {
        const raw = std.process.getEnvVarOwned(alloc, "NULLCLAW_OBSERVER") catch return .{ .backend = .log_backend };
        defer alloc.free(raw);
        const backend: ObserverBackend = if (std.ascii.eqlIgnoreCase(raw, "noop"))
            .noop
        else if (std.ascii.eqlIgnoreCase(raw, "verbose"))
            .verbose
        else
            .log_backend;
        return .{ .backend = backend };
    }

    fn observer(self: *ObserverRuntime) observability.Observer {
        return switch (self.backend) {
            .log_backend => self.log_observer.observer(),
            .noop => self.noop.observer(),
            .verbose => self.verbose_observer.observer(),
        };
    }
};

/// Execute a NullClaw agent from RPC parameters.
///
/// This is the main entry point called by the handler. It:
/// 1. Validates required params (message, agent_config with model)
/// 2. Builds a NullClaw Config from env defaults + agent_config overrides
/// 3. Builds tools from the tools spec array
/// 4. Composes the message with context fields
/// 5. Runs the agent synchronously
/// 6. Returns an ExecutionResult with content, tokens, wall time
pub fn execute(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    agent_config: ?std.json.Value,
    tools_spec: ?std.json.Value,
    message: ?[]const u8,
    context: ?std.json.Value,
    progress: ?*const progress_writer_mod,
    policy: ?*const context_budget.ExecutionPolicy,
) types.ExecutionResult {
    // Test-only harness path. Stripped from the production binary because the
    // build option is a comptime-known false there. See runner_harness.zig.
    if (build_options.executor_harness) {
        return runner_harness.execute(alloc, workspace_path, agent_config, tools_spec, message, context, progress);
    }

    const msg = message orelse {
        log.err("executor.runner.invalid_config error_code={s} reason=missing_message", .{ERR_EXEC_RUNNER_INVALID_CONFIG});
        executor_metrics.incStagesFailed();
        return .{ .content = "", .exit_ok = false, .failure = .startup_posture };
    };

    executor_metrics.incStagesStarted();
    const start = std.time.milliTimestamp();

    const result = executeInner(alloc, workspace_path, agent_config, tools_spec, msg, context, progress, policy) catch |err| {
        const elapsed = elapsedSeconds(start);
        executor_metrics.incStagesFailed();
        executor_metrics.observeAgentDurationSeconds(elapsed);
        const failure = mapError(err);
        incFailureMetric(failure);
        log.err("executor.runner.failed error_code={s} err={s} wall_seconds={d}", .{
            errorCodeForFailure(failure), @errorName(err), elapsed,
        });
        return .{ .content = "", .wall_seconds = elapsed, .exit_ok = false, .failure = failure };
    };

    const elapsed = elapsedSeconds(start);
    executor_metrics.incStagesCompleted();
    executor_metrics.addAgentTokens(result.token_count);
    executor_metrics.observeAgentDurationSeconds(elapsed);

    log.info("executor.runner.done exit_ok=true tokens={d} wall_seconds={d}", .{ result.token_count, elapsed });

    return .{
        .content = result.content,
        .token_count = result.token_count,
        .wall_seconds = elapsed,
        .exit_ok = true,
        .failure = null,
    };
}

const InnerResult = struct {
    content: []const u8,
    token_count: u64,
};

fn executeInner(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    agent_config: ?std.json.Value,
    tools_spec: ?std.json.Value,
    message: []const u8,
    context: ?std.json.Value,
    progress: ?*const progress_writer_mod,
    policy: ?*const context_budget.ExecutionPolicy,
) !InnerResult {
    // 1. Build config from env defaults + agent_config overrides.
    var cfg = Config.load(alloc) catch {
        log.err("executor.runner.config_load_failed error_code={s}", .{ERR_EXEC_RUNNER_AGENT_INIT});
        return RunnerError.AgentInitFailed;
    };
    defer cfg.deinit();
    cfg.workspace_dir = workspace_path;

    // Apply agent_config overrides (model, temperature, max_tokens, api_key).
    if (agent_config) |ac| {
        applyAgentConfig(&cfg, ac);
        // M16_003 §1.4: inject api_key from RPC payload into NullClaw Config.
        // This ensures the executor never reads ANTHROPIC_API_KEY (or any other
        // provider key) from the process environment.
        if (json.getStr(ac, "api_key")) |key| {
            injectProviderApiKey(&cfg, key) catch {
                log.err("executor.runner.api_key_inject_failed error_code={s}", .{ERR_EXEC_RUNNER_INVALID_CONFIG});
                return RunnerError.InvalidConfig;
            };
        }
        // M16_003 §2: configure git credentials in workspace for github_token.
        if (json.getStr(ac, "github_token")) |token| {
            runner_credentials.prepareGitCredential(alloc, workspace_path, token) catch |err| {
                log.warn("executor.runner.git_cred_configure_failed err={s}", .{@errorName(err)});
                // Non-fatal: git operations will fail at push time if token is needed
                // but missing. The worker re-requests before PR creation (§2.3).
            };
        }
    }

    // 2. Build provider (real LLM bundle, or stub in zombied-executor-stub).
    var provider_bundle: runner_helpers.ProviderBundle = .{};
    defer provider_bundle.deinit();
    const provider_i = provider_bundle.acquire(alloc, &cfg) catch return RunnerError.AgentInitFailed;

    // 3. Build tools from spec (or allTools as fallback).
    const tools = buildToolsFromSpec(alloc, workspace_path, tools_spec, &cfg, policy) catch {
        log.err("executor.runner.tool_build_failed error_code={s}", .{ERR_EXEC_RUNNER_AGENT_INIT});
        return RunnerError.AgentInitFailed;
    };
    defer tools_mod.deinitTools(alloc, tools);

    // 4. Initialize memory runtime.
    // M14_001: If agent_config carries memory_connection + memory_namespace,
    // use the per-zombie postgres backend (zombie_memory.zig) which sets
    // instance_id correctly for row-level isolation. Otherwise fall back to
    // NullClaw's default ephemeral workspace SQLite.
    var mem_rt: ?memory_mod.MemoryRuntime = blk: {
        if (agent_config) |ac| {
            // NOTE: these JSON field names ("memory_connection", "memory_namespace") are
            // the API contract between the dispatch layer and the executor. If they are
            // ever renamed in the RPC schema, this silently falls back to ephemeral SQLite
            // with no error. Tracked in M14_005 — validate field presence explicitly when
            // the zombiectl config surface is added.
            const mem_conn = json.getStr(ac, "memory_connection") orelse "";
            const mem_ns = json.getStr(ac, "memory_namespace") orelse "";
            if (mem_conn.len > 0 and mem_ns.len > 0) {
                const mem_cfg = types.MemoryBackendConfig{
                    .backend = "postgres",
                    .connection = mem_conn,
                    .namespace = mem_ns,
                };
                mem_cfg.validate() catch |err| {
                    log.warn("executor.runner.memory_config_invalid err={s} falling_back=ephemeral", .{@errorName(err)});
                    break :blk memory_mod.initRuntime(alloc, &cfg.memory, workspace_path);
                };
                break :blk zombie_memory.initRuntime(alloc, &mem_cfg, workspace_path);
            }
        }
        break :blk memory_mod.initRuntime(alloc, &cfg.memory, workspace_path);
    };
    defer if (mem_rt) |*rt| rt.deinit();
    const mem_opt: ?memory_mod.Memory = if (mem_rt) |rt| rt.memory else null;
    tools_mod.bindMemoryTools(tools, mem_opt);

    // 5. Initialize observer. When streaming is enabled (progress != null),
    // wire the runner_progress adapter so NullClaw events surface as
    // ProgressFrame notifications on the StartStage RPC. Otherwise fall
    // back to the noop / log / verbose backend (default for non-streaming
    // call sites and tests).
    var obs_runtime = ObserverRuntime.init(alloc);
    var secrets_list = collectSecrets(agent_config);
    var adapter: runner_progress.Adapter = undefined;
    const checkpoint_every: u32 = if (policy) |p| p.context.memory_checkpoint_every else 0;
    const tool_window: u32 = if (policy) |p| p.context.tool_window else 0;
    const stage_chunk_threshold: f32 = if (policy) |p| p.context.stage_chunk_threshold else 0.0;
    const context_cap_tokens: u32 = if (policy) |p| p.context.context_cap_tokens else 0;
    const obs = if (progress) |w| blk: {
        adapter = .{
            .writer = w,
            .alloc = alloc,
            .secrets = secrets_list[0..],
            .memory_checkpoint_every = checkpoint_every,
            .tool_window = tool_window,
            .stage_chunk_threshold = stage_chunk_threshold,
            .context_cap_tokens = context_cap_tokens,
        };
        break :blk adapter.observer();
    } else obs_runtime.observer();

    // 6. Create agent.
    var agent = Agent.fromConfig(alloc, &cfg, provider_i, tools, mem_opt, obs) catch {
        log.err("executor.runner.agent_init_failed error_code={s}", .{ERR_EXEC_RUNNER_AGENT_INIT});
        return RunnerError.AgentInitFailed;
    };
    defer agent.deinit();

    if (progress != null) {
        const stream_pair = adapter.streamCallback();
        agent.stream_callback = stream_pair.cb;
        agent.stream_ctx = stream_pair.ctx;
    }

    // 7. Compose message with context fields.
    const composed = composeMessage(alloc, message, context) catch {
        log.err("executor.runner.message_compose_failed error_code={s}", .{ERR_EXEC_RUNNER_AGENT_RUN});
        return RunnerError.AgentRunFailed;
    };
    defer if (composed.ptr != message.ptr) alloc.free(composed);

    // 8. Run agent + redact terminal reply (see runner_helpers).
    const response = agent.runSingle(composed) catch {
        log.err("executor.runner.agent_run_failed error_code={s}", .{ERR_EXEC_RUNNER_AGENT_RUN});
        return RunnerError.AgentRunFailed;
    };
    const owned = runner_helpers.redactedFinalReply(alloc, response, &secrets_list) catch return RunnerError.AgentRunFailed;

    return .{
        .content = owned,
        .token_count = agent.tokensUsed(),
    };
}

// Delegate to runner_helpers.zig (split for RULE FLL).
const applyAgentConfig = runner_helpers.applyAgentConfig;
const injectProviderApiKey = runner_helpers.injectProviderApiKey;
const buildToolsFromSpec = runner_helpers.buildToolsFromSpec;
pub const composeMessage = runner_helpers.composeMessage;

/// Collect the known secret values + canonical placeholders from
/// agent_config so the progress adapter can redact them out of tool
/// arguments before they cross the RPC boundary. The returned array is
/// stack-storage; the adapter borrows it for the duration of the run.
/// Secrets with empty values are still returned but the redactor
/// short-circuits on `value.len == 0`.
fn collectSecrets(agent_config: ?std.json.Value) [2]runner_progress.Secret {
    const ac = agent_config orelse return .{
        .{ .value = "", .placeholder = "${secrets.llm.api_key}" },
        .{ .value = "", .placeholder = "${secrets.github.token}" },
    };
    return .{
        .{ .value = json.getStr(ac, "api_key") orelse "", .placeholder = "${secrets.llm.api_key}" },
        .{ .value = json.getStr(ac, "github_token") orelse "", .placeholder = "${secrets.github.token}" },
    };
}

/// Map a runner error to a FailureClass.
pub fn mapError(err: anyerror) types.FailureClass {
    return switch (err) {
        RunnerError.InvalidConfig => .startup_posture,
        RunnerError.AgentInitFailed => .startup_posture,
        RunnerError.Timeout => .timeout_kill,
        RunnerError.OutOfMemory => .oom_kill,
        RunnerError.AgentRunFailed => .executor_crash,
        else => .executor_crash,
    };
}

pub fn incFailureMetric(failure: types.FailureClass) void {
    switch (failure) {
        .oom_kill => executor_metrics.incExecutorOomKills(),
        .timeout_kill => executor_metrics.incExecutorTimeoutKills(),
        .landlock_deny => executor_metrics.incExecutorLandlockDenials(),
        .resource_kill => executor_metrics.incExecutorResourceKills(),
        .lease_expired => executor_metrics.incExecutorLeaseExpired(),
        else => {},
    }
}

pub fn errorCodeForFailure(failure: types.FailureClass) []const u8 {
    return switch (failure) {
        .startup_posture => ERR_EXEC_RUNNER_AGENT_INIT,
        .timeout_kill => "UZ-EXEC-003",
        .oom_kill => "UZ-EXEC-004",
        .executor_crash => ERR_EXEC_RUNNER_AGENT_RUN,
        else => ERR_EXEC_RUNNER_AGENT_RUN,
    };
}

fn elapsedSeconds(start_ms: i64) u64 {
    const elapsed_ms = std.time.milliTimestamp() - start_ms;
    return @as(u64, @intCast(@max(0, elapsed_ms))) / 1000;
}

// Tests live in runner_test.zig and runner_security_test.zig.
