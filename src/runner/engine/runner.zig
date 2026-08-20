//! NullClaw runner module — bridges the runner handler to fleet execution.
//!
//! The runner is fleet-agnostic: it receives a NullClaw config, tool spec,
//! and message from the RPC layer, builds the runtime, executes the fleet,
//! and returns an ExecutionResult. It does NOT know about echo/scout/warden.
//!
//! Sandbox enforcement: Landlock (filesystem) and cgroups (memory/CPU) apply
//! at the process level. NullClaw's tools run within this boundary.
//!
//! Call chain: execute() → Config from params+env → build tool set from spec →
//! Fleet.fromConfig() → fleet.runSingle(composed_message) → ExecutionResult.

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const nullclaw = @import("nullclaw");

const Config = nullclaw.config.Config;
const Fleet = nullclaw.agent.Agent;
const tools_mod = nullclaw.tools;
const memory_mod = nullclaw.memory;

const json = @import("json_helpers.zig");
const wire = @import("wire.zig");
const types = @import("types.zig");
const inrun_memory = @import("inrun_memory.zig");
const protocol = @import("contract").protocol;
const runner_helpers = @import("runner_helpers.zig");
const runner_progress = @import("runner_progress.zig");
const runner_observer = @import("runner_observer.zig");
const runner_capture = @import("runner_capture.zig");
const context_budget = @import("context_budget.zig");
const client_errors = @import("client_errors.zig");
const run_context = @import("run_context.zig");
const credential_request = @import("credential_request.zig");

const log = logging.scoped(.runner);

const DETAIL_MISSING_MESSAGE = "lease carried no message to run";

const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;
const ERR_EXEC_RUNNER_FLEET_RUN = client_errors.ERR_EXEC_RUNNER_FLEET_RUN;
const ERR_EXEC_RUNNER_INVALID_CONFIG = client_errors.ERR_EXEC_RUNNER_INVALID_CONFIG;

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
    /// The child→runner on-demand mint channel (M102 §4), or null when none was
    /// wired (tests). Forwarded to the tool bridge for tool-boundary minting.
    cred_channel: ?credential_request.Channel,
) types.ExecutionResult {
    const msg = message orelse {
        log.err("invalid_config", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .reason = "missing_message" });
        return .{ .outcome = .{ .failed = .{ .class = .startup_posture, .detail = DETAIL_MISSING_MESSAGE } } };
    };

    const start = clock.nowMillis();

    const result = executeInner(.{ .cred_channel = cred_channel }, env_map, alloc, workspace_path, fleet_config, tools_spec, msg, context, policy, progress_fd, hydrated_memory) catch |err| {
        const elapsed = elapsedSeconds(start);
        const failure = mapError(err);
        log.err("runner_execute_failed", .{
            .error_code = errorCodeForFailure(failure),
            .err = @errorName(err),
            .wall_seconds = elapsed,
        });
        return .{ .wall_seconds = elapsed, .outcome = .{ .failed = .{ .class = failure, .detail = @errorName(err) } } };
    };

    const elapsed = elapsedSeconds(start);

    log.debug("runner_execute_completed", .{ .exit_ok = true, .tokens = result.token_count, .wall_seconds = elapsed });

    return .{
        .content = result.content,
        .token_count = result.token_count,
        .input_tokens = result.input_tokens,
        .output_tokens = result.output_tokens,
        .wall_seconds = elapsed,
        .outcome = .{ .completed = .{} },
    };
}

/// Record a config-load failure WITH its cause. Split from `executeInner` so the
/// record's shape is drivable in a test: this runs inside the sandboxed child,
/// where the usual fault is an environment the cage did not carry (`NoHomeDir`
/// when the daemon itself has no HOME). Dropping the error name leaves the
/// journal showing a code and nothing else, which is the difference between
/// reading the fault and reproducing it on the host to find it.
fn logConfigLoadFailure(err: anyerror) void {
    log.err("config_load_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err) });
}

pub const InnerResult = struct {
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

/// `pub` for `run_context_test.zig` — the DI seam (M100) makes this path
/// drivable against an injected stub provider, offline.
pub fn executeInner(
    deps: run_context.RunDeps,
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
    var cfg = Config.load(alloc) catch |err| {
        logConfigLoadFailure(err);
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

    // 2. Build provider through the injectable seam (M100): production wires the
    // real LLM bundle; a test injects a stub to drive this path offline. The
    // bundle is owned here regardless — a stub leaves it empty so deinit no-ops.
    var provider_bundle: runner_helpers.ProviderBundle = .{};
    defer provider_bundle.deinit();
    const provider_i = deps.acquireProvider(alloc, &cfg, &provider_bundle) catch return RunnerError.FleetInitFailed;

    // 3. Build tools from the declared spec — no fallback; absent means zero.
    const tools = buildToolsFromSpec(alloc, workspace_path, tools_spec, &cfg, policy, deps.cred_channel) catch {
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
    var capturer = runner_capture.makeCapturer(progress_fd, mem_opt, alloc);

    // 5. Observer + live-tail sink. With a progress fd, the redacting Adapter
    // is the fleet's observer AND per-token stream callback so tool-call and
    // response-chunk frames stream to the parent; without one, fall back to the
    // env-selected log/noop observer. `writer`/`adapter` are stack-owned here
    // because the Adapter's observer vtable captures `&adapter` for the run.
    var obs_runtime = runner_observer.init(env_map);
    // Redaction set = api_key ∪ every secrets_map leaf (the same set the tool
    // substitutor resolves into outbound HTTP), so no resolved secret can ride a
    // frame/reply un-redacted. Fail closed on OOM — never run with an incomplete
    // redaction set. (M100 §1.)
    const secrets_map: ?std.json.Value = if (policy) |p| p.secrets_map else null;
    const secrets_list = collectSecrets(alloc, fleet_config, secrets_map) catch {
        log.err("secret_collection_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT });
        return RunnerError.FleetInitFailed;
    };
    defer freeSecrets(alloc, secrets_list);
    // SAFETY: set by selectObserver when progress_fd is present; else unread.
    var writer: runner_progress.ProgressWriter = undefined;
    // SAFETY: set by selectObserver when progress_fd is present; else unread.
    var adapter: runner_progress.Adapter = undefined;
    const obs = runner_capture.selectObserver(progress_fd, obs_runtime.observer(), &writer, &adapter, alloc, secrets_list);
    defer if (progress_fd != null) adapter.deinit(alloc); // progress-fd path only inits adapter
    // With a live observer, drive mid-run capture off the checkpoint cadence the
    // lease carries (`adapter` is only initialized on the progress-fd path).
    if (progress_fd != null) {
        if (capturer) |*c| adapter.memory_capturer = c;
        if (policy) |p| adapter.memory_checkpoint_every = p.context.memory_checkpoint_every;
    }

    // 6. Create fleet.
    // Mapped error kept — the raw error would lose `.startup_posture` to `mapError`'s `.runner_crash` fallback.
    var fleet = Fleet.fromConfig(alloc, &cfg, provider_i, tools, mem_opt, obs) catch |err| {
        log.err("fleet_init_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err) });
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
    const composed = composeMessage(alloc, message, context) catch |err| {
        log.err("message_compose_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_RUN, .err = @errorName(err) });
        return err;
    };
    defer if (composed.ptr != message.ptr) alloc.free(composed);

    // 8. Run fleet + redact terminal reply (see runner_helpers).
    // True error propagates — `mapError`'s `else` arm yields the same `.runner_crash` class, so only the detail changes.
    const response = fleet.runSingle(composed) catch |err| {
        log.err("fleet_run_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_RUN, .err = @errorName(err) });
        return err;
    };
    const owned = try runner_helpers.redactedFinalReply(alloc, response, secrets_list);

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
// Delegate to runner_helpers.zig (split for RULE FLL).
const applyFleetConfig = runner_helpers.applyFleetConfig;
const injectProviderApiKey = runner_helpers.injectProviderApiKey;
const injectProviderBaseUrl = runner_helpers.injectProviderBaseUrl;
const buildToolsFromSpec = runner_helpers.buildToolsFromSpec;
pub const composeMessage = runner_helpers.composeMessage;

/// Build the wire-redaction secret set (api_key ∪ every `secrets_map` leaf) and
/// free it. Defined in runner_helpers (RULE FLL) and re-exported so call sites
/// and tests keep using `runner.collectSecrets` / `runner.freeSecrets`. (M100 §1.)
pub const collectSecrets = runner_helpers.collectSecrets;
pub const freeSecrets = runner_helpers.freeSecrets;

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

test "test_config_load_failure_names_error: the record carries the cause, not just the code" {
    // The regression: `Config.load(alloc) catch { log.err(...) }` discarded the
    // error, so a dev fleet where every lease died at init logged only
    // UZ-EXEC-012 with no cause — and the real fault (no HOME in the daemon's
    // environment) could only be found by reproducing it on the host.
    var bs = logging.sinks.BufferedSink.init(std.testing.allocator);
    defer bs.deinit();

    logging.sinks.clearSinksForTest();
    defer logging.sinks.clearSinksForTest();
    logging.sinks.registerSink(bs.sink());

    logConfigLoadFailure(error.NoHomeDir);

    const captured = try bs.snapshot();
    defer std.testing.allocator.free(captured);
    try std.testing.expect(std.mem.indexOf(u8, captured, "config_load_failed") != null);
    try std.testing.expect(std.mem.indexOf(u8, captured, ERR_EXEC_RUNNER_FLEET_INIT) != null);
    // The assertion that would have failed before the fix.
    try std.testing.expect(std.mem.indexOf(u8, captured, "NoHomeDir") != null);
}
