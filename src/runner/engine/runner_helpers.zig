//! Runner helpers — config mutation, tool building, and message composition.
//! Split from runner.zig to keep that file under the 350-line RULE FLL limit.

const std = @import("std");
const logging = @import("log");
const nullclaw = @import("nullclaw");

const Config = nullclaw.config.Config;
const tools_mod = nullclaw.tools;
const providers = nullclaw.providers;

const json = @import("json_helpers.zig");
const wire = @import("wire.zig");
const tool_bridge = @import("tool_bridge.zig");
const context_budget = @import("context_budget.zig");
const credential_request = @import("credential_request.zig");
const runner_progress = @import("runner_progress.zig");
const client_errors = @import("client_errors.zig");

const log = logging.scoped(.runner);
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;
const ERR_TOOL_UNKNOWN = client_errors.ERR_TOOL_UNKNOWN;

const Secret = runner_progress.Secret;

/// Canonical placeholder for the LLM provider api_key — the one secret not keyed
/// by a `secrets_map` name/field. RULE UFS: single source.
const S_SECRETS_LLM_API_KEY = "${secrets.llm.api_key}";
/// Leading bytes of every `${secrets.NAME.FIELD}` placeholder built for a
/// `secrets_map` leaf; kept in lockstep with `secret_substitution`'s grammar.
const S_SECRETS_PLACEHOLDER_PREFIX = "${secrets.";

/// Take ownership of NullClaw's composeFinalReply buffer, redact every
/// known secret value, and return a freshly-allocated, redacted copy.
/// The terminal StageResponse content rides the same RPC channel as
/// progress frames; the redactor must scrub it identically before the
/// bytes leave the runner process.
pub fn redactedFinalReply(
    alloc: std.mem.Allocator,
    response: []const u8,
    secrets: []const runner_progress.Secret,
) ![]const u8 {
    defer alloc.free(response);
    // Fail CLOSED on redaction OOM — returning un-redacted `response` would leak a
    // secret; propagate (caller maps to FleetRunFailed, no content). (M100 §1.)
    const redacted = try runner_progress.redactBytes(alloc, response, secrets);
    defer if (redacted.ptr != response.ptr) alloc.free(redacted);
    return alloc.dupe(u8, redacted);
}

/// Collect every secret VALUE the redactor must scrub: api_key ∪ every leaf in
/// `secrets_map` — the same set `secret_substitution` resolves — so the redaction
/// set equals the substitution set (M100 §1, Invariant 1). Each maps to its
/// `${secrets.NAME.FIELD}` placeholder. The returned `Secret`s are FULLY owned —
/// both `value` and `placeholder` are `alloc`-duped, so the result outlives the
/// caller's JSON and `freeSecrets` releases every field (no borrow footgun).
/// Fails closed.
pub fn collectSecrets(
    alloc: std.mem.Allocator,
    fleet_config: ?std.json.Value,
    secrets_map: ?std.json.Value,
) std.mem.Allocator.Error![]const Secret {
    var list: std.ArrayList(Secret) = .empty;
    errdefer freeSecretsList(alloc, &list);

    // api_key — always a slot (empty value short-circuits redaction; both fields
    // duped so the set is fully owned and freed uniformly).
    const api_key = if (fleet_config) |ac| (json.getStr(ac, wire.api_key) orelse "") else "";
    try list.ensureUnusedCapacity(alloc, 1);
    {
        const val = try alloc.dupe(u8, api_key);
        const ph = alloc.dupe(u8, S_SECRETS_LLM_API_KEY) catch |e| {
            alloc.free(val);
            return e;
        };
        list.appendAssumeCapacity(.{ .value = val, .placeholder = ph });
    }

    // Every tool credential: secrets_map is {name:{field:"value"}}; mirror
    // secret_substitution's traversal. Non-object/non-string shapes are skipped.
    if (secrets_map) |sm| {
        if (sm == .object) {
            var names = sm.object.iterator();
            while (names.next()) |name_e| {
                const cred = name_e.value_ptr.*;
                if (cred != .object) continue;
                var fields = cred.object.iterator();
                while (fields.next()) |field_e| {
                    const value = switch (field_e.value_ptr.*) {
                        .string => |s| s,
                        else => continue,
                    };
                    try list.ensureUnusedCapacity(alloc, 1);
                    const val = try alloc.dupe(u8, value);
                    const ph = std.fmt.allocPrint(alloc, "{s}{s}.{s}}}", .{
                        S_SECRETS_PLACEHOLDER_PREFIX, name_e.key_ptr.*, field_e.key_ptr.*,
                    }) catch |e| {
                        alloc.free(val);
                        return e;
                    };
                    list.appendAssumeCapacity(.{ .value = val, .placeholder = ph });
                }
            }
        }
    }

    return list.toOwnedSlice(alloc);
}

/// Free a `collectSecrets` result: every owned field (value + placeholder), then the slice.
pub fn freeSecrets(alloc: std.mem.Allocator, secrets: []const Secret) void {
    for (secrets) |s| {
        alloc.free(s.value);
        alloc.free(s.placeholder);
    }
    alloc.free(secrets);
}

fn freeSecretsList(alloc: std.mem.Allocator, list: *std.ArrayList(Secret)) void {
    for (list.items) |s| {
        alloc.free(s.value);
        alloc.free(s.placeholder);
    }
    list.deinit(alloc);
}

/// Holds the runtime LLM provider bundle for the fleet loop.
/// `inner` owns the real `RuntimeProviderBundle`.
/// Caller defers `deinit()` to release the optional.
pub const ProviderBundle = struct {
    inner: ?providers.runtime_bundle.RuntimeProviderBundle = null,

    pub fn deinit(self: *@This()) void {
        if (self.inner) |*rp| rp.deinit();
    }

    pub fn acquire(
        self: *@This(),
        alloc: std.mem.Allocator,
        cfg: *Config,
    ) error{FleetInitFailed}!providers.Provider {
        self.inner = providers.runtime_bundle.RuntimeProviderBundle.init(alloc, cfg) catch {
            log.err("provider_init_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT });
            return error.FleetInitFailed;
        };
        return self.inner.?.provider();
    }
};

/// Apply fleet_config JSON overrides to the NullClaw Config.
/// Only overrides fields that are present in the JSON object.
///
/// NullClaw Config uses: default_model, default_provider, default_temperature,
/// temperature (convenience alias), max_tokens (convenience alias).
pub fn applyFleetConfig(cfg: *Config, ac: std.json.Value) void {
    if (ac != .object) return;
    if (json.getStr(ac, wire.model)) |model| cfg.default_model = model;
    if (json.getStr(ac, wire.provider)) |prov| cfg.default_provider = prov;
    if (json.getFloat(ac, wire.temperature)) |t| {
        cfg.default_temperature = t;
        cfg.temperature = t;
    }
    // M100: a tenant-influenced max_tokens that is negative, zero, or
    // >u32max is bad input — cast it safely and ignore an out-of-range/zero
    // value (leave the Config default) rather than @intCast-panicking.
    if (json.getInt(ac, wire.max_tokens)) |mt| {
        if (std.math.cast(u32, mt)) |v| {
            if (v > 0) cfg.max_tokens = v;
        }
    }
    // system_prompt is not a Config field — it's passed via the message.
    // The fleet receives it as part of the composed message from composeMessage().
}

/// Inject an LLM API key into NullClaw Config for cfg.default_provider.
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
    const entry = try ensureProviderEntry(cfg);
    // Old api_key lives in the arena — overwriting the pointer is safe.
    entry.api_key = owned_key;
}

/// Inject a custom OpenAI-compatible endpoint URL onto the ProviderEntry for
/// cfg.default_provider (which the daemon set to `custom:<url>`, so nullclaw's
/// `classifyProvider` routes it to `.compatible_provider` and `getProviderBaseUrl`
/// returns this URL as the dial target). Without this, the compatible path falls
/// back to the `custom:` prefix or the built-in URL table — setting it on the
/// entry is the explicit, audited override. Same arena-ownership contract as
/// `injectProviderApiKey`; safe to call after it (operates on the same entry).
pub fn injectProviderBaseUrl(cfg: *Config, url: []const u8) !void {
    const owned_url = try cfg.allocator.dupe(u8, url);
    const entry = try ensureProviderEntry(cfg);
    entry.base_url = owned_url;
}

/// Find the ProviderEntry for cfg.default_provider, creating (prepending) one if
/// absent. Shared by the api_key + base_url injectors so both mutate the SAME
/// entry. The returned pointer is valid until the next `ensureProviderEntry`
/// call that has to grow the slice (callers use it immediately). All allocations
/// are arena-backed (cfg.allocator) — freed by cfg.deinit(), no double-free.
fn ensureProviderEntry(cfg: *Config) !*nullclaw.config.ProviderEntry {
    for (@constCast(cfg.providers)) |*entry| {
        if (std.mem.eql(u8, entry.name, cfg.default_provider)) return entry;
    }
    const new_providers = try cfg.allocator.alloc(nullclaw.config.ProviderEntry, cfg.providers.len + 1);
    new_providers[0] = .{ .name = cfg.default_provider };
    @memcpy(new_providers[1..], cfg.providers);
    // Replace the slice pointer. The old slice is still in the arena and will be
    // freed when the arena deinits — no double-free, no leak.
    cfg.providers = new_providers;
    return &new_providers[0];
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
    cred_channel: ?credential_request.Channel,
) ![]tools_mod.Tool {
    const spec = tools_spec orelse return tools_mod.allTools(alloc, workspace_path, .{
        .allowed_paths = &.{workspace_path},
        .tools_config = cfg.tools,
    });
    if (spec != .array) return tools_mod.allTools(alloc, workspace_path, .{
        .allowed_paths = &.{workspace_path},
        .tools_config = cfg.tools,
    });

    const result = try tool_bridge.buildTools(alloc, spec, workspace_path, cfg, policy, cred_channel);
    for (result.skipped) |name| {
        log.warn("tool_skipped", .{ .error_code = ERR_TOOL_UNKNOWN, .name = name });
        alloc.free(name);
    }
    alloc.free(result.skipped);
    return result.tools;
}

/// Section label for the installed `SKILL.md` instructions rendered ahead of the
/// trigger event. Named (RULE UFS) — asserted by tests too. The runner fails
/// closed on an empty body upstream (`child_exec`), so this section only renders
/// when instructions are present.
pub const INSTALLED_INSTRUCTIONS_LABEL = "Installed instructions";
/// Blank line between a markdown section heading and its body (RULE UFS).
const HEADING_GAP = "\n\n";

/// Compose the fleet message: the installed `SKILL.md` instructions render
/// FIRST (the fleet's installed behaviour frames the trigger), then the trigger
/// event message, then any appended coding-fleet context sections.
///
/// The runner does NOT interpret context semantics — it concatenates allowlisted
/// fields as markdown sections so the fleet receives full context. Only known
/// keys render, so a secret accidentally placed in `context` never reaches the
/// prompt.
pub fn composeMessage(
    alloc: std.mem.Allocator,
    message: []const u8,
    context: ?std.json.Value,
) ![]const u8 {
    const ctx = context orelse return message;
    if (ctx != .object) return message;

    var parts: std.ArrayList(u8) = .empty;
    errdefer parts.deinit(alloc);

    // Installed instructions lead the prompt (present + non-empty only; the
    // runner fails closed on an empty body before composing, so a no-playbook
    // run never reaches here).
    if (json.getStr(ctx, wire.installed_instructions)) |instr| {
        if (instr.len > 0) {
            try parts.appendSlice(alloc, "## ");
            try parts.appendSlice(alloc, INSTALLED_INSTRUCTIONS_LABEL);
            try parts.appendSlice(alloc, HEADING_GAP);
            try parts.appendSlice(alloc, instr);
            try parts.appendSlice(alloc, "\n\n---\n");
        }
    }

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
                try parts.appendSlice(alloc, HEADING_GAP);
                try parts.appendSlice(alloc, content);
            }
        }
    }

    return parts.toOwnedSlice(alloc);
}

// Tests live in the sibling `runner_helpers_test.zig` (extracted to keep this
// file under the 350-line cap once landed); pull them into the test build.
test {
    _ = @import("runner_helpers_test.zig");
}
