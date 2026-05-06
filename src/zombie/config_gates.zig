// Gate policy parser — parses the "gates" section from Zombie config JSON.
//
// Gate policies define which tool actions require human approval and which
// anomaly patterns trigger auto-kill. Parsed from config_json at claim time.
// Types are re-exported by config.zig for use by approval_gate.zig.

const std = @import("std");
const logging = @import("log");
const Allocator = std.mem.Allocator;
const ec = @import("../errors/error_registry.zig");

const log = logging.scoped(.zombie_config_gates);

pub const GateBehavior = enum { approve, auto_kill };

pub const GateRule = struct {
    tool: []const u8,
    action: []const u8,
    condition: ?[]const u8,
    behavior: GateBehavior,
};

pub const AnomalyPattern = enum {
    same_action,

    fn fromString(s: []const u8) ?AnomalyPattern {
        if (std.mem.eql(u8, s, "same_action")) return .same_action;
        return null;
    }
};

pub const AnomalyRule = struct {
    pattern: AnomalyPattern,
    threshold_count: u32,
    threshold_window_s: u32,
};

pub const GatePolicy = struct {
    rules: []const GateRule,
    anomaly_rules: []const AnomalyRule,
    timeout_ms: u64,
};

const GateConfigError = error{
    MissingRequiredField,
    InvalidBudget,
};

pub fn parseGatePolicy(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || GateConfigError)!GatePolicy {
    const timeout_ms: u64 = blk: {
        const val = obj.get("timeout_ms") orelse break :blk ec.GATE_DEFAULT_TIMEOUT_MS;
        break :blk switch (val) {
            .integer => |i| if (i > 0) @intCast(i) else ec.GATE_DEFAULT_TIMEOUT_MS,
            else => ec.GATE_DEFAULT_TIMEOUT_MS,
        };
    };

    const rules = blk: {
        const val = obj.get("rules") orelse break :blk try alloc.alloc(GateRule, 0);
        const arr = switch (val) {
            .array => |a| a,
            else => return GateConfigError.MissingRequiredField,
        };
        break :blk try parseGateRules(alloc, arr.items);
    };
    errdefer freeGateRules(alloc, rules);

    const anomaly_rules = blk: {
        const val = obj.get("anomaly_rules") orelse break :blk try alloc.alloc(AnomalyRule, 0);
        const arr = switch (val) {
            .array => |a| a,
            else => return GateConfigError.MissingRequiredField,
        };
        break :blk try parseAnomalyRules(alloc, arr.items);
    };

    return GatePolicy{
        .rules = rules,
        .anomaly_rules = anomaly_rules,
        .timeout_ms = timeout_ms,
    };
}

pub fn freeGatePolicy(alloc: Allocator, policy: GatePolicy) void {
    freeGateRules(alloc, policy.rules);
    alloc.free(policy.anomaly_rules);
}

// ── Internal helpers ──────────────────────────────────────────────────────

fn parseGateRules(alloc: Allocator, items: []const std.json.Value) (Allocator.Error || GateConfigError)![]const GateRule {
    const out = try alloc.alloc(GateRule, items.len);
    var i: usize = 0;
    errdefer {
        for (out[0..i]) |r| freeGateRule(alloc, r);
        alloc.free(out);
    }
    for (items) |item| {
        const obj = switch (item) {
            .object => |o| o,
            else => return GateConfigError.MissingRequiredField,
        };
        out[i] = try parseOneGateRule(alloc, obj);
        i += 1;
    }
    return out;
}

fn parseOneGateRule(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || GateConfigError)!GateRule {
    const tool_str = jsonStr(obj, "tool") orelse return GateConfigError.MissingRequiredField;
    const tool = try alloc.dupe(u8, tool_str);
    errdefer alloc.free(tool);

    const action_str = jsonStr(obj, "action") orelse return GateConfigError.MissingRequiredField;
    const action = try alloc.dupe(u8, action_str);
    errdefer alloc.free(action);

    const condition: ?[]const u8 = blk: {
        const s = jsonStr(obj, "condition") orelse break :blk null;
        break :blk try alloc.dupe(u8, s);
    };

    const behavior = blk: {
        const s = jsonStr(obj, "behavior") orelse break :blk GateBehavior.approve;
        if (std.mem.eql(u8, s, "approve")) break :blk GateBehavior.approve;
        if (std.mem.eql(u8, s, "auto_kill")) break :blk GateBehavior.auto_kill;
        return GateConfigError.MissingRequiredField;
    };

    return GateRule{
        .tool = tool,
        .action = action,
        .condition = condition,
        .behavior = behavior,
    };
}

fn parseAnomalyRules(alloc: Allocator, items: []const std.json.Value) (Allocator.Error || GateConfigError)![]const AnomalyRule {
    const out = try alloc.alloc(AnomalyRule, items.len);
    var i: usize = 0;
    errdefer alloc.free(out);
    for (items) |item| {
        const obj = switch (item) {
            .object => |o| o,
            else => return GateConfigError.MissingRequiredField,
        };
        const pattern_str = jsonStr(obj, "pattern") orelse return GateConfigError.MissingRequiredField;
        const pattern = AnomalyPattern.fromString(pattern_str) orelse return GateConfigError.MissingRequiredField;
        const threshold_count: u32 = blk: {
            const val = obj.get("threshold_count") orelse return GateConfigError.MissingRequiredField;
            break :blk switch (val) {
                .integer => |n| if (n > 0 and n <= 10000) @intCast(n) else return GateConfigError.InvalidBudget,
                else => return GateConfigError.MissingRequiredField,
            };
        };
        const threshold_window_s: u32 = blk: {
            const val = obj.get("threshold_window_s") orelse return GateConfigError.MissingRequiredField;
            break :blk switch (val) {
                .integer => |n| if (n > 0 and n <= 86400) @intCast(n) else return GateConfigError.InvalidBudget,
                else => return GateConfigError.MissingRequiredField,
            };
        };
        out[i] = AnomalyRule{
            .pattern = pattern,
            .threshold_count = threshold_count,
            .threshold_window_s = threshold_window_s,
        };
        i += 1;
    }
    return out;
}

fn jsonStr(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const val = obj.get(key) orelse return null;
    return switch (val) {
        .string => |s| s,
        else => null,
    };
}

fn freeGateRule(alloc: Allocator, rule: GateRule) void {
    alloc.free(rule.tool);
    alloc.free(rule.action);
    if (rule.condition) |c| alloc.free(c);
}

fn freeGateRules(alloc: Allocator, rules: []const GateRule) void {
    for (rules) |r| freeGateRule(alloc, r);
    alloc.free(rules);
}

// ── Tests ─────────────────────────────────────────────────────────────────

test "parseGatePolicy: valid policy with rules and anomaly" {
    const alloc = std.testing.allocator;
    const json =
        \\{"rules":[{"tool":"git","action":"push","condition":"branch == 'main'","behavior":"approve"},{"tool":"github","action":"create_pr"}],"anomaly_rules":[{"pattern":"same_action","threshold_count":10,"threshold_window_s":60}],"timeout_ms":1800000}
    ;
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch unreachable;
    defer parsed.deinit();
    const obj = parsed.value.object;
    const policy = try parseGatePolicy(alloc, obj);
    defer freeGatePolicy(alloc, policy);

    try std.testing.expectEqual(@as(usize, 2), policy.rules.len);
    try std.testing.expectEqualStrings("git", policy.rules[0].tool);
    try std.testing.expectEqualStrings("push", policy.rules[0].action);
    try std.testing.expectEqualStrings("branch == 'main'", policy.rules[0].condition.?);
    try std.testing.expectEqual(GateBehavior.approve, policy.rules[0].behavior);
    try std.testing.expectEqualStrings("github", policy.rules[1].tool);
    try std.testing.expect(policy.rules[1].condition == null);
    try std.testing.expectEqual(@as(usize, 1), policy.anomaly_rules.len);
    try std.testing.expectEqual(AnomalyPattern.same_action, policy.anomaly_rules[0].pattern);
    try std.testing.expectEqual(@as(u32, 10), policy.anomaly_rules[0].threshold_count);
    try std.testing.expectEqual(@as(u32, 60), policy.anomaly_rules[0].threshold_window_s);
    try std.testing.expectEqual(@as(u64, 1_800_000), policy.timeout_ms);
}

test "parseGatePolicy: empty rules defaults" {
    const alloc = std.testing.allocator;
    const json =
        \\{}
    ;
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch unreachable;
    defer parsed.deinit();
    const policy = try parseGatePolicy(alloc, parsed.value.object);
    defer freeGatePolicy(alloc, policy);

    try std.testing.expectEqual(@as(usize, 0), policy.rules.len);
    try std.testing.expectEqual(@as(usize, 0), policy.anomaly_rules.len);
    try std.testing.expectEqual(ec.GATE_DEFAULT_TIMEOUT_MS, policy.timeout_ms);
}

test "parseGatePolicy: missing tool in rule returns error" {
    const alloc = std.testing.allocator;
    const json =
        \\{"rules":[{"action":"push"}]}
    ;
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch unreachable;
    defer parsed.deinit();
    try std.testing.expectError(
        GateConfigError.MissingRequiredField,
        parseGatePolicy(alloc, parsed.value.object),
    );
}

test "parseGatePolicy: invalid behavior string returns error (RULES.md #36)" {
    const alloc = std.testing.allocator;
    const json =
        \\{"rules":[{"tool":"git","action":"push","behavior":"autokill"}]}
    ;
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch unreachable;
    defer parsed.deinit();
    try std.testing.expectError(
        GateConfigError.MissingRequiredField,
        parseGatePolicy(alloc, parsed.value.object),
    );
}

test "parseGatePolicy: unknown anomaly pattern returns error" {
    const alloc = std.testing.allocator;
    const json =
        \\{"anomaly_rules":[{"pattern":"unknown_pattern","threshold_count":5,"threshold_window_s":30}]}
    ;
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch unreachable;
    defer parsed.deinit();
    try std.testing.expectError(
        GateConfigError.MissingRequiredField,
        parseGatePolicy(alloc, parsed.value.object),
    );
}

test "parseGatePolicy: anomaly threshold_count zero returns error" {
    const alloc = std.testing.allocator;
    const json =
        \\{"anomaly_rules":[{"pattern":"same_action","threshold_count":0,"threshold_window_s":60}]}
    ;
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch unreachable;
    defer parsed.deinit();
    try std.testing.expectError(
        GateConfigError.InvalidBudget,
        parseGatePolicy(alloc, parsed.value.object),
    );
}

test "parseGatePolicy: anomaly threshold_window_s exceeds max returns error" {
    const alloc = std.testing.allocator;
    const json =
        \\{"anomaly_rules":[{"pattern":"same_action","threshold_count":10,"threshold_window_s":100000}]}
    ;
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch unreachable;
    defer parsed.deinit();
    try std.testing.expectError(
        GateConfigError.InvalidBudget,
        parseGatePolicy(alloc, parsed.value.object),
    );
}

test "parseGatePolicy: auto_kill behavior parses correctly" {
    const alloc = std.testing.allocator;
    const json =
        \\{"rules":[{"tool":"stripe","action":"charge","behavior":"auto_kill"}]}
    ;
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch unreachable;
    defer parsed.deinit();
    const policy = try parseGatePolicy(alloc, parsed.value.object);
    defer freeGatePolicy(alloc, policy);
    try std.testing.expectEqual(GateBehavior.auto_kill, policy.rules[0].behavior);
}

test "AnomalyPattern.fromString: valid and invalid patterns" {
    try std.testing.expectEqual(AnomalyPattern.same_action, AnomalyPattern.fromString("same_action").?);
    try std.testing.expect(AnomalyPattern.fromString("unknown") == null);
    try std.testing.expect(AnomalyPattern.fromString("") == null);
    try std.testing.expect(AnomalyPattern.fromString("SAME_ACTION") == null);
}
