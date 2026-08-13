// Fleet config JSON parser.
//
// Parses the `config_json` value (server-derived from TRIGGER.md
// frontmatter) into a FleetConfig. The runtime keys (`triggers`, `tools`,
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
const config_repositories = @import("config_repositories.zig");
const config_context = @import("config_context.zig");

const FleetConfig = config_types.FleetConfig;
const FleetConfigError = config_types.FleetConfigError;
const FleetTrigger = config_types.FleetTrigger;
const FleetNetwork = config_types.FleetNetwork;
const FleetBudget = config_types.FleetBudget;

const freeStringSlice = config_types.freeStringSlice;
const freeFleetTrigger = config_types.freeFleetTrigger;

/// Parse `config_json` into an owned FleetConfig. The errdefer chain frees
/// every field allocated before a failure.
const S_CONTEXT = "context";
const S_NETWORK = "network";
const S_TRIGGERS = "triggers";
const S_SKILL = "skill";
const S_BUDGET = "budget";
const S_GATES = "gates";
const S_TOOLS = "tools";
const S_CREDENTIALS = "credentials";
const S_MODEL = "model";
const S_REPOSITORIES = config_repositories.S_REPOSITORIES;
const S_REPOSITORY_ACCESS = config_repositories.S_REPOSITORY_ACCESS;
const S_REPOSITORY_BASE = config_repositories.S_REPOSITORY_BASE;

pub fn parseFleetConfig(
    alloc: Allocator,
    config_json: []const u8,
) (Allocator.Error || FleetConfigError)!FleetConfig {
    return parseConfig(alloc, config_json, .authoring);
}

/// Parse a persisted config. This differs from authoring only for write
/// bindings saved before `repository_base` existed: the incomplete binding is
/// retained so lease preflight can record a typed, actionable refusal.
pub fn parseStoredFleetConfig(
    alloc: Allocator,
    config_json: []const u8,
) (Allocator.Error || FleetConfigError)!FleetConfig {
    return parseConfig(alloc, config_json, .stored);
}

fn parseConfig(
    alloc: Allocator,
    config_json: []const u8,
    mode: config_repositories.ParseMode,
) (Allocator.Error || FleetConfigError)!FleetConfig {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, config_json, .{}) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return FleetConfigError.MissingRequiredField,
    };
    defer parsed.deinit();

    const root = switch (parsed.value) {
        .object => |o| o,
        else => return FleetConfigError.MissingRequiredField,
    };

    try ensureRuntimeKeysNotAtTopLevel(root);
    const runtime = try extractRuntimeBlock(root);
    try ensureKnownRuntimeKeys(runtime);

    const name = try parseNameField(alloc, root);
    errdefer alloc.free(name);

    const triggers = try parseTriggersField(alloc, runtime);
    errdefer {
        for (triggers) |t| freeFleetTrigger(alloc, t);
        alloc.free(triggers);
    }

    const tools = try parseToolsField(alloc, runtime);
    errdefer freeStringSlice(alloc, tools);

    const credentials = try parseCredentialsField(alloc, runtime);
    errdefer freeStringSlice(alloc, credentials);

    const network = try parseNetworkField(alloc, runtime);
    errdefer if (network) |net| {
        freeStringSlice(alloc, net.allow);
        freeStringSlice(alloc, net.read_post_paths);
    };

    const budget = try parseBudgetField(runtime);
    const gates = try parseGatesField(alloc, runtime);
    errdefer if (gates) |g| config_gates.freeGatePolicy(alloc, g);

    const repository_binding = try config_repositories.parse(alloc, runtime, mode);
    errdefer if (repository_binding) |b| {
        freeStringSlice(alloc, b.repositories);
        if (b.base_branch) |base| alloc.free(base);
    };

    try validate.validateCredentials(credentials);

    const skill = try parseSkillRef(alloc, runtime);
    errdefer if (skill) |s| alloc.free(s);

    const model = try parseModelField(alloc, runtime);
    errdefer if (model) |s| alloc.free(s);
    const ctx = try config_context.parse(runtime, S_CONTEXT);

    return FleetConfig{
        .name = name,
        .triggers = triggers,
        .tools = tools,
        .credentials = credentials,
        .network = network,
        .budget = budget,
        .gates = gates,
        .repository_binding = repository_binding,
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
fn ensureRuntimeKeysNotAtTopLevel(root: std.json.ObjectMap) FleetConfigError!void {
    const forbidden = [_][]const u8{
        S_TRIGGERS,          S_TOOLS,           S_CREDENTIALS, S_NETWORK, S_BUDGET,
        S_GATES,             S_SKILL,           S_MODEL,       S_CONTEXT, S_REPOSITORIES,
        S_REPOSITORY_ACCESS, S_REPOSITORY_BASE,
    };
    for (forbidden) |k| {
        if (root.get(k) != null) return FleetConfigError.RuntimeKeysOutsideBlock;
    }
}

/// Extract the `x-agentsfleet:` runtime block from the parsed JSON root.
/// Distinguished from `MissingRequiredField` because the user fix is different:
/// they need to add a whole namespaced block, not just one missing key.
fn extractRuntimeBlock(root: std.json.ObjectMap) FleetConfigError!std.json.ObjectMap {
    const val = root.get("x-agentsfleet") orelse return FleetConfigError.UsefleetBlockRequired;
    return switch (val) {
        .object => |o| o,
        else => FleetConfigError.UsefleetBlockRequired,
    };
}

/// Rigid: any subkey under `x-agentsfleet:` outside the known set is an
/// authoring error. Typos must fail loud.
fn ensureKnownRuntimeKeys(runtime: std.json.ObjectMap) FleetConfigError!void {
    const known = [_][]const u8{
        S_TRIGGERS,          S_TOOLS,           S_CREDENTIALS, S_NETWORK, S_BUDGET,
        S_GATES,             S_SKILL,           S_MODEL,       S_CONTEXT, S_REPOSITORIES,
        S_REPOSITORY_ACCESS, S_REPOSITORY_BASE,
    };
    var it = runtime.iterator();
    while (it.next()) |entry| {
        var found = false;
        for (known) |k| if (std.mem.eql(u8, k, entry.key_ptr.*)) {
            found = true;
            break;
        };
        if (!found) return FleetConfigError.UnknownRuntimeKey;
    }
}

fn parseNameField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || FleetConfigError)![]const u8 {
    const val = root.get("name") orelse return FleetConfigError.MissingRequiredField;
    const s = switch (val) {
        .string => |str| str,
        else => return FleetConfigError.MissingRequiredField,
    };
    if (s.len == 0) return FleetConfigError.MissingRequiredField;
    try validate.validateSkillName(s);
    return try alloc.dupe(u8, s);
}

fn parseTriggersField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || FleetConfigError)![]const FleetTrigger {
    const val = root.get(S_TRIGGERS) orelse return FleetConfigError.MissingRequiredField;
    const arr = switch (val) {
        .array => |a| a,
        else => return FleetConfigError.MissingRequiredField,
    };
    return helpers.parseFleetTriggers(alloc, arr.items);
}

fn parseToolsField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || FleetConfigError)![]const []const u8 {
    const val = root.get(S_TOOLS) orelse return FleetConfigError.MissingRequiredField;
    const arr = switch (val) {
        .array => |a| a,
        else => return FleetConfigError.MissingRequiredField,
    };
    return try helpers.dupeStringArray(alloc, arr.items);
}

fn parseCredentialsField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || FleetConfigError)![]const []const u8 {
    const val = root.get(S_CREDENTIALS) orelse return try alloc.alloc([]const u8, 0);
    const arr = switch (val) {
        .array => |a| a,
        else => return FleetConfigError.MissingRequiredField,
    };
    return try helpers.dupeStringArray(alloc, arr.items);
}

fn parseNetworkField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || FleetConfigError)!?FleetNetwork {
    const val = root.get(S_NETWORK) orelse return null;
    const obj = switch (val) {
        .object => |o| o,
        else => return FleetConfigError.MissingRequiredField,
    };
    return try helpers.parseFleetNetwork(alloc, obj);
}

fn parseBudgetField(root: std.json.ObjectMap) FleetConfigError!FleetBudget {
    const val = root.get(S_BUDGET) orelse return FleetConfigError.MissingRequiredField;
    const obj = switch (val) {
        .object => |o| o,
        else => return FleetConfigError.MissingRequiredField,
    };
    return helpers.parseFleetBudget(obj);
}

fn parseGatesField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || FleetConfigError)!?config_gates.GatePolicy {
    const val = root.get(S_GATES) orelse return null;
    const obj = switch (val) {
        .object => |o| o,
        else => return FleetConfigError.MissingRequiredField,
    };
    return config_gates.parseGatePolicy(alloc, obj) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return FleetConfigError.MissingRequiredField,
    };
}

fn parseSkillRef(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || FleetConfigError)!?[]const u8 {
    const val = root.get(S_SKILL) orelse return null;
    const s = switch (val) {
        .string => |str| str,
        else => return null,
    };
    if (s.len == 0) return null;
    return try alloc.dupe(u8, s);
}

/// Opaque pass-through. Empty string → null (self-managed sentinel; the runner
/// resolves the model from `tenant_model_selection` at trigger time).
fn parseModelField(
    alloc: Allocator,
    runtime: std.json.ObjectMap,
) (Allocator.Error || FleetConfigError)!?[]const u8 {
    const val = runtime.get(S_MODEL) orelse return null;
    const s = switch (val) {
        .string => |str| str,
        else => return FleetConfigError.InvalidFieldType,
    };
    if (s.len == 0) return null;
    return try alloc.dupe(u8, s);
}
