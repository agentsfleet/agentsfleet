// Agent config JSON parser.
//
// Parses the `config_json` value (server-derived from TRIGGER.md
// frontmatter) into a AgentConfig. The runtime keys (`triggers`, `tools`,
// `credentials`, `network`, `budget`, `gates`) live under the `x-agentsfleet:`
// top-level object; `name` is the only top-level field outside that block.
// Field parsers take the runtime ObjectMap (the inside of `x-agentsfleet:`),
// not the root.
//
// Decomposed into per-field helpers so every function stays ≤50 lines and
// so errdefer chains free partial state on mid-parse failure (see
// ZIG_RULES "Struct Init Partial Leak").

const std = @import("std");
const Allocator = std.mem.Allocator;

const config_types = @import("config_types.zig");
const config_gates = @import("config_gates.zig");
const helpers = @import("config_helpers.zig");
const validate = @import("config_validate.zig");

const AgentConfig = config_types.AgentConfig;
const AgentConfigError = config_types.AgentConfigError;
const AgentTrigger = config_types.AgentTrigger;
const AgentNetwork = config_types.AgentNetwork;
const AgentBudget = config_types.AgentBudget;
const AgentContextBudget = config_types.AgentContextBudget;

const freeStringSlice = config_types.freeStringSlice;
const freeAgentTrigger = config_types.freeAgentTrigger;

/// Parse `config_json` into a AgentConfig. Caller owns the result and
/// must call `.deinit(alloc)`. On failure, every field allocated up to
/// the failure point is freed via the errdefer chain.
const S_CONTEXT = "context";
const S_CONTEXT_CAP_TOKENS = "context_cap_tokens";
const S_NETWORK = "network";
const S_TRIGGERS = "triggers";
const S_SKILL = "skill";
const S_BUDGET = "budget";
const S_GATES = "gates";
const S_MEMORY_CHECKPOINT_EVERY = "memory_checkpoint_every";
const S_TOOLS = "tools";
const S_CREDENTIALS = "credentials";
const S_STAGE_CHUNK_THRESHOLD = "stage_chunk_threshold";
const S_TOOL_WINDOW = "tool_window";
const S_MODEL = "model";

pub fn parseAgentConfig(
    alloc: Allocator,
    config_json: []const u8,
) (Allocator.Error || AgentConfigError)!AgentConfig {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, config_json, .{}) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return AgentConfigError.MissingRequiredField,
    };
    defer parsed.deinit();

    const root = switch (parsed.value) {
        .object => |o| o,
        else => return AgentConfigError.MissingRequiredField,
    };

    try ensureRuntimeKeysNotAtTopLevel(root);
    const runtime = try extractRuntimeBlock(root);
    try ensureKnownRuntimeKeys(runtime);

    const name = try parseNameField(alloc, root);
    errdefer alloc.free(name);

    const triggers = try parseTriggersField(alloc, runtime);
    errdefer {
        for (triggers) |t| freeAgentTrigger(alloc, t);
        alloc.free(triggers);
    }

    const tools = try parseToolsField(alloc, runtime);
    errdefer freeStringSlice(alloc, tools);

    const credentials = try parseCredentialsField(alloc, runtime);
    errdefer freeStringSlice(alloc, credentials);

    const network = try parseNetworkField(alloc, runtime);
    errdefer if (network) |net| freeStringSlice(alloc, net.allow);

    const budget = try parseBudgetField(runtime);
    const gates = try parseGatesField(alloc, runtime);
    errdefer if (gates) |g| config_gates.freeGatePolicy(alloc, g);

    try validate.validateCredentials(credentials);

    const skill = try parseSkillRef(alloc, runtime);
    errdefer if (skill) |s| alloc.free(s);

    const model = try parseModelField(alloc, runtime);
    errdefer if (model) |s| alloc.free(s);
    const ctx = try parseContextField(runtime);

    return AgentConfig{
        .name = name,
        .triggers = triggers,
        .tools = tools,
        .credentials = credentials,
        .network = network,
        .budget = budget,
        .gates = gates,
        .skill = skill,
        .model = model,
        .context = ctx,
    };
}

/// Runtime keys must live under `x-agentsfleet:`. Their presence at the top
/// level is a structural error pointing the author at the schema doc.
/// Forbidden set must mirror the `known` set in `ensureKnownRuntimeKeys` —
/// any key that's accepted under `x-agentsfleet:` must also be rejected at
/// top level. Otherwise an author who forgets the indentation gets a
/// silently-dropped key (e.g. `gates:` at root → no rate limiting installed,
/// no error surfaced).
fn ensureRuntimeKeysNotAtTopLevel(root: std.json.ObjectMap) AgentConfigError!void {
    const forbidden = [_][]const u8{
        S_TRIGGERS, S_TOOLS, S_CREDENTIALS, S_NETWORK, S_BUDGET,
        S_GATES,    S_SKILL, S_MODEL,       S_CONTEXT,
    };
    for (forbidden) |k| {
        if (root.get(k) != null) return AgentConfigError.RuntimeKeysOutsideBlock;
    }
}

/// Extract the `x-agentsfleet:` runtime block from the parsed JSON root.
/// Distinguished from `MissingRequiredField` because the user fix is different:
/// they need to add a whole namespaced block, not just one missing key.
fn extractRuntimeBlock(root: std.json.ObjectMap) AgentConfigError!std.json.ObjectMap {
    const val = root.get("x-agentsfleet") orelse return AgentConfigError.UseagentBlockRequired;
    return switch (val) {
        .object => |o| o,
        else => AgentConfigError.UseagentBlockRequired,
    };
}

/// Rigid: any subkey under `x-agentsfleet:` outside the known set is an
/// authoring error. Typos must fail loud.
fn ensureKnownRuntimeKeys(runtime: std.json.ObjectMap) AgentConfigError!void {
    const known = [_][]const u8{
        S_TRIGGERS, S_TOOLS, S_CREDENTIALS, S_NETWORK, S_BUDGET,
        S_GATES,    S_SKILL, S_MODEL,       S_CONTEXT,
    };
    var it = runtime.iterator();
    while (it.next()) |entry| {
        var found = false;
        for (known) |k| if (std.mem.eql(u8, k, entry.key_ptr.*)) {
            found = true;
            break;
        };
        if (!found) return AgentConfigError.UnknownRuntimeKey;
    }
}

fn parseNameField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || AgentConfigError)![]const u8 {
    const val = root.get("name") orelse return AgentConfigError.MissingRequiredField;
    const s = switch (val) {
        .string => |str| str,
        else => return AgentConfigError.MissingRequiredField,
    };
    if (s.len == 0) return AgentConfigError.MissingRequiredField;
    try validate.validateSkillName(s);
    return try alloc.dupe(u8, s);
}

fn parseTriggersField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || AgentConfigError)![]const AgentTrigger {
    const val = root.get(S_TRIGGERS) orelse return AgentConfigError.MissingRequiredField;
    const arr = switch (val) {
        .array => |a| a,
        else => return AgentConfigError.MissingRequiredField,
    };
    return helpers.parseAgentTriggers(alloc, arr.items);
}

fn parseToolsField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || AgentConfigError)![]const []const u8 {
    const val = root.get(S_TOOLS) orelse return AgentConfigError.MissingRequiredField;
    const arr = switch (val) {
        .array => |a| a,
        else => return AgentConfigError.MissingRequiredField,
    };
    return try helpers.dupeStringArray(alloc, arr.items);
}

fn parseCredentialsField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || AgentConfigError)![]const []const u8 {
    const val = root.get(S_CREDENTIALS) orelse return try alloc.alloc([]const u8, 0);
    const arr = switch (val) {
        .array => |a| a,
        else => return AgentConfigError.MissingRequiredField,
    };
    return try helpers.dupeStringArray(alloc, arr.items);
}

fn parseNetworkField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || AgentConfigError)!?AgentNetwork {
    const val = root.get(S_NETWORK) orelse return null;
    const obj = switch (val) {
        .object => |o| o,
        else => return AgentConfigError.MissingRequiredField,
    };
    return try helpers.parseAgentNetwork(alloc, obj);
}

fn parseBudgetField(root: std.json.ObjectMap) AgentConfigError!AgentBudget {
    const val = root.get(S_BUDGET) orelse return AgentConfigError.MissingRequiredField;
    const obj = switch (val) {
        .object => |o| o,
        else => return AgentConfigError.MissingRequiredField,
    };
    return helpers.parseAgentBudget(obj);
}

fn parseGatesField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || AgentConfigError)!?config_gates.GatePolicy {
    const val = root.get(S_GATES) orelse return null;
    const obj = switch (val) {
        .object => |o| o,
        else => return AgentConfigError.MissingRequiredField,
    };
    return config_gates.parseGatePolicy(alloc, obj) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return AgentConfigError.MissingRequiredField,
    };
}

fn parseSkillRef(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || AgentConfigError)!?[]const u8 {
    const val = root.get(S_SKILL) orelse return null;
    const s = switch (val) {
        .string => |str| str,
        else => return null,
    };
    if (s.len == 0) return null;
    return try alloc.dupe(u8, s);
}

/// Opaque pass-through. Empty string → null (self-managed sentinel; the runner
/// resolves the model from `tenant_providers` at trigger time).
fn parseModelField(
    alloc: Allocator,
    runtime: std.json.ObjectMap,
) (Allocator.Error || AgentConfigError)!?[]const u8 {
    const val = runtime.get(S_MODEL) orelse return null;
    const s = switch (val) {
        .string => |str| str,
        else => return AgentConfigError.InvalidFieldType,
    };
    if (s.len == 0) return null;
    return try alloc.dupe(u8, s);
}

/// Optional `x-agentsfleet.context:` block. Every field zero-defaults so the
/// runner's `ContextBudget.applyDefaults` can substitute auto-sentinel values.
/// Absent block → null; present-but-empty block → all-zero struct (still
/// gets defaulted downstream — same observable behaviour).
fn parseContextField(runtime: std.json.ObjectMap) AgentConfigError!?AgentContextBudget {
    const val = runtime.get(S_CONTEXT) orelse return null;
    const obj = switch (val) {
        .object => |o| o,
        else => return AgentConfigError.InvalidFieldType,
    };
    try ensureKnownContextKeys(obj);
    return AgentContextBudget{
        .context_cap_tokens = try readU32(obj, S_CONTEXT_CAP_TOKENS),
        .tool_window = try readU32(obj, S_TOOL_WINDOW),
        .memory_checkpoint_every = try readU32(obj, S_MEMORY_CHECKPOINT_EVERY),
        .stage_chunk_threshold = try readF32(obj, S_STAGE_CHUNK_THRESHOLD),
    };
}

/// Same rigid contract as `ensureKnownRuntimeKeys` but for the nested
/// `x-agentsfleet.context:` object. Without this, a typo like
/// `tool_windw: 30` silently falls through to the zero auto-sentinel
/// and the operator's intended override is dropped at runtime — the
/// failure is invisible until somebody traces a confusing budget at
/// runtime back to a misspelled key in frontmatter.
fn ensureKnownContextKeys(ctx: std.json.ObjectMap) AgentConfigError!void {
    const known = [_][]const u8{
        S_CONTEXT_CAP_TOKENS,      S_TOOL_WINDOW,
        S_MEMORY_CHECKPOINT_EVERY, S_STAGE_CHUNK_THRESHOLD,
    };
    var it = ctx.iterator();
    while (it.next()) |entry| {
        var found = false;
        for (known) |k| if (std.mem.eql(u8, k, entry.key_ptr.*)) {
            found = true;
            break;
        };
        if (!found) return AgentConfigError.UnknownRuntimeKey;
    }
}

fn readU32(obj: std.json.ObjectMap, key: []const u8) AgentConfigError!u32 {
    const v = obj.get(key) orelse return 0;
    return switch (v) {
        .integer => |i| blk: {
            if (i < 0 or i > std.math.maxInt(u32)) return AgentConfigError.InvalidFieldType;
            break :blk @intCast(i);
        },
        // Authoring convenience: `tool_window: auto` (bare YAML string) maps to
        // the zero-value auto-sentinel. Same observable behaviour as omitting
        // the key, but keeps the template self-documenting.
        .string => |s| if (std.mem.eql(u8, s, "auto")) 0 else return AgentConfigError.InvalidFieldType,
        else => return AgentConfigError.InvalidFieldType,
    };
}

fn readF32(obj: std.json.ObjectMap, key: []const u8) AgentConfigError!f32 {
    const v = obj.get(key) orelse return 0.0;
    return switch (v) {
        .float => |f| @floatCast(f),
        .integer => |i| @floatFromInt(i),
        else => return AgentConfigError.InvalidFieldType,
    };
}
