//! Tests for the fleet gate-policy parser — split from `config_gates.zig` for
//! the file-length budget (RULE FLL). Subject is unchanged: rule/anomaly parsing,
//! the timeout clamp, and the approval copy a gate rule carries.

const std = @import("std");
const config_gates = @import("config_gates.zig");

const parseGatePolicy = config_gates.parseGatePolicy;
const freeGatePolicy = config_gates.freeGatePolicy;
const GateBehavior = config_gates.GateBehavior;
const AnomalyPattern = config_gates.AnomalyPattern;
const GateConfigError = config_gates.GateConfigError;
const gate_constants = @import("approval_gate_constants.zig");

/// The window ceiling as wire text — lives with the test that asserts the clamp.
const MAX_THRESHOLD_WINDOW_SECONDS_TEXT = "100000";

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

test "parseGatePolicy: timeout above the cap clamps to GATE_TIMEOUT_MS_MAX" {
    const alloc = std.testing.allocator;
    const json =
        \\{"timeout_ms": 999999999999}
    ;
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch unreachable;
    defer parsed.deinit();
    const policy = try parseGatePolicy(alloc, parsed.value.object);
    defer freeGatePolicy(alloc, policy);
    try std.testing.expectEqual(gate_constants.GATE_TIMEOUT_MS_MAX, policy.timeout_ms);
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
    try std.testing.expectEqual(gate_constants.GATE_DEFAULT_TIMEOUT_MS, policy.timeout_ms);
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
        \\{"anomaly_rules":[{"pattern":"same_action","threshold_count":10,"threshold_window_s":
    ++ MAX_THRESHOLD_WINDOW_SECONDS_TEXT ++
        \\}]}
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

test "parseGatePolicy: a rule carries its workspace-authored approval copy" {
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"rules":[{"tool":"*","action":"*","behavior":"approve",
        \\"gate_kind":"repair","blast_radius":"one draft Pull Request on acme/widgets"}]}
    , .{});
    defer parsed.deinit();

    const policy = try parseGatePolicy(alloc, parsed.value.object);
    defer freeGatePolicy(alloc, policy);

    try std.testing.expectEqualStrings("repair", policy.rules[0].gate_kind);
    try std.testing.expectEqualStrings("one draft Pull Request on acme/widgets", policy.rules[0].blast_radius);
}

test "parseGatePolicy: a rule omitting the approval copy parses to empty, not to a default" {
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"rules":[{"tool":"git","action":"push","behavior":"approve"}]}
    , .{});
    defer parsed.deinit();

    const policy = try parseGatePolicy(alloc, parsed.value.object);
    defer freeGatePolicy(alloc, policy);

    // Empty, so the card omits the line entirely. Inventing copy here would tell
    // a human the blast radius was considered when nobody wrote one down.
    try std.testing.expectEqualStrings("", policy.rules[0].gate_kind);
    try std.testing.expectEqualStrings("", policy.rules[0].blast_radius);
}
