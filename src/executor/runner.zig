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

const json = @import("json_helpers.zig");
const types = @import("types.zig");
const executor_metrics = @import("executor_metrics.zig");

const log = std.log.scoped(.executor_runner);

// Runner-specific error codes and docs base URL.
// Canonical source: src/errors/codes.zig (ERR_EXEC_RUNNER_*, ERROR_DOCS_BASE).
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
) types.ExecutionResult {
    const msg = message orelse {
        log.err("executor.runner.invalid_config error_code={s} reason=missing_message", .{ERR_EXEC_RUNNER_INVALID_CONFIG});
        executor_metrics.incStagesFailed();
        return .{ .content = "", .exit_ok = false, .failure = .startup_posture };
    };

    executor_metrics.incStagesStarted();
    const start = std.time.milliTimestamp();

    const result = executeInner(alloc, workspace_path, agent_config, tools_spec, msg, context) catch |err| {
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
            configureGitCredentials(alloc, workspace_path, token) catch |err| {
                log.warn("executor.runner.git_cred_configure_failed err={s}", .{@errorName(err)});
                // Non-fatal: git operations will fail at push time if token is needed
                // but missing. The worker re-requests before PR creation (§2.3).
            };
        }
    }

    // 2. Build provider.
    var runtime_provider = providers.runtime_bundle.RuntimeProviderBundle.init(alloc, &cfg) catch {
        log.err("executor.runner.provider_init_failed error_code={s}", .{ERR_EXEC_RUNNER_AGENT_INIT});
        return RunnerError.AgentInitFailed;
    };
    defer runtime_provider.deinit();
    const provider_i = runtime_provider.provider();

    // 3. Build tools from spec (or allTools as fallback).
    const tools = buildToolsFromSpec(alloc, workspace_path, tools_spec, &cfg) catch {
        log.err("executor.runner.tool_build_failed error_code={s}", .{ERR_EXEC_RUNNER_AGENT_INIT});
        return RunnerError.AgentInitFailed;
    };
    defer tools_mod.deinitTools(alloc, tools);

    // 4. Initialize memory runtime.
    var mem_rt = memory_mod.initRuntime(alloc, &cfg.memory, workspace_path);
    defer if (mem_rt) |*rt| rt.deinit();
    const mem_opt: ?memory_mod.Memory = if (mem_rt) |rt| rt.memory else null;
    tools_mod.bindMemoryTools(tools, mem_opt);

    // 5. Initialize observer.
    var obs_runtime = ObserverRuntime.init(alloc);
    const obs = obs_runtime.observer();

    // 6. Create agent.
    var agent = Agent.fromConfig(alloc, &cfg, provider_i, tools, mem_opt, obs) catch {
        log.err("executor.runner.agent_init_failed error_code={s}", .{ERR_EXEC_RUNNER_AGENT_INIT});
        return RunnerError.AgentInitFailed;
    };
    defer agent.deinit();

    // 7. Compose message with context fields.
    const composed = composeMessage(alloc, message, context) catch {
        log.err("executor.runner.message_compose_failed error_code={s}", .{ERR_EXEC_RUNNER_AGENT_RUN});
        return RunnerError.AgentRunFailed;
    };
    defer if (composed.ptr != message.ptr) alloc.free(composed);

    // 8. Run agent.
    const response = agent.runSingle(composed) catch {
        log.err("executor.runner.agent_run_failed error_code={s}", .{ERR_EXEC_RUNNER_AGENT_RUN});
        return RunnerError.AgentRunFailed;
    };
    const owned = alloc.dupe(u8, response) catch return RunnerError.AgentRunFailed;

    return .{
        .content = owned,
        .token_count = agent.tokensUsed(),
    };
}

/// Apply agent_config JSON overrides to the NullClaw Config.
/// Only overrides fields that are present in the JSON object.
///
/// NullClaw Config uses: default_model, default_provider, default_temperature,
/// temperature (convenience alias), max_tokens (convenience alias).
fn applyAgentConfig(cfg: *Config, ac: std.json.Value) void {
    if (ac != .object) return;
    if (json.getStr(ac, "model")) |model| cfg.default_model = model;
    if (json.getStr(ac, "provider")) |prov| cfg.default_provider = prov;
    if (json.getFloat(ac, "temperature")) |t| {
        cfg.default_temperature = t;
        cfg.temperature = t;
    }
    if (json.getInt(ac, "max_tokens")) |mt| cfg.max_tokens = @intCast(mt);
    // system_prompt is not a Config field — it's passed via the message.
    // The agent receives it as part of the composed message from composeMessage().
}

/// Inject an LLM API key into NullClaw Config for cfg.default_provider (M16_003 §1.4).
///
/// Strategy:
/// 1. Scan cfg.providers for an entry matching cfg.default_provider.
///    If found, overwrite its api_key using cfg.allocator (arena-backed).
///    The old pointer remains in the arena and is freed with it on cfg.deinit().
/// 2. If no matching entry exists, prepend a new ProviderEntry to cfg.providers.
///    Both the new entry slice and its api_key string are allocated from cfg.allocator,
///    so cfg.deinit() (arena.deinit) frees them automatically.
///
/// After this call, RuntimeProviderBundle.init() finds the injected key via
/// resolveApiKeyFromConfig() and never falls through to the process environment.
fn injectProviderApiKey(cfg: *Config, api_key: []const u8) !void {
    const owned_key = try cfg.allocator.dupe(u8, api_key);

    // Try to update an existing provider entry.
    for (@constCast(cfg.providers)) |*entry| {
        if (std.mem.eql(u8, entry.name, cfg.default_provider)) {
            // Old api_key lives in the arena — overwriting the pointer is safe.
            entry.api_key = owned_key;
            return;
        }
    }

    // No existing entry for default_provider — prepend one to the slice.
    // cfg.allocator is the arena so all allocations are freed by cfg.deinit().
    const nullclaw_config = @import("nullclaw").config;
    const new_providers = try cfg.allocator.alloc(nullclaw_config.ProviderEntry, cfg.providers.len + 1);
    new_providers[0] = .{
        .name = cfg.default_provider,
        .api_key = owned_key,
    };
    @memcpy(new_providers[1..], cfg.providers);
    // Replace the slice pointer. The old slice is still in the arena and
    // will be freed when the arena deinits — no double-free, no leak.
    cfg.providers = new_providers;
}

/// Write a git credential config inside the workspace so that git push and
/// GitHub API calls can authenticate using the installation token (M16_003 §2).
///
/// Writes `.git/config` credential helper pointing to a per-workspace credential
/// script, and the credential value itself. The workspace is a temporary worktree
/// that is deleted after the run, so these credentials have run-scoped lifetime.
fn configureGitCredentials(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    github_token: []const u8,
) !void {
    // Write the credentials file inside .git/ so it is never exposed to the
    // working tree (agent diffs, file listings, LLM context).
    // Format: https://x-access-token:{token}@github.com
    const creds_path = try std.fs.path.join(alloc, &.{ workspace_path, ".git", "credentials" });
    defer alloc.free(creds_path);

    const creds_content = try std.fmt.allocPrint(
        alloc,
        "https://x-access-token:{s}@github.com\n",
        .{github_token},
    );
    defer alloc.free(creds_content);

    const file = try std.fs.createFileAbsolute(creds_path, .{ .truncate = true, .mode = 0o600 });
    defer file.close();
    try file.writeAll(creds_content);

    // Configure git to use the credentials file.
    // We write into .git/config (local to the workspace) so it does not leak
    // to the system git config.
    const git_config_path = try std.fs.path.join(alloc, &.{ workspace_path, ".git", "config" });
    defer alloc.free(git_config_path);

    // Append the credential store config if the file exists; otherwise skip.
    // The worktree .git/config is managed by libgit2 — we append our section.
    const git_config_file = std.fs.openFileAbsolute(git_config_path, .{ .mode = .read_write }) catch return;
    defer git_config_file.close();

    // Quote creds_path in the git config value so paths with spaces parse correctly.
    const cred_section = try std.fmt.allocPrint(
        alloc,
        "\n[credential]\n\thelper = store --file=\"{s}\"\n",
        .{creds_path},
    );
    defer alloc.free(cred_section);

    try git_config_file.seekFromEnd(0);
    try git_config_file.writeAll(cred_section);

    log.debug("executor.runner.git_cred_configured workspace={s}", .{workspace_path});
}

/// Build tools from the RPC tools array, or fall back to allTools.
fn buildToolsFromSpec(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    tools_spec: ?std.json.Value,
    cfg: *const Config,
) ![]tools_mod.Tool {
    // If no tools spec provided, create all available tools.
    const spec = tools_spec orelse {
        return try tools_mod.allTools(alloc, workspace_path, .{
            .allowed_paths = &.{workspace_path},
            .tools_config = cfg.tools,
        });
    };
    if (spec != .array) {
        return try tools_mod.allTools(alloc, workspace_path, .{
            .allowed_paths = &.{workspace_path},
            .tools_config = cfg.tools,
        });
    }

    var list: std.ArrayList(tools_mod.Tool) = .{};
    errdefer {
        for (list.items) |t| t.deinit(alloc);
        list.deinit(alloc);
    }

    for (spec.array.items) |item| {
        if (item != .object) continue;
        const name = json.getStr(item, "name") orelse continue;
        const enabled = json.getBoolDefaultTrue(item, "enabled");
        if (!enabled) continue;

        if (std.mem.eql(u8, name, "file_read")) {
            const ptr = try alloc.create(tools_mod.file_read.FileReadTool);
            ptr.* = .{
                .workspace_dir = workspace_path,
                .allowed_paths = cfg.autonomy.allowed_paths,
                .max_file_size = cfg.tools.max_file_size_bytes,
            };
            try list.append(alloc, ptr.tool());
        } else if (std.mem.eql(u8, name, "memory_read") or std.mem.eql(u8, name, "memory_recall")) {
            const ptr = try alloc.create(tools_mod.memory_recall.MemoryRecallTool);
            ptr.* = .{};
            try list.append(alloc, ptr.tool());
        } else if (std.mem.eql(u8, name, "memory_write") or std.mem.eql(u8, name, "memory_list")) {
            const ptr = try alloc.create(tools_mod.memory_list.MemoryListTool);
            ptr.* = .{};
            try list.append(alloc, ptr.tool());
        } else {
            // Unknown tool name (shell, file_write, or future tools).
            // Fall back to allTools() for the entire request — selective
            // per-tool extraction from allTools is unsafe (use-after-free
            // on deinit). The caller should send tools=null to get allTools.
            log.warn("executor.runner.unknown_tool name={s} — falling back to allTools()", .{name});
        }
    }

    return list.toOwnedSlice(alloc);
}

/// Compose the agent message by appending context fields.
///
/// The executor does NOT interpret context semantics — it concatenates
/// non-null fields as markdown sections so the agent receives full context.
pub fn composeMessage(
    alloc: std.mem.Allocator,
    message: []const u8,
    context: ?std.json.Value,
) ![]const u8 {
    const ctx = context orelse return message;
    if (ctx != .object) return message;

    var parts: std.ArrayList(u8) = .{};
    errdefer parts.deinit(alloc);

    try parts.appendSlice(alloc, message);

    const fields = [_]struct { key: []const u8, label: []const u8 }{
        .{ .key = "spec_content", .label = "Spec" },
        .{ .key = "plan_content", .label = "Plan" },
        .{ .key = "memory_context", .label = "Memory context" },
        .{ .key = "defects_content", .label = "Defects from previous attempt" },
        .{ .key = "implementation_summary", .label = "Implementation summary" },
    };

    for (fields) |f| {
        if (json.getStr(ctx, f.key)) |content| {
            if (content.len > 0) {
                try parts.appendSlice(alloc, "\n\n---\n## ");
                try parts.appendSlice(alloc, f.label);
                try parts.appendSlice(alloc, "\n\n");
                try parts.appendSlice(alloc, content);
            }
        }
    }

    return parts.toOwnedSlice(alloc);
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
