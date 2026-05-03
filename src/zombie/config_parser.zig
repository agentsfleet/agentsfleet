// Zombie config JSON parser.
//
// Parses the `config_json` value (server-derived from TRIGGER.md
// frontmatter) into a ZombieConfig. The runtime keys (`trigger`, `tools`,
// `credentials`, `network`, `budget`, `gates`) live under the `x-usezombie:`
// top-level object; `name` is the only top-level field outside that block.
// Field parsers take the runtime ObjectMap (the inside of `x-usezombie:`),
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

const ZombieConfig = config_types.ZombieConfig;
const ZombieConfigError = config_types.ZombieConfigError;
const ZombieTrigger = config_types.ZombieTrigger;
const ZombieNetwork = config_types.ZombieNetwork;
const ZombieBudget = config_types.ZombieBudget;
const ZombieContextBudget = config_types.ZombieContextBudget;

const freeStringSlice = config_types.freeStringSlice;
const freeZombieTrigger = config_types.freeZombieTrigger;

/// Parse `config_json` into a ZombieConfig. Caller owns the result and
/// must call `.deinit(alloc)`. On failure, every field allocated up to
/// the failure point is freed via the errdefer chain.
pub fn parseZombieConfig(
    alloc: Allocator,
    config_json: []const u8,
) (Allocator.Error || ZombieConfigError)!ZombieConfig {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, config_json, .{}) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return ZombieConfigError.MissingRequiredField,
    };
    defer parsed.deinit();

    const root = switch (parsed.value) {
        .object => |o| o,
        else => return ZombieConfigError.MissingRequiredField,
    };

    try ensureRuntimeKeysNotAtTopLevel(root);
    const runtime = try extractRuntimeBlock(root);
    try ensureKnownRuntimeKeys(runtime);

    const name = try parseNameField(alloc, root);
    errdefer alloc.free(name);

    const trigger = try parseTriggerField(alloc, runtime);
    errdefer freeZombieTrigger(alloc, trigger);

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

    const extended = try parseExtendedFields(alloc, runtime);
    errdefer if (extended.skill) |s| alloc.free(s);
    errdefer freeStringSlice(alloc, extended.chain);

    const model = try parseModelField(alloc, runtime);
    errdefer if (model) |s| alloc.free(s);
    const ctx = try parseContextField(runtime);

    return ZombieConfig{
        .name = name,
        .trigger = trigger,
        .tools = tools,
        .credentials = credentials,
        .network = network,
        .budget = budget,
        .gates = gates,
        .skill = extended.skill,
        .chain = extended.chain,
        .model = model,
        .context = ctx,
    };
}

/// Runtime keys must live under `x-usezombie:`. Their presence at the top
/// level is a structural error pointing the author at the schema doc.
/// Forbidden set must mirror the `known` set in `ensureKnownRuntimeKeys` —
/// any key that's accepted under `x-usezombie:` must also be rejected at
/// top level. Otherwise an author who forgets the indentation gets a
/// silently-dropped key (e.g. `gates:` at root → no rate limiting installed,
/// no error surfaced).
fn ensureRuntimeKeysNotAtTopLevel(root: std.json.ObjectMap) ZombieConfigError!void {
    const forbidden = [_][]const u8{
        "trigger", "tools",   "credentials", "network", "budget",
        "gates",   "skill",   "chain",       "model",   "context",
    };
    for (forbidden) |k| {
        if (root.get(k) != null) return ZombieConfigError.RuntimeKeysOutsideBlock;
    }
}

/// Extract the `x-usezombie:` runtime block from the parsed JSON root.
/// Distinguished from `MissingRequiredField` because the user fix is different:
/// they need to add a whole namespaced block, not just one missing key.
fn extractRuntimeBlock(root: std.json.ObjectMap) ZombieConfigError!std.json.ObjectMap {
    const val = root.get("x-usezombie") orelse return ZombieConfigError.UsezombieBlockRequired;
    return switch (val) {
        .object => |o| o,
        else => ZombieConfigError.UsezombieBlockRequired,
    };
}

/// Rigid: any subkey under `x-usezombie:` outside the known set is an
/// authoring error. Typos must fail loud.
fn ensureKnownRuntimeKeys(runtime: std.json.ObjectMap) ZombieConfigError!void {
    const known = [_][]const u8{
        "trigger", "tools",   "credentials", "network", "budget",
        "gates",   "skill",   "chain",       "model",   "context",
    };
    var it = runtime.iterator();
    while (it.next()) |entry| {
        var found = false;
        for (known) |k| if (std.mem.eql(u8, k, entry.key_ptr.*)) {
            found = true;
            break;
        };
        if (!found) return ZombieConfigError.UnknownRuntimeKey;
    }
}

fn parseNameField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)![]const u8 {
    const val = root.get("name") orelse return ZombieConfigError.MissingRequiredField;
    const s = switch (val) {
        .string => |str| str,
        else => return ZombieConfigError.MissingRequiredField,
    };
    if (s.len == 0) return ZombieConfigError.MissingRequiredField;
    try validate.validateSkillName(s);
    return try alloc.dupe(u8, s);
}

fn parseTriggerField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)!ZombieTrigger {
    const val = root.get("trigger") orelse return ZombieConfigError.MissingRequiredField;
    const obj = switch (val) {
        .object => |o| o,
        else => return ZombieConfigError.MissingRequiredField,
    };
    return helpers.parseZombieTrigger(alloc, obj);
}

fn parseToolsField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)![]const []const u8 {
    const val = root.get("tools") orelse return ZombieConfigError.MissingRequiredField;
    const arr = switch (val) {
        .array => |a| a,
        else => return ZombieConfigError.MissingRequiredField,
    };
    if (arr.items.len == 0) return ZombieConfigError.MissingRequiredField;
    return try helpers.dupeStringArray(alloc, arr.items);
}

fn parseCredentialsField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)![]const []const u8 {
    const val = root.get("credentials") orelse return try alloc.alloc([]const u8, 0);
    const arr = switch (val) {
        .array => |a| a,
        else => return ZombieConfigError.MissingRequiredField,
    };
    return try helpers.dupeStringArray(alloc, arr.items);
}

fn parseNetworkField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)!?ZombieNetwork {
    const val = root.get("network") orelse return null;
    const obj = switch (val) {
        .object => |o| o,
        else => return ZombieConfigError.MissingRequiredField,
    };
    return try helpers.parseZombieNetwork(alloc, obj);
}

fn parseBudgetField(root: std.json.ObjectMap) ZombieConfigError!ZombieBudget {
    const val = root.get("budget") orelse return ZombieConfigError.MissingRequiredField;
    const obj = switch (val) {
        .object => |o| o,
        else => return ZombieConfigError.MissingRequiredField,
    };
    return helpers.parseZombieBudget(obj);
}

fn parseGatesField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)!?config_gates.GatePolicy {
    const val = root.get("gates") orelse return null;
    const obj = switch (val) {
        .object => |o| o,
        else => return ZombieConfigError.MissingRequiredField,
    };
    return config_gates.parseGatePolicy(alloc, obj) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return ZombieConfigError.MissingRequiredField,
    };
}

const ExtendedFields = struct {
    skill: ?[]const u8,
    chain: []const []const u8,
};

fn parseExtendedFields(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)!ExtendedFields {
    const skill_ref = try parseSkillRef(alloc, root);
    errdefer if (skill_ref) |s| alloc.free(s);
    const chain_arr = try parseChainArray(alloc, root);
    return .{ .skill = skill_ref, .chain = chain_arr };
}

fn parseSkillRef(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)!?[]const u8 {
    const val = root.get("skill") orelse return null;
    const s = switch (val) {
        .string => |str| str,
        else => return null,
    };
    if (s.len == 0) return null;
    return try alloc.dupe(u8, s);
}

fn parseChainArray(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)![]const []const u8 {
    const val = root.get("chain") orelse return try alloc.alloc([]const u8, 0);
    const arr = switch (val) {
        .array => |a| a,
        else => return ZombieConfigError.MissingRequiredField,
    };
    return try helpers.dupeStringArray(alloc, arr.items);
}

/// Opaque pass-through. Empty string → null (BYOK sentinel; the executor
/// resolves the model from `tenant_providers` at trigger time).
fn parseModelField(
    alloc: Allocator,
    runtime: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)!?[]const u8 {
    const val = runtime.get("model") orelse return null;
    const s = switch (val) {
        .string => |str| str,
        else => return ZombieConfigError.InvalidFieldType,
    };
    if (s.len == 0) return null;
    return try alloc.dupe(u8, s);
}

/// Optional `x-usezombie.context:` block. Every field zero-defaults so the
/// executor's `applyContextDefaults` can substitute auto-sentinel values.
/// Absent block → null; present-but-empty block → all-zero struct (still
/// gets defaulted downstream — same observable behaviour).
fn parseContextField(runtime: std.json.ObjectMap) ZombieConfigError!?ZombieContextBudget {
    const val = runtime.get("context") orelse return null;
    const obj = switch (val) {
        .object => |o| o,
        else => return ZombieConfigError.InvalidFieldType,
    };
    try ensureKnownContextKeys(obj);
    return ZombieContextBudget{
        .context_cap_tokens = try readU32(obj, "context_cap_tokens"),
        .tool_window = try readU32(obj, "tool_window"),
        .memory_checkpoint_every = try readU32(obj, "memory_checkpoint_every"),
        .stage_chunk_threshold = try readF32(obj, "stage_chunk_threshold"),
    };
}

/// Same rigid contract as `ensureKnownRuntimeKeys` but for the nested
/// `x-usezombie.context:` object. Without this, a typo like
/// `tool_windw: 30` silently falls through to the zero auto-sentinel
/// and the operator's intended override is dropped at runtime — the
/// failure is invisible until somebody traces a confusing budget at
/// runtime back to a misspelled key in frontmatter.
fn ensureKnownContextKeys(ctx: std.json.ObjectMap) ZombieConfigError!void {
    const known = [_][]const u8{
        "context_cap_tokens", "tool_window",
        "memory_checkpoint_every", "stage_chunk_threshold",
    };
    var it = ctx.iterator();
    while (it.next()) |entry| {
        var found = false;
        for (known) |k| if (std.mem.eql(u8, k, entry.key_ptr.*)) {
            found = true;
            break;
        };
        if (!found) return ZombieConfigError.UnknownRuntimeKey;
    }
}

fn readU32(obj: std.json.ObjectMap, key: []const u8) ZombieConfigError!u32 {
    const v = obj.get(key) orelse return 0;
    return switch (v) {
        .integer => |i| blk: {
            if (i < 0 or i > std.math.maxInt(u32)) return ZombieConfigError.InvalidFieldType;
            break :blk @intCast(i);
        },
        // Authoring convenience: `tool_window: auto` (bare YAML string) maps to
        // the zero-value auto-sentinel. Same observable behaviour as omitting
        // the key, but keeps the template self-documenting.
        .string => |s| if (std.mem.eql(u8, s, "auto")) 0 else return ZombieConfigError.InvalidFieldType,
        else => return ZombieConfigError.InvalidFieldType,
    };
}

fn readF32(obj: std.json.ObjectMap, key: []const u8) ZombieConfigError!f32 {
    const v = obj.get(key) orelse return 0.0;
    return switch (v) {
        .float => |f| @floatCast(f),
        .integer => |i| @floatFromInt(i),
        else => return ZombieConfigError.InvalidFieldType,
    };
}
