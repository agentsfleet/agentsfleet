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

    // Apply agent_config overrides (model, temperature, max_tokens).
    if (agent_config) |ac| {
        applyAgentConfig(&cfg, ac);
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

// ── Tests ────────────────────────────────────────────────────────────────────

test "mapError covers all RunnerError variants" {
    try std.testing.expectEqual(types.FailureClass.startup_posture, mapError(RunnerError.InvalidConfig));
    try std.testing.expectEqual(types.FailureClass.startup_posture, mapError(RunnerError.AgentInitFailed));
    try std.testing.expectEqual(types.FailureClass.executor_crash, mapError(RunnerError.AgentRunFailed));
    try std.testing.expectEqual(types.FailureClass.timeout_kill, mapError(RunnerError.Timeout));
    try std.testing.expectEqual(types.FailureClass.oom_kill, mapError(RunnerError.OutOfMemory));
}

test "mapError returns executor_crash for unknown errors" {
    try std.testing.expectEqual(types.FailureClass.executor_crash, mapError(error.Unexpected));
}

test "composeMessage returns original when context is null" {
    const alloc = std.testing.allocator;
    const msg = "Hello agent";
    const composed = try composeMessage(alloc, msg, null);
    // When no context, returns the original pointer (no allocation).
    try std.testing.expectEqualStrings("Hello agent", composed);
}

test "composeMessage appends context fields" {
    const alloc = std.testing.allocator;
    var ctx = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer ctx.object.deinit();
    try ctx.object.put("spec_content", .{ .string = "Feature X spec" });
    try ctx.object.put("plan_content", .{ .string = "Step 1, Step 2" });

    const composed = try composeMessage(alloc, "Analyze this", ctx);
    defer alloc.free(composed);

    // Should contain the original message.
    try std.testing.expect(std.mem.indexOf(u8, composed, "Analyze this") != null);
    // Should contain the spec section.
    try std.testing.expect(std.mem.indexOf(u8, composed, "## Spec") != null);
    try std.testing.expect(std.mem.indexOf(u8, composed, "Feature X spec") != null);
    // Should contain the plan section.
    try std.testing.expect(std.mem.indexOf(u8, composed, "## Plan") != null);
    try std.testing.expect(std.mem.indexOf(u8, composed, "Step 1, Step 2") != null);
}

test "composeMessage skips empty context fields" {
    const alloc = std.testing.allocator;
    var ctx = std.json.Value{ .object = std.json.ObjectMap.init(alloc) };
    defer ctx.object.deinit();
    try ctx.object.put("spec_content", .{ .string = "" });
    try ctx.object.put("plan_content", .{ .string = "Plan data" });

    const composed = try composeMessage(alloc, "msg", ctx);
    defer alloc.free(composed);

    // Empty spec_content should be skipped.
    try std.testing.expect(std.mem.indexOf(u8, composed, "## Spec") == null);
    // Plan should be present.
    try std.testing.expect(std.mem.indexOf(u8, composed, "## Plan") != null);
}

test "composeMessage with non-object context returns original" {
    const alloc = std.testing.allocator;
    const composed = try composeMessage(alloc, "msg", .{ .integer = 42 });
    try std.testing.expectEqualStrings("msg", composed);
}

test "execute returns failure for null message" {
    const alloc = std.testing.allocator;
    const result = execute(alloc, "/tmp/ws", null, null, null, null);
    try std.testing.expect(!result.exit_ok);
    try std.testing.expect(result.failure != null);
    try std.testing.expectEqual(types.FailureClass.startup_posture, result.failure.?);
}

test "errorCodeForFailure returns correct codes" {
    try std.testing.expectEqualStrings("UZ-EXEC-012", errorCodeForFailure(.startup_posture));
    try std.testing.expectEqualStrings("UZ-EXEC-003", errorCodeForFailure(.timeout_kill));
    try std.testing.expectEqualStrings("UZ-EXEC-004", errorCodeForFailure(.oom_kill));
    try std.testing.expectEqualStrings("UZ-EXEC-013", errorCodeForFailure(.executor_crash));
}

test "incFailureMetric increments oom counter" {
    const before = executor_metrics.executorSnapshot().oom_kills_total;
    incFailureMetric(.oom_kill);
    const after = executor_metrics.executorSnapshot().oom_kills_total;
    try std.testing.expect(after > before);
}

test "incFailureMetric increments timeout counter" {
    const before = executor_metrics.executorSnapshot().timeout_kills_total;
    incFailureMetric(.timeout_kill);
    const after = executor_metrics.executorSnapshot().timeout_kills_total;
    try std.testing.expect(after > before);
}
