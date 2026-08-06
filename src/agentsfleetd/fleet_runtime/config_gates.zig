// Gate policy parser — parses the "gates" section from Fleet config JSON.
//
// Gate policies define which tool actions require human approval and which
// anomaly patterns trigger auto-kill. Parsed from config_json at claim time.
// Types are re-exported by config.zig for use by approval_gate.zig.

const std = @import("std");
const Allocator = std.mem.Allocator;
const ec = @import("../errors/error_registry.zig");
const gate_constants = @import("approval_gate_constants.zig");
const gate_condition = @import("gate_condition.zig");
const logging = @import("log");
const log = logging.scoped(.fleet_config_gates);
const MAX_BUDGET_UNITS = 10000;
const SECONDS_PER_DAY = 86400;

pub const GateBehavior = enum { approve, auto_kill };

/// Frontmatter keys for the workspace-authored approval copy (RULE UFS — the
/// parser, the bundle fixtures, and the gate tests share these spellings).
pub const S_GATE_KIND = "gate_kind";
pub const S_BLAST_RADIUS = "blast_radius";

pub const GateRule = struct {
    tool: []const u8,
    action: []const u8,
    condition: ?[]const u8,
    behavior: GateBehavior,
    /// What KIND of decision this gate is asking a human to make ("repair",
    /// "deploy"), and how far a yes reaches ("one draft Pull Request on the
    /// declared repository"). Both are WORKSPACE-authored config, so an approval
    /// card can state them as fact — unlike the model-authored half of
    /// `ActionDetail`, which is only ever an attributed claim. Empty when the
    /// rule omits them; a blank field renders as nothing rather than as a
    /// reassuring default.
    gate_kind: []const u8 = "",
    blast_radius: []const u8 = "",
};

pub const AnomalyPattern = enum {
    same_action,

    pub fn fromString(s: []const u8) ?AnomalyPattern {
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

pub const GateConfigError = error{
    MissingRequiredField,
    InvalidBudget,
};

pub fn parseGatePolicy(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || GateConfigError)!GatePolicy {
    const timeout_ms: u64 = blk: {
        const val = obj.get("timeout_ms") orelse break :blk gate_constants.GATE_DEFAULT_TIMEOUT_MS;
        break :blk switch (val) {
            .integer => |i| if (i <= 0)
                gate_constants.GATE_DEFAULT_TIMEOUT_MS
            else if (i > @as(i64, @intCast(gate_constants.GATE_TIMEOUT_MS_MAX))) clamped: {
                log.warn("gate_timeout_clamped", .{ .error_code = ec.ERR_AGENTSFLEET_INVALID_CONFIG, .configured_ms = i, .max_ms = gate_constants.GATE_TIMEOUT_MS_MAX });
                break :clamped gate_constants.GATE_TIMEOUT_MS_MAX;
            } else @intCast(i),
            else => gate_constants.GATE_DEFAULT_TIMEOUT_MS,
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

/// Write-time validation: the first gate-rule condition that is not a parseable
/// expression (gate_condition.isValid), else null. The runtime parser stays
/// lenient; create/patch call this to reject a bad condition with UZ-APPROVAL-005.
pub fn firstInvalidCondition(rules: []const GateRule) ?[]const u8 {
    for (rules) |rule| {
        const c = rule.condition orelse continue;
        if (!gate_condition.isValid(c)) return c;
    }
    return null;
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
    // Guards the `behavior` block below, which returns on an unrecognised
    // string: an errdefer placed after it would leak this dupe on exactly
    // the error path it exists for.
    errdefer if (condition) |c| alloc.free(c);

    const behavior = blk: {
        const s = jsonStr(obj, "behavior") orelse break :blk GateBehavior.approve;
        if (std.mem.eql(u8, s, "approve")) break :blk GateBehavior.approve;
        if (std.mem.eql(u8, s, "auto_kill")) break :blk GateBehavior.auto_kill;
        return GateConfigError.MissingRequiredField;
    };

    // Workspace-authored approval copy. Absent → empty, and an empty field is
    // omitted from the card entirely: a gate that does not say what it is
    // approving must not imply one.
    const gate_kind = try alloc.dupe(u8, jsonStr(obj, S_GATE_KIND) orelse "");
    errdefer alloc.free(gate_kind);
    const blast_radius = try alloc.dupe(u8, jsonStr(obj, S_BLAST_RADIUS) orelse "");

    return GateRule{
        .tool = tool,
        .action = action,
        .condition = condition,
        .behavior = behavior,
        .gate_kind = gate_kind,
        .blast_radius = blast_radius,
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
                .integer => |n| if (n > 0 and n <= MAX_BUDGET_UNITS) @intCast(n) else return GateConfigError.InvalidBudget,
                else => return GateConfigError.MissingRequiredField,
            };
        };
        const threshold_window_s: u32 = blk: {
            const val = obj.get("threshold_window_s") orelse return GateConfigError.MissingRequiredField;
            break :blk switch (val) {
                .integer => |n| if (n > 0 and n <= SECONDS_PER_DAY) @intCast(n) else return GateConfigError.InvalidBudget,
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
    alloc.free(rule.gate_kind);
    alloc.free(rule.blast_radius);
}

fn freeGateRules(alloc: Allocator, rules: []const GateRule) void {
    for (rules) |r| freeGateRule(alloc, r);
    alloc.free(rules);
}

test {
    _ = @import("config_gates_test.zig"); // force-import the split test sibling
}
