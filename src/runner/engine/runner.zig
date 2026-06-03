//! NullClaw runner module — bridges the runner handler to agent execution.
//!
//! The runner is agent-agnostic: it receives a NullClaw config, tool spec,
//! and message from the RPC layer, builds the runtime, executes the agent,
//! and returns an ExecutionResult. It does NOT know about echo/scout/warden.
//!
//! Sandbox enforcement: Landlock (filesystem) and cgroups (memory/CPU) apply
//! at the process level. NullClaw's tools run within this boundary.
//!
//! Call chain:
//!   runner.execute(alloc, workspace_path, agent_config, tools_spec, message, context, policy)
//!     -> Config from params + env defaults
//!     -> build tool set from spec
//!     -> Agent.fromConfig()
//!     -> agent.runSingle(composed_message)
//!     -> capture result
//!   <- ExecutionResult

const std = @import("std");
const logging = @import("log");
const nullclaw = @import("nullclaw");

const Config = nullclaw.config.Config;
const Agent = nullclaw.agent.Agent;
const tools_mod = nullclaw.tools;
const memory_mod = nullclaw.memory;
const observability = nullclaw.observability;

const json = @import("json_helpers.zig");
const wire = @import("wire.zig");
const types = @import("types.zig");
const zombie_memory = @import("zombie_memory.zig");
const runner_helpers = @import("runner_helpers.zig");
const runner_progress = @import("runner_progress.zig");
const runner_observer = @import("runner_observer.zig");
const context_budget = @import("context_budget.zig");
const client_errors = @import("client_errors.zig");

const log = logging.scoped(.runner);

const ERR_EXEC_RUNNER_AGENT_INIT = client_errors.ERR_EXEC_RUNNER_AGENT_INIT;
const ERR_EXEC_RUNNER_AGENT_RUN = client_errors.ERR_EXEC_RUNNER_AGENT_RUN;
const ERR_EXEC_RUNNER_INVALID_CONFIG = client_errors.ERR_EXEC_RUNNER_INVALID_CONFIG;

const S_SECRETS_LLM_API_KEY = "${secrets.llm.api_key}";

pub const RunnerError = error{
    InvalidConfig,
    AgentInitFailed,
    AgentRunFailed,
    Timeout,
    OutOfMemory,
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
    policy: ?*const context_budget.ExecutionPolicy,
    /// fd to stream live-tail `activity` frames on (`pipe_proto`), or null to
    /// fall back to the env-selected log/noop observer (tests, non-streaming).
    progress_fd: ?std.posix.fd_t,
) types.ExecutionResult {
    const msg = message orelse {
        log.err("invalid_config", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .reason = "missing_message" });
        return .{ .content = "", .exit_ok = false, .failure = .startup_posture };
    };

    const start = std.time.milliTimestamp();

    const result = executeInner(alloc, workspace_path, agent_config, tools_spec, msg, context, policy, progress_fd) catch |err| {
        const elapsed = elapsedSeconds(start);
        const failure = mapError(err);
        log.err("failed", .{
            .error_code = errorCodeForFailure(failure),
            .err = @errorName(err),
            .wall_seconds = elapsed,
        });
        return .{ .content = "", .wall_seconds = elapsed, .exit_ok = false, .failure = failure };
    };

    const elapsed = elapsedSeconds(start);

    log.info("done", .{ .exit_ok = true, .tokens = result.token_count, .wall_seconds = elapsed });

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
    policy: ?*const context_budget.ExecutionPolicy,
    progress_fd: ?std.posix.fd_t,
) !InnerResult {
    // 1. Build config from env defaults + agent_config overrides.
    var cfg = Config.load(alloc) catch {
        log.err("config_load_failed", .{ .error_code = ERR_EXEC_RUNNER_AGENT_INIT });
        return RunnerError.AgentInitFailed;
    };
    defer cfg.deinit();
    cfg.workspace_dir = workspace_path;

    // Apply agent_config overrides (model, temperature, max_tokens, api_key).
    if (agent_config) |ac| {
        applyAgentConfig(&cfg, ac);
        // Inject api_key from RPC payload into NullClaw Config so the
        // runner never reads ANTHROPIC_API_KEY (or any other provider
        // key) from the process environment.
        if (json.getStr(ac, wire.api_key)) |key| {
            injectProviderApiKey(&cfg, key) catch {
                log.err("api_key_inject_failed", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG });
                return RunnerError.InvalidConfig;
            };
        }
    }

    // 2. Build provider (real LLM bundle, or a stub in test builds).
    var provider_bundle: runner_helpers.ProviderBundle = .{};
    defer provider_bundle.deinit();
    const provider_i = provider_bundle.acquire(alloc, &cfg) catch return RunnerError.AgentInitFailed;

    // 3. Build tools from spec (or allTools as fallback).
    const tools = buildToolsFromSpec(alloc, workspace_path, tools_spec, &cfg, policy) catch {
        log.err("tool_build_failed", .{ .error_code = ERR_EXEC_RUNNER_AGENT_INIT });
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
            const mem_conn = json.getStr(ac, wire.memory_connection) orelse "";
            const mem_ns = json.getStr(ac, wire.memory_namespace) orelse "";
            if (mem_conn.len > 0 and mem_ns.len > 0) {
                const mem_cfg = types.MemoryBackendConfig{
                    .backend = "postgres",
                    .connection = mem_conn,
                    .namespace = mem_ns,
                };
                mem_cfg.validate() catch |err| {
                    log.warn("memory_config_invalid", .{ .err = @errorName(err), .falling_back = "ephemeral" });
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

    // 5. Observer + live-tail sink. With a progress fd, the redacting Adapter
    // is the agent's observer AND per-token stream callback so tool-call and
    // response-chunk frames stream to the parent; without one, fall back to the
    // env-selected log/noop observer. `writer`/`adapter` are stack-owned here
    // because the Adapter's observer vtable captures `&adapter` for the run.
    var obs_runtime = runner_observer.init(alloc);
    var secrets_list = collectSecrets(agent_config);
    // SAFETY: set by selectObserver when progress_fd is present; else unread.
    var writer: runner_progress.ProgressWriter = undefined;
    // SAFETY: set by selectObserver when progress_fd is present; else unread.
    var adapter: runner_progress.Adapter = undefined;
    const obs = selectObserver(progress_fd, obs_runtime.observer(), &writer, &adapter, alloc, &secrets_list);

    // 6. Create agent.
    var agent = Agent.fromConfig(alloc, &cfg, provider_i, tools, mem_opt, obs) catch {
        log.err("agent_init_failed", .{ .error_code = ERR_EXEC_RUNNER_AGENT_INIT });
        return RunnerError.AgentInitFailed;
    };
    defer agent.deinit();
    if (progress_fd != null) {
        const sc = adapter.streamCallback();
        agent.stream_callback = sc.cb;
        agent.stream_ctx = sc.ctx;
    }

    // 7. Compose message with context fields.
    const composed = composeMessage(alloc, message, context) catch {
        log.err("message_compose_failed", .{ .error_code = ERR_EXEC_RUNNER_AGENT_RUN });
        return RunnerError.AgentRunFailed;
    };
    defer if (composed.ptr != message.ptr) alloc.free(composed);

    // 8. Run agent + redact terminal reply (see runner_helpers).
    const response = agent.runSingle(composed) catch {
        log.err("agent_run_failed", .{ .error_code = ERR_EXEC_RUNNER_AGENT_RUN });
        return RunnerError.AgentRunFailed;
    };
    const owned = runner_helpers.redactedFinalReply(alloc, response, &secrets_list) catch return RunnerError.AgentRunFailed;

    return .{
        .content = owned,
        .token_count = agent.tokensUsed(),
    };
}

/// Pick the agent's observer. With a progress fd, init the caller-owned
/// `writer`/`adapter` in place (so the returned observer's vtable, which
/// captures `&adapter`, stays valid for the run) and return the redacting
/// Adapter's observer; otherwise the env-selected backend.
fn selectObserver(
    progress_fd: ?std.posix.fd_t,
    fallback: observability.Observer,
    writer: *runner_progress.ProgressWriter,
    adapter: *runner_progress.Adapter,
    alloc: std.mem.Allocator,
    secrets: []const runner_progress.Secret,
) observability.Observer {
    const fd = progress_fd orelse return fallback;
    writer.* = .{ .fd = fd, .alloc = alloc };
    adapter.* = .{ .writer = writer, .alloc = alloc, .secrets = secrets };
    return adapter.observer();
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
///
/// Adding a new credential slot here (e.g. an installation token) is
/// a two-step change: add the wire constant in `wire.zig`, extract it
/// here, and extend the array length. `collectSecrets` is `pub` so the
/// extraction shape is unit-testable from `runner_test.zig` —
/// reviewers should add a row to those tests for every new slot.
pub fn collectSecrets(agent_config: ?std.json.Value) [1]runner_progress.Secret {
    const ac = agent_config orelse return .{
        .{ .value = "", .placeholder = S_SECRETS_LLM_API_KEY },
    };
    return .{
        .{ .value = json.getStr(ac, wire.api_key) orelse "", .placeholder = S_SECRETS_LLM_API_KEY },
    };
}

/// Map a runner error to a FailureClass.
pub fn mapError(err: anyerror) types.FailureClass {
    return switch (err) {
        RunnerError.InvalidConfig => .startup_posture,
        RunnerError.AgentInitFailed => .startup_posture,
        RunnerError.Timeout => .timeout_kill,
        RunnerError.OutOfMemory => .oom_kill,
        RunnerError.AgentRunFailed => .runner_crash,
        else => .runner_crash,
    };
}

pub fn errorCodeForFailure(failure: types.FailureClass) []const u8 {
    return switch (failure) {
        .startup_posture => ERR_EXEC_RUNNER_AGENT_INIT,
        .timeout_kill => client_errors.ERR_EXEC_TIMEOUT_KILL,
        .oom_kill => client_errors.ERR_EXEC_OOM_KILL,
        .runner_crash => ERR_EXEC_RUNNER_AGENT_RUN,
        else => ERR_EXEC_RUNNER_AGENT_RUN,
    };
}

fn elapsedSeconds(start_ms: i64) u64 {
    const elapsed_ms = std.time.milliTimestamp() - start_ms;
    return @as(u64, @intCast(@max(0, elapsed_ms))) / 1000;
}

// Engine test aggregator — these sibling suites are reachable only through
// here (the runner test root is src/runner/main.zig → engine/runner.zig). They
// were orphaned when the cutover deleted runner_test.zig (RULE ORP).
test {
    _ = @import("runner_security_test.zig");
    _ = @import("sandbox_edge_test.zig");
    _ = @import("resource_security_test.zig");
    _ = @import("runner_progress_redact_test.zig");
    _ = @import("session_test.zig");
}
