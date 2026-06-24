//! NullClaw runner module — bridges the runner handler to fleet execution.
//!
//! The runner is fleet-agnostic: it receives a NullClaw config, tool spec,
//! and message from the RPC layer, builds the runtime, executes the fleet,
//! and returns an ExecutionResult. It does NOT know about echo/scout/warden.
//!
//! Sandbox enforcement: Landlock (filesystem) and cgroups (memory/CPU) apply
//! at the process level. NullClaw's tools run within this boundary.
//!
//! Call chain:
//!   runner.execute(alloc, workspace_path, fleet_config, tools_spec, message, context, policy)
//!     -> Config from params + env defaults
//!     -> build tool set from spec
//!     -> Fleet.fromConfig()
//!     -> fleet.runSingle(composed_message)
//!     -> capture result
//!   <- ExecutionResult

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const nullclaw = @import("nullclaw");

const Config = nullclaw.config.Config;
const Fleet = nullclaw.agent.Agent;
const tools_mod = nullclaw.tools;
const memory_mod = nullclaw.memory;
const observability = nullclaw.observability;

const json = @import("json_helpers.zig");
const wire = @import("wire.zig");
const types = @import("types.zig");
const inrun_memory = @import("inrun_memory.zig");
const protocol = @import("contract").protocol;
const runner_helpers = @import("runner_helpers.zig");
const runner_progress = @import("runner_progress.zig");
const runner_observer = @import("runner_observer.zig");
const context_budget = @import("context_budget.zig");
const client_errors = @import("client_errors.zig");

const log = logging.scoped(.runner);

const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;
const ERR_EXEC_RUNNER_FLEET_RUN = client_errors.ERR_EXEC_RUNNER_FLEET_RUN;
const ERR_EXEC_RUNNER_INVALID_CONFIG = client_errors.ERR_EXEC_RUNNER_INVALID_CONFIG;

const S_SECRETS_LLM_API_KEY = "${secrets.llm.api_key}";

pub const RunnerError = error{
    InvalidConfig,
    FleetInitFailed,
    FleetRunFailed,
    Timeout,
    OutOfMemory,
};

/// Execute a NullClaw fleet from RPC parameters.
///
/// This is the main entry point called by the handler. It:
/// 1. Validates required params (message, fleet_config with model)
/// 2. Builds a NullClaw Config from env defaults + fleet_config overrides
/// 3. Builds tools from the tools spec array
/// 4. Composes the message with context fields
/// 5. Runs the fleet synchronously
/// 6. Returns an ExecutionResult with content, tokens, wall time
pub fn execute(
    env_map: *const std.process.Environ.Map,
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    fleet_config: ?std.json.Value,
    tools_spec: ?std.json.Value,
    message: ?[]const u8,
    context: ?std.json.Value,
    policy: ?*const context_budget.ExecutionPolicy,
    /// fd to stream live-tail `activity` frames on (`pipe_proto`), or null to
    /// fall back to the env-selected log/noop observer (tests, non-streaming).
    progress_fd: ?std.posix.fd_t,
    /// Prior memory the parent hydrated over the trusted plane; the in-run store
    /// is seeded from it at run start. Empty when there is no prior memory.
    hydrated_memory: []const protocol.MemoryDelta,
) types.ExecutionResult {
    const msg = message orelse {
        log.err("invalid_config", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .reason = "missing_message" });
        return .{ .content = "", .exit_ok = false, .failure = .startup_posture };
    };

    const start = clock.nowMillis();

    const result = executeInner(env_map, alloc, workspace_path, fleet_config, tools_spec, msg, context, policy, progress_fd, hydrated_memory) catch |err| {
        const elapsed = elapsedSeconds(start);
        const failure = mapError(err);
        log.err("runner_execute_failed", .{
            .error_code = errorCodeForFailure(failure),
            .err = @errorName(err),
            .wall_seconds = elapsed,
        });
        return .{ .content = "", .wall_seconds = elapsed, .exit_ok = false, .failure = failure };
    };

    const elapsed = elapsedSeconds(start);

    log.debug("runner_execute_completed", .{ .exit_ok = true, .tokens = result.token_count, .wall_seconds = elapsed });

    return .{
        .content = result.content,
        .token_count = result.token_count,
        .input_tokens = result.input_tokens,
        .output_tokens = result.output_tokens,
        .wall_seconds = elapsed,
        .exit_ok = true,
        .failure = null,
    };
}

const InnerResult = struct {
    content: []const u8,
    token_count: u64,
    input_tokens: u64,
    output_tokens: u64,
};

/// Cumulative split mapping at the engine boundary: prompt-side → input,
/// completion-side → output. Cached-input stays 0 downstream until the fleet
/// layer surfaces cache reads separately from prompt tokens.
pub fn usageSplits(fleet: *const Fleet) struct { input: u64, output: u64 } {
    return .{ .input = fleet.promptTokensUsed(), .output = fleet.completionTokensUsed() };
}

fn executeInner(
    env_map: *const std.process.Environ.Map,
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    fleet_config: ?std.json.Value,
    tools_spec: ?std.json.Value,
    message: []const u8,
    context: ?std.json.Value,
    policy: ?*const context_budget.ExecutionPolicy,
    progress_fd: ?std.posix.fd_t,
    hydrated_memory: []const protocol.MemoryDelta,
) !InnerResult {
    // 1. Build config from env defaults + fleet_config overrides.
    var cfg = Config.load(alloc) catch {
        log.err("config_load_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT });
        return RunnerError.FleetInitFailed;
    };
    defer cfg.deinit();
    cfg.workspace_dir = workspace_path;

    // Apply fleet_config overrides (model, temperature, max_tokens, api_key).
    if (fleet_config) |ac| {
        applyFleetConfig(&cfg, ac);
        // Inject api_key from RPC payload into NullClaw Config so the
        // runner never reads ANTHROPIC_API_KEY (or any other provider
        // key) from the process environment.
        if (json.getStr(ac, wire.api_key)) |key| {
            injectProviderApiKey(&cfg, key) catch {
                log.err("api_key_inject_failed", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG });
                return RunnerError.InvalidConfig;
            };
        }
        // Custom OpenAI-compatible endpoint: pin the dial URL onto the provider
        // entry so nullclaw reaches exactly the host the egress allowlist permits
        // (only present for a `custom:<url>` provider; named providers omit it).
        if (json.getStr(ac, wire.base_url)) |url| {
            injectProviderBaseUrl(&cfg, url) catch {
                log.err("base_url_inject_failed", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG });
                return RunnerError.InvalidConfig;
            };
        }
    }

    // 2. Build provider (real LLM bundle, or a stub in test builds).
    var provider_bundle: runner_helpers.ProviderBundle = .{};
    defer provider_bundle.deinit();
    const provider_i = provider_bundle.acquire(alloc, &cfg) catch return RunnerError.FleetInitFailed;

    // 3. Build tools from spec (or allTools as fallback).
    const tools = buildToolsFromSpec(alloc, workspace_path, tools_spec, &cfg, policy) catch {
        log.err("tool_build_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT });
        return RunnerError.FleetInitFailed;
    };
    defer tools_mod.deinitTools(alloc, tools);

    // 4. Build the NON-durable in-run store (SQLite `:memory:`) and seed it with
    // the memory the parent hydrated over the trusted plane. Durable memory is
    // the control plane's Postgres, written via the parent's runner push — the
    // child holds no DB connection, DSN, or on-disk memory file.
    var mem_rt: ?memory_mod.MemoryRuntime = inrun_memory.initRuntime(alloc, workspace_path);
    defer if (mem_rt) |*rt| rt.deinit();
    const mem_opt: ?memory_mod.Memory = if (mem_rt) |rt| rt.memory else null;
    if (mem_opt) |m| inrun_memory.seed(m, hydrated_memory);
    tools_mod.bindMemoryTools(tools, mem_opt);

    // The capturer flushes the in-run store back to the parent (mid-run on the
    // checkpoint cadence + once at run end). Only meaningful with a progress fd
    // and a live store; null otherwise (tests / non-streaming) → capture no-ops.
    var capturer = makeCapturer(progress_fd, mem_opt, alloc);

    // 5. Observer + live-tail sink. With a progress fd, the redacting Adapter
    // is the fleet's observer AND per-token stream callback so tool-call and
    // response-chunk frames stream to the parent; without one, fall back to the
    // env-selected log/noop observer. `writer`/`adapter` are stack-owned here
    // because the Adapter's observer vtable captures `&adapter` for the run.
    var obs_runtime = runner_observer.init(env_map);
    var secrets_list = collectSecrets(fleet_config);
    // SAFETY: set by selectObserver when progress_fd is present; else unread.
    var writer: runner_progress.ProgressWriter = undefined;
    // SAFETY: set by selectObserver when progress_fd is present; else unread.
    var adapter: runner_progress.Adapter = undefined;
    const obs = selectObserver(progress_fd, obs_runtime.observer(), &writer, &adapter, alloc, &secrets_list);
    // With a live observer, drive mid-run capture off the checkpoint cadence the
    // lease carries (`adapter` is only initialized on the progress-fd path).
    if (progress_fd != null) {
        if (capturer) |*c| adapter.memory_capturer = c;
        if (policy) |p| adapter.memory_checkpoint_every = p.context.memory_checkpoint_every;
    }

    // 6. Create fleet.
    var fleet = Fleet.fromConfig(alloc, &cfg, provider_i, tools, mem_opt, obs) catch {
        log.err("fleet_init_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT });
        return RunnerError.FleetInitFailed;
    };
    defer fleet.deinit();
    if (progress_fd != null) {
        const sc = adapter.streamCallback();
        fleet.stream_callback = sc.cb;
        fleet.stream_ctx = sc.ctx;
        adapter.fleet = &fleet; // usage frames read the cumulative split accessors
    }

    // 7. Compose message with context fields.
    const composed = composeMessage(alloc, message, context) catch {
        log.err("message_compose_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_RUN });
        return RunnerError.FleetRunFailed;
    };
    defer if (composed.ptr != message.ptr) alloc.free(composed);

    // 8. Run fleet + redact terminal reply (see runner_helpers).
    const response = fleet.runSingle(composed) catch {
        log.err("fleet_run_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_RUN });
        return RunnerError.FleetRunFailed;
    };
    const owned = runner_helpers.redactedFinalReply(alloc, response, &secrets_list) catch return RunnerError.FleetRunFailed;

    // Run-end capture: flush the final memory state so a run that wrote memory
    // without crossing a mid-run checkpoint is still persisted by the parent.
    if (capturer) |*c| c.capture();
    // Terminal usage frame — covers any token fold after the last metric emit.
    if (progress_fd != null) adapter.emitUsage();

    const splits = usageSplits(&fleet);
    return .{
        .content = owned,
        .token_count = fleet.tokensUsed(),
        .input_tokens = splits.input,
        .output_tokens = splits.output,
    };
}

/// Build the in-run memory capturer iff there is both a progress fd to write the
/// `.memory` frame on and a live store to read. Null otherwise — capture is a
/// no-op on the non-streaming/test path and when the store failed to build.
fn makeCapturer(
    progress_fd: ?std.posix.fd_t,
    mem_opt: ?memory_mod.Memory,
    alloc: std.mem.Allocator,
) ?inrun_memory.MemoryCapturer {
    const fd = progress_fd orelse return null;
    const mem = mem_opt orelse return null;
    return .{ .mem = mem, .fd = fd, .alloc = alloc };
}

/// Pick the fleet's observer. With a progress fd, init the caller-owned
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
const applyFleetConfig = runner_helpers.applyFleetConfig;
const injectProviderApiKey = runner_helpers.injectProviderApiKey;
const injectProviderBaseUrl = runner_helpers.injectProviderBaseUrl;
const buildToolsFromSpec = runner_helpers.buildToolsFromSpec;
pub const composeMessage = runner_helpers.composeMessage;

/// Collect the known secret values + canonical placeholders from
/// fleet_config so the progress adapter can redact them out of tool
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
pub fn collectSecrets(fleet_config: ?std.json.Value) [1]runner_progress.Secret {
    const ac = fleet_config orelse return .{
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
        RunnerError.FleetInitFailed => .startup_posture,
        RunnerError.Timeout => .timeout_kill,
        RunnerError.OutOfMemory => .oom_kill,
        RunnerError.FleetRunFailed => .runner_crash,
        else => .runner_crash,
    };
}

/// Canonical `FailureClass` → `UZ-EXEC-*` error code. Lives in `client_errors`
/// (colocated with the code constants, kept out of this file for RULE FLL) and
/// re-exported here so call sites and tests keep using `runner.errorCodeForFailure`.
pub const errorCodeForFailure = client_errors.errorCodeForFailure;

fn elapsedSeconds(start_ms: i64) u64 {
    const elapsed_ms = clock.nowMillis() - start_ms;
    return @as(u64, @intCast(@max(0, elapsed_ms))) / std.time.ms_per_s;
}

// Engine test aggregator — these sibling suites are reachable only through
// here (the runner test root is src/runner/main.zig → engine/runner.zig). They
// were orphaned when the cutover deleted runner_test.zig (RULE ORP).
test {
    _ = @import("runner_security_test.zig");
    _ = @import("runner_progress_redact_test.zig");
    _ = inrun_memory; // discovery via the existing import binding (RULE UFS: no re-spelled path)
    _ = @import("runner_progress_memory_test.zig");
    _ = @import("runner_usage_test.zig");
}
