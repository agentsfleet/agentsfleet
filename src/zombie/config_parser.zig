// Zombie config JSON parser. Runtime keys live under `x-usezombie:`; `name`
// is the only top-level field. Per-field helpers keep functions ≤50 lines.

const std = @import("std");
const Allocator = std.mem.Allocator;
const logging = @import("log");

const config_types = @import("config_types.zig");
const config_gates = @import("config_gates.zig");
const helpers = @import("config_helpers.zig");
const validate = @import("config_validate.zig");

const log = logging.scoped(.zombie_config);

const ZombieConfig = config_types.ZombieConfig;
const ZombieConfigError = config_types.ZombieConfigError;
const ZombieTrigger = config_types.ZombieTrigger;
const ZombieNetwork = config_types.ZombieNetwork;
const ZombieBudget = config_types.ZombieBudget;
const ZombieContextBudget = config_types.ZombieContextBudget;

const freeStringSlice = config_types.freeStringSlice;
const freeZombieTrigger = config_types.freeZombieTrigger;

/// Caller owns the result; call `.deinit(alloc)`. errdefer chain frees
/// any field allocated up to a mid-parse failure.
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

    const triggers = try parseTriggersField(alloc, runtime);
    errdefer {
        for (triggers) |t| freeZombieTrigger(alloc, t);
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

    return ZombieConfig{
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

// Forbidden mirrors `ensureKnownRuntimeKeys` — else a missed indent silently drops keys like `gates:` at root.
fn ensureRuntimeKeysNotAtTopLevel(root: std.json.ObjectMap) ZombieConfigError!void {
    const forbidden = [_][]const u8{
        "trigger",     "triggers", "tools", "credentials", "network",
        "budget",      "gates",    "skill", "model",       "context",
    };
    for (forbidden) |k| {
        if (root.get(k) != null) return ZombieConfigError.RuntimeKeysOutsideBlock;
    }
}

// Distinct from `MissingRequiredField` — author fix is to add the whole
// namespaced block, not just one key.
fn extractRuntimeBlock(root: std.json.ObjectMap) ZombieConfigError!std.json.ObjectMap {
    const val = root.get("x-usezombie") orelse return ZombieConfigError.UsezombieBlockRequired;
    return switch (val) {
        .object => |o| o,
        else => ZombieConfigError.UsezombieBlockRequired,
    };
}

// Rigid: typos must fail loud. Singular `trigger:` is intentionally absent
// from `known` — falls through to `UnknownRuntimeKey` (RULE NLG: plural-only).
fn ensureKnownRuntimeKeys(runtime: std.json.ObjectMap) ZombieConfigError!void {
    const known = [_][]const u8{
        "triggers", "tools", "credentials", "network",
        "budget",   "gates", "skill",       "model",   "context",
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
    return alloc.dupe(u8, s);
}

// Enforces length 1..MAX_TRIGGERS_PER_ZOMBIE_LOCAL, ≤1 cron entry, unique
// `(type, source)` tuple across webhooks. Errfree on partial parse.
fn parseTriggersField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)![]const ZombieTrigger {
    const val = root.get("triggers") orelse return ZombieConfigError.MissingRequiredField;
    const arr = switch (val) {
        .array => |a| a,
        else => return ZombieConfigError.MissingRequiredField,
    };
    const items = arr.items;
    if (items.len == 0 or items.len > MAX_TRIGGERS_PER_ZOMBIE_LOCAL) {
        log.warn("triggers_count_out_of_bounds", .{
            .count = items.len,
            .max = MAX_TRIGGERS_PER_ZOMBIE_LOCAL,
        });
        return ZombieConfigError.InvalidFieldType;
    }
    var out = try alloc.alloc(ZombieTrigger, items.len);
    var parsed_count: usize = 0;
    errdefer {
        for (out[0..parsed_count]) |t| freeZombieTrigger(alloc, t);
        alloc.free(out);
    }
    var cron_count: usize = 0;
    for (items, 0..) |item, idx| {
        const obj = switch (item) {
            .object => |o| o,
            else => return ZombieConfigError.InvalidFieldType,
        };
        const trig = try helpers.parseZombieTrigger(alloc, obj);
        out[idx] = trig;
        parsed_count += 1;
        if (trig == .cron) {
            cron_count += 1;
            if (cron_count > 1) {
                log.warn("multiple_cron_triggers_rejected", .{});
                return ZombieConfigError.InvalidTriggerType;
            }
        }
        for (out[0..idx]) |existing| {
            if (std.meta.activeTag(existing) != std.meta.activeTag(trig)) continue;
            const conflict = switch (trig) {
                .webhook => |w| std.mem.eql(u8, existing.webhook.source, w.source),
                .cron => false, // ≤1-cron rule handles this above
                .api => true,   // api rejected at parseZombieTrigger; unreachable
            };
            if (conflict) {
                log.warn("duplicate_trigger_tuple", .{ .index = idx });
                return ZombieConfigError.InvalidTriggerType;
            }
        }
    }
    return out;
}

// Identifier matches `config_helpers.MAX_TRIGGERS_PER_ZOMBIE` (RULE UFS).
const MAX_TRIGGERS_PER_ZOMBIE_LOCAL: usize = 8;

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
    return helpers.dupeStringArray(alloc, arr.items);
}

fn parseCredentialsField(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)![]const []const u8 {
    const val = root.get("credentials") orelse return alloc.alloc([]const u8, 0);
    const arr = switch (val) {
        .array => |a| a,
        else => return ZombieConfigError.MissingRequiredField,
    };
    return helpers.dupeStringArray(alloc, arr.items);
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

// Empty string → null (self-managed sentinel; executor resolves from
// `tenant_providers` at trigger time).
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

// Zero-defaults — `ContextBudget.applyDefaults` substitutes auto-sentinels
// downstream.
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

// Typos must fail loud — otherwise the intended override silently zeroes.
fn ensureKnownContextKeys(ctx: std.json.ObjectMap) ZombieConfigError!void {
    const known = [_][]const u8{
        "context_cap_tokens",      "tool_window",
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
        // `tool_window: auto` (bare YAML string) maps to the zero auto-sentinel.
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
