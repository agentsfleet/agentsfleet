//! Runner helpers — config mutation, tool building, and message composition.
//! Split from runner.zig to keep that file under the 350-line RULE FLL limit.

const std = @import("std");
const nullclaw = @import("nullclaw");
const build_options = @import("build_options");

const Config = nullclaw.config.Config;
const tools_mod = nullclaw.tools;
const providers = nullclaw.providers;

const json = @import("json_helpers.zig");
const tool_bridge = @import("tool_bridge.zig");
const context_budget = @import("context_budget.zig");
const stub_gate = @import("stub_provider_gate.zig");
const runner_progress = @import("runner_progress.zig");

const log = std.log.scoped(.executor_runner);

/// Take ownership of NullClaw's composeFinalReply buffer, redact every
/// known secret value, and return a freshly-allocated, redacted copy.
/// The terminal StageResponse content rides the same RPC channel as
/// progress frames; the redactor must scrub it identically before the
/// bytes leave the executor process.
pub fn redactedFinalReply(
    alloc: std.mem.Allocator,
    response: []const u8,
    secrets: []const runner_progress.Secret,
) ![]const u8 {
    defer alloc.free(response);
    const redacted = runner_progress.redactBytes(alloc, response, secrets) catch response;
    defer if (redacted.ptr != response.ptr) alloc.free(redacted);
    return alloc.dupe(u8, redacted);
}

/// Holds the runtime LLM provider bundle for the agent loop. In the stub
/// binary `inner` stays null and `stub` carries the canned-response provider;
/// in production/harness `inner` owns the real `RuntimeProviderBundle`.
/// Caller defers `deinit()` to release the optional.
pub const ProviderBundle = struct {
    inner: ?providers.runtime_bundle.RuntimeProviderBundle = null,
    stub: stub_gate.Module.StubProvider = undefined,

    pub fn deinit(self: *@This()) void {
        if (self.inner) |*rp| rp.deinit();
    }

    pub fn acquire(
        self: *@This(),
        alloc: std.mem.Allocator,
        cfg: *Config,
    ) error{AgentInitFailed}!providers.Provider {
        self.stub = stub_gate.Module.StubProvider.init(alloc);
        if (build_options.executor_provider_stub) return self.stub.provider();
        self.inner = providers.runtime_bundle.RuntimeProviderBundle.init(alloc, cfg) catch {
            log.err("executor.runner.provider_init_failed error_code=UZ-EXEC-012", .{});
            return error.AgentInitFailed;
        };
        return self.inner.?.provider();
    }
};

/// Apply agent_config JSON overrides to the NullClaw Config.
/// Only overrides fields that are present in the JSON object.
///
/// NullClaw Config uses: default_model, default_provider, default_temperature,
/// temperature (convenience alias), max_tokens (convenience alias).
pub fn applyAgentConfig(cfg: *Config, ac: std.json.Value) void {
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
pub fn injectProviderApiKey(cfg: *Config, api_key: []const u8) !void {
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

/// Build tools from RPC tools array, or fall back to allTools.
/// Unknown names are logged to stderr and collected in BuildResult.skipped.
///
/// `policy` is the session-owned ExecutionPolicy. When non-null, tools that
/// consult per-execution policy (currently only http_request) construct
/// the policy-aware variant. Null is the legitimate path for the
/// `allTools()` fallback (no spec) and for harness/test paths that don't
/// drive the bridge.
pub fn buildToolsFromSpec(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    tools_spec: ?std.json.Value,
    cfg: *const Config,
    policy: ?*const context_budget.ExecutionPolicy,
) ![]tools_mod.Tool {
    const spec = tools_spec orelse return try tools_mod.allTools(alloc, workspace_path, .{
        .allowed_paths = &.{workspace_path},
        .tools_config = cfg.tools,
    });
    if (spec != .array) return try tools_mod.allTools(alloc, workspace_path, .{
        .allowed_paths = &.{workspace_path},
        .tools_config = cfg.tools,
    });

    const result = try tool_bridge.buildTools(alloc, spec, workspace_path, cfg, policy);
    for (result.skipped) |name| {
        log.warn("executor.runner.tool_skipped name={s}", .{name});
        alloc.free(name);
    }
    alloc.free(result.skipped);
    return result.tools;
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

test "redactedFinalReply substitutes the placeholder and frees the input" {
    const alloc = std.testing.allocator;
    const secrets = [_]runner_progress.Secret{
        .{ .value = "sk-leak", .placeholder = "${secrets.llm.api_key}" },
    };
    const input = try alloc.dupe(u8, "hello sk-leak world");
    const out = try redactedFinalReply(alloc, input, &secrets);
    defer alloc.free(out);
    try std.testing.expectEqualStrings("hello ${secrets.llm.api_key} world", out);
}

test "redactedFinalReply with no matching secret still transfers ownership" {
    // Negative-path: when redactBytes returns the input slice unchanged
    // (no hit), the helper must still free `input` and return a fresh
    // copy — caller cannot tell the two paths apart from outside.
    const alloc = std.testing.allocator;
    const secrets = [_]runner_progress.Secret{
        .{ .value = "absent-token", .placeholder = "${secrets.llm.api_key}" },
    };
    const input = try alloc.dupe(u8, "no leak here");
    const out = try redactedFinalReply(alloc, input, &secrets);
    defer alloc.free(out);
    try std.testing.expectEqualStrings("no leak here", out);
    // The std.testing.allocator catches double-free / leak; a defective
    // implementation that returned `input` directly would either leak
    // the dupe or double-free on the caller's defer.
}
