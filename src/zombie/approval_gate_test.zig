// Unit tests for approval_gate.zig — gate evaluation, condition parsing,
// anomaly detection types, and message builder.
//
// Tests mapped to spec sections:
//   §1.0 — evaluateGate (1.1-1.4)
//   §3.0 — checkAnomaly types (3.1, 3.3 — unit portions only)
//   §5.0 — ActionDetail struct, GateResult enum
//   §6.0 — error paths (condition parsing edge cases)
//
// Integration tests requiring Redis are in the integration test suite.

const std = @import("std");
const approval_gate = @import("approval_gate.zig");
const config_gates = @import("config_gates.zig");
const ec = @import("../errors/error_registry.zig");

const GateDecision = approval_gate.GateDecision;

fn makePolicy(rules: []const config_gates.GateRule) config_gates.GatePolicy {
    return .{ .rules = rules, .anomaly_rules = &.{}, .timeout_ms = ec.GATE_DEFAULT_TIMEOUT_MS };
}

// ── T1: Happy path — Spec §1.2: evaluateGate rule matching ──────────────

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

// ── T1: Happy path — Spec §1.3: condition miss → auto_approve ───────────

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

// ── T1: Happy path — Spec §1.4: no matching rule → auto_approve ────────

test "1.4: no matching rule returns auto_approve" {
    const rules = [_]config_gates.GateRule{
        .{ .tool = "git", .action = "push", .condition = null, .behavior = .approve },
    };
    try std.testing.expectEqual(
        GateDecision.auto_approve,
        approval_gate.evaluateGate(makePolicy(&rules), "slack", "react", null),
    );
}

// ── T2: Edge cases — wildcard matching ──────────────────────────────────

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

test "double wildcard matches everything" {
    const rules = [_]config_gates.GateRule{
        .{ .tool = "*", .action = "*", .condition = null, .behavior = .approve },
    };
    try std.testing.expectEqual(
        GateDecision.requires_approval,
        approval_gate.evaluateGate(makePolicy(&rules), "anything", "at_all", null),
    );
}

// ── T2: Edge cases — condition evaluation ───────────────────────────────

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

test "condition: context is non-object returns true (safe default)" {
    const alloc = std.testing.allocator;
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc,
        \\42
    , .{});
    defer parsed.deinit();
    const rules = [_]config_gates.GateRule{
        .{ .tool = "git", .action = "push", .condition = "x == 'y'", .behavior = .approve },
    };
    try std.testing.expectEqual(
        GateDecision.requires_approval,
        approval_gate.evaluateGate(makePolicy(&rules), "git", "push", parsed.value),
    );
}

test "condition: == positive match" {
    const alloc = std.testing.allocator;
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"env":"production"}
    , .{});
    defer parsed.deinit();
    const rules = [_]config_gates.GateRule{
        .{ .tool = "deploy", .action = "run", .condition = "env == 'production'", .behavior = .approve },
    };
    try std.testing.expectEqual(
        GateDecision.requires_approval,
        approval_gate.evaluateGate(makePolicy(&rules), "deploy", "run", parsed.value),
    );
}

test "condition: == negative match skips rule" {
    const alloc = std.testing.allocator;
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"env":"staging"}
    , .{});
    defer parsed.deinit();
    const rules = [_]config_gates.GateRule{
        .{ .tool = "deploy", .action = "run", .condition = "env == 'production'", .behavior = .approve },
    };
    try std.testing.expectEqual(
        GateDecision.auto_approve,
        approval_gate.evaluateGate(makePolicy(&rules), "deploy", "run", parsed.value),
    );
}

test "condition: != positive match (field differs from value)" {
    const alloc = std.testing.allocator;
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"channel":"#alerts"}
    , .{});
    defer parsed.deinit();
    const rules = [_]config_gates.GateRule{
        .{ .tool = "slack", .action = "post", .condition = "channel != '#general'", .behavior = .approve },
    };
    try std.testing.expectEqual(
        GateDecision.requires_approval,
        approval_gate.evaluateGate(makePolicy(&rules), "slack", "post", parsed.value),
    );
}

test "condition: invalid expression defaults to gate-fires (safe)" {
    const rules = [_]config_gates.GateRule{
        .{ .tool = "x", .action = "y", .condition = "garbage expression no operator", .behavior = .approve },
    };
    // Invalid condition → safe default → gate fires
    try std.testing.expectEqual(
        GateDecision.requires_approval,
        approval_gate.evaluateGate(makePolicy(&rules), "x", "y", null),
    );
}

// ── T1/T2: Rule ordering and precedence ─────────────────────────────────

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

test "auto_kill behavior returns auto_kill decision" {
    const rules = [_]config_gates.GateRule{
        .{ .tool = "stripe", .action = "create_charge", .condition = null, .behavior = .auto_kill },
    };
    try std.testing.expectEqual(
        GateDecision.auto_kill,
        approval_gate.evaluateGate(makePolicy(&rules), "stripe", "create_charge", null),
    );
}

test "empty policy returns auto_approve for any action" {
    const empty_rules = [_]config_gates.GateRule{};
    try std.testing.expectEqual(
        GateDecision.auto_approve,
        approval_gate.evaluateGate(makePolicy(&empty_rules), "any", "action", null),
    );
}

test "specific rule before wildcard — specific wins" {
    const rules = [_]config_gates.GateRule{
        .{ .tool = "git", .action = "push", .condition = null, .behavior = .auto_kill },
        .{ .tool = "*", .action = "*", .condition = null, .behavior = .approve },
    };
    // git.push hits auto_kill first
    try std.testing.expectEqual(
        GateDecision.auto_kill,
        approval_gate.evaluateGate(makePolicy(&rules), "git", "push", null),
    );
    // other actions hit the wildcard approve
    try std.testing.expectEqual(
        GateDecision.requires_approval,
        approval_gate.evaluateGate(makePolicy(&rules), "slack", "post", null),
    );
}

// ── T1: Slack message builder ───────────────────────────────────────────

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

// ── T8: Security — JSON injection via user input ────────────────────────

test "T8: JSON injection in zombie_name does not break message" {
    const alloc = std.testing.allocator;
    const msg = try approval_gate.buildSlackApprovalMessage(
        alloc,
        "zombie\"with\\quotes\nnewline",
        "a1",
        .{ .tool = "t", .action = "a", .params_summary = "s\"quoted" },
        "",
    );
    defer alloc.free(msg);
    // Must still parse as valid JSON
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value == .object);
}

// ── T10: Constants — error codes follow naming convention ───────────────

test "T10: approval error codes follow UZ-APPROVAL- prefix" {
    try std.testing.expect(std.mem.startsWith(u8, ec.ERR_APPROVAL_PARSE_FAILED, "UZ-APPROVAL-"));
    try std.testing.expect(std.mem.startsWith(u8, ec.ERR_APPROVAL_NOT_FOUND, "UZ-APPROVAL-"));
    try std.testing.expect(std.mem.startsWith(u8, ec.ERR_APPROVAL_INVALID_SIGNATURE, "UZ-APPROVAL-"));
    try std.testing.expect(std.mem.startsWith(u8, ec.ERR_APPROVAL_REDIS_UNAVAILABLE, "UZ-APPROVAL-"));
    try std.testing.expect(std.mem.startsWith(u8, ec.ERR_APPROVAL_CONDITION_INVALID, "UZ-APPROVAL-"));
}

test "T10: approval error codes are distinct — no collision" {
    const codes = [_][]const u8{
        ec.ERR_APPROVAL_PARSE_FAILED,
        ec.ERR_APPROVAL_NOT_FOUND,
        ec.ERR_APPROVAL_INVALID_SIGNATURE,
        ec.ERR_APPROVAL_REDIS_UNAVAILABLE,
        ec.ERR_APPROVAL_CONDITION_INVALID,
    };
    for (codes, 0..) |a, i| {
        for (codes[i + 1 ..]) |b| {
            try std.testing.expect(!std.mem.eql(u8, a, b));
        }
    }
}

test "T10: approval error hints exist and are actionable" {
    try std.testing.expect(ec.hint(ec.ERR_APPROVAL_PARSE_FAILED).len > 0);
    try std.testing.expect(ec.hint(ec.ERR_APPROVAL_NOT_FOUND).len > 0);
    try std.testing.expect(ec.hint(ec.ERR_APPROVAL_REDIS_UNAVAILABLE).len > 0);
    try std.testing.expect(ec.hint(ec.ERR_APPROVAL_CONDITION_INVALID).len > 0);
}

// ── T10: Constants — gate event types and decision strings ──────────────

test "T10: gate event type constants are non-empty" {
    try std.testing.expect(ec.GATE_EVENT_REQUIRED.len > 0);
    try std.testing.expect(ec.GATE_EVENT_APPROVED.len > 0);
    try std.testing.expect(ec.GATE_EVENT_DENIED.len > 0);
    try std.testing.expect(ec.GATE_EVENT_TIMEOUT.len > 0);
    try std.testing.expect(ec.GATE_EVENT_AUTO_KILL.len > 0);
    try std.testing.expect(ec.GATE_EVENT_AUTO_APPROVE.len > 0);
}

test "T10: gate decision constants match expected values" {
    try std.testing.expectEqualStrings("approve", ec.GATE_DECISION_APPROVE);
    try std.testing.expectEqualStrings("deny", ec.GATE_DECISION_DENY);
}

// ── T11: Memory safety — leak detection ─────────────────────────────────

// ── T5: GateResult enum exhaustiveness ──────────────────────────────────

test "T5: GateResult has exactly 3 variants (no auto_killed)" {
    // Verify the enum fields to catch if someone adds back auto_killed
    const fields = @typeInfo(approval_gate.GateResult).@"enum".fields;
    try std.testing.expectEqual(@as(usize, 3), fields.len);
}

// ── T2: Multiple rules with conditions — complex policy ─────────────────

test "complex policy: multiple rules with mixed conditions" {
    const alloc = std.testing.allocator;
    const rules = [_]config_gates.GateRule{
        .{ .tool = "git", .action = "push", .condition = "branch == 'main'", .behavior = .auto_kill },
        .{ .tool = "git", .action = "push", .condition = null, .behavior = .approve },
        .{ .tool = "slack", .action = "post_message", .condition = "channel != '#general'", .behavior = .approve },
    };
    const policy = makePolicy(&rules);

    // git push to main → auto_kill (first rule)
    {
        const ctx = try std.json.parseFromSlice(std.json.Value, alloc,
            \\{"branch":"main"}
        , .{});
        defer ctx.deinit();
        try std.testing.expectEqual(GateDecision.auto_kill, approval_gate.evaluateGate(policy, "git", "push", ctx.value));
    }

    // git push to feature → approve (second rule, first condition misses)
    {
        const ctx = try std.json.parseFromSlice(std.json.Value, alloc,
            \\{"branch":"feature/x"}
        , .{});
        defer ctx.deinit();
        try std.testing.expectEqual(GateDecision.requires_approval, approval_gate.evaluateGate(policy, "git", "push", ctx.value));
    }

    // slack post to #alerts → approve (third rule)
    {
        const ctx = try std.json.parseFromSlice(std.json.Value, alloc,
            \\{"channel":"#alerts"}
        , .{});
        defer ctx.deinit();
        try std.testing.expectEqual(GateDecision.requires_approval, approval_gate.evaluateGate(policy, "slack", "post_message", ctx.value));
    }

    // slack post to #general → auto_approve (third rule condition misses, no more rules)
    {
        const ctx = try std.json.parseFromSlice(std.json.Value, alloc,
            \\{"channel":"#general"}
        , .{});
        defer ctx.deinit();
        try std.testing.expectEqual(GateDecision.auto_approve, approval_gate.evaluateGate(policy, "slack", "post_message", ctx.value));
    }

    // unmatched tool → auto_approve
    try std.testing.expectEqual(GateDecision.auto_approve, approval_gate.evaluateGate(policy, "linear", "create_issue", null));
}
