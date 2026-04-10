// Unit tests for approval_gate.zig — gate evaluation and condition parsing.

const std = @import("std");
const approval_gate = @import("approval_gate.zig");
const config_gates = @import("config_gates.zig");
const ec = @import("../errors/codes.zig");

const GateDecision = approval_gate.GateDecision;

fn makePolicy(rules: []const config_gates.GateRule) config_gates.GatePolicy {
    return .{ .rules = rules, .anomaly_rules = &.{}, .timeout_ms = ec.GATE_DEFAULT_TIMEOUT_MS };
}

// ── Spec §1.2: evaluateGate — rule matching ─────────────────────────────

test "1.2: matching rule with condition returns requires_approval" {
    const rules = [_]config_gates.GateRule{
        .{ .tool = "git", .action = "push", .condition = "branch == 'main'", .behavior = .approve },
    };
    const alloc = std.testing.allocator;
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"branch":"main"}
    , .{});
    defer parsed.deinit();

    try std.testing.expectEqual(
        GateDecision.requires_approval,
        approval_gate.evaluateGate(makePolicy(&rules), "git", "push", parsed.value),
    );
}

// ── Spec §1.3: condition miss → auto_approve ────────────────────────────

test "1.3: condition not met (same channel) returns auto_approve" {
    const rules = [_]config_gates.GateRule{
        .{ .tool = "slack", .action = "post_message", .condition = "channel != '#general'", .behavior = .approve },
    };
    const alloc = std.testing.allocator;
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"channel":"#general"}
    , .{});
    defer parsed.deinit();

    try std.testing.expectEqual(
        GateDecision.auto_approve,
        approval_gate.evaluateGate(makePolicy(&rules), "slack", "post_message", parsed.value),
    );
}

// ── Spec §1.4: no matching rule → auto_approve ─────────────────────────

test "1.4: no matching rule returns auto_approve" {
    const rules = [_]config_gates.GateRule{
        .{ .tool = "git", .action = "push", .condition = null, .behavior = .approve },
    };
    try std.testing.expectEqual(
        GateDecision.auto_approve,
        approval_gate.evaluateGate(makePolicy(&rules), "slack", "react", null),
    );
}

// ── Wildcard matching ───────────────────────────────────────────────────

test "wildcard tool matches any tool name" {
    const rules = [_]config_gates.GateRule{
        .{ .tool = "*", .action = "delete", .condition = null, .behavior = .approve },
    };
    const policy = makePolicy(&rules);
    try std.testing.expectEqual(GateDecision.requires_approval, approval_gate.evaluateGate(policy, "github", "delete", null));
    try std.testing.expectEqual(GateDecision.auto_approve, approval_gate.evaluateGate(policy, "github", "create", null));
}

test "wildcard action matches any action name" {
    const rules = [_]config_gates.GateRule{
        .{ .tool = "stripe", .action = "*", .condition = null, .behavior = .approve },
    };
    try std.testing.expectEqual(
        GateDecision.requires_approval,
        approval_gate.evaluateGate(makePolicy(&rules), "stripe", "create_charge", null),
    );
}

// ── Condition evaluation edge cases ─────────────────────────────────────

test "condition: null context returns true (safe default — gate fires)" {
    const rules = [_]config_gates.GateRule{
        .{ .tool = "git", .action = "push", .condition = "branch == 'main'", .behavior = .approve },
    };
    try std.testing.expectEqual(
        GateDecision.requires_approval,
        approval_gate.evaluateGate(makePolicy(&rules), "git", "push", null),
    );
}

test "condition: field not in context returns true (safe default)" {
    const alloc = std.testing.allocator;
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"other":"value"}
    , .{});
    defer parsed.deinit();
    const rules = [_]config_gates.GateRule{
        .{ .tool = "git", .action = "push", .condition = "branch == 'main'", .behavior = .approve },
    };
    try std.testing.expectEqual(
        GateDecision.requires_approval,
        approval_gate.evaluateGate(makePolicy(&rules), "git", "push", parsed.value),
    );
}

// ── First-match wins (rule ordering) ────────────────────────────────────

test "first matching rule wins — second rule ignored" {
    const rules = [_]config_gates.GateRule{
        .{ .tool = "git", .action = "push", .condition = null, .behavior = .approve },
        .{ .tool = "git", .action = "push", .condition = null, .behavior = .auto_kill },
    };
    try std.testing.expectEqual(
        GateDecision.requires_approval,
        approval_gate.evaluateGate(makePolicy(&rules), "git", "push", null),
    );
}

// ── Auto-kill behavior ──────────────────────────────────────────────────

test "auto_kill behavior returns auto_kill decision" {
    const rules = [_]config_gates.GateRule{
        .{ .tool = "stripe", .action = "create_charge", .condition = null, .behavior = .auto_kill },
    };
    try std.testing.expectEqual(
        GateDecision.auto_kill,
        approval_gate.evaluateGate(makePolicy(&rules), "stripe", "create_charge", null),
    );
}

// ── Slack message builder ───────────────────────────────────────────────

test "buildSlackApprovalMessage: produces valid JSON with action_id" {
    const alloc = std.testing.allocator;
    const msg = try approval_gate.buildSlackApprovalMessage(
        alloc,
        "test-zombie",
        "action-001",
        .{ .tool = "git", .action = "push", .params_summary = "3 files to main" },
        "https://api.usezombie.com/v1/webhooks/z1:approval",
    );
    defer alloc.free(msg);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value == .object);
    try std.testing.expect(std.mem.indexOf(u8, msg, "action-001") != null);
}
