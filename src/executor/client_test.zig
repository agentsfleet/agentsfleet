//! Unit tests for ExecutorClient (classifyError, StagePayload, AgentConfig).
//! Inline tests were split from client.zig per RULE FLL (350-line gate, M18_001).

const std = @import("std");
const client = @import("client.zig");
const types = @import("types.zig");
const protocol = @import("protocol.zig");

const ExecutorClient = client.ExecutorClient;
const classifyError = @import("client.zig").classifyError;

test "classifyError maps all known error codes" {
    try std.testing.expectEqual(types.FailureClass.timeout_kill, classifyError(protocol.ErrorCode.timeout_killed));
    try std.testing.expectEqual(types.FailureClass.oom_kill, classifyError(protocol.ErrorCode.oom_killed));
    try std.testing.expectEqual(types.FailureClass.policy_deny, classifyError(protocol.ErrorCode.policy_denied));
    try std.testing.expectEqual(types.FailureClass.lease_expired, classifyError(protocol.ErrorCode.lease_expired));
    try std.testing.expectEqual(types.FailureClass.landlock_deny, classifyError(protocol.ErrorCode.landlock_denied));
    try std.testing.expectEqual(types.FailureClass.resource_kill, classifyError(protocol.ErrorCode.resource_killed));
}

test "classifyError falls back to executor_crash for unknown codes" {
    try std.testing.expectEqual(types.FailureClass.executor_crash, classifyError(0));
    try std.testing.expectEqual(types.FailureClass.executor_crash, classifyError(-999));
    try std.testing.expectEqual(types.FailureClass.executor_crash, classifyError(42));
}

test "StagePayload default values" {
    const payload = ExecutorClient.StagePayload{
        .stage_id = "plan",
        .role_id = "coder",
        .skill_id = "zig",
    };
    try std.testing.expectEqualStrings("plan", payload.stage_id);
    try std.testing.expectEqualStrings("coder", payload.role_id);
    try std.testing.expectEqualStrings("zig", payload.skill_id);
    try std.testing.expectEqualStrings("", payload.message);
    try std.testing.expect(payload.tools == null);
    try std.testing.expect(payload.context == null);
    // Agent config should use defaults.
    try std.testing.expectEqualStrings("", payload.agent_config.model);
    try std.testing.expectEqualStrings("anthropic", payload.agent_config.provider);
    try std.testing.expectEqual(@as(f64, 0.7), payload.agent_config.temperature);
    try std.testing.expectEqual(@as(u64, 16384), payload.agent_config.max_tokens);
}

test "AgentConfig default values" {
    const ac = ExecutorClient.AgentConfig{};
    try std.testing.expectEqualStrings("", ac.model);
    try std.testing.expectEqualStrings("anthropic", ac.provider);
    try std.testing.expectEqualStrings("", ac.system_prompt);
    try std.testing.expectEqual(@as(f64, 0.7), ac.temperature);
    try std.testing.expectEqual(@as(u64, 16384), ac.max_tokens);
}

test "classifyError maps all known protocol error codes" {
    // All 7 known codes (6 domain + execution_failed).
    try std.testing.expectEqual(types.FailureClass.timeout_kill, classifyError(protocol.ErrorCode.timeout_killed));
    try std.testing.expectEqual(types.FailureClass.oom_kill, classifyError(protocol.ErrorCode.oom_killed));
    try std.testing.expectEqual(types.FailureClass.policy_deny, classifyError(protocol.ErrorCode.policy_denied));
    try std.testing.expectEqual(types.FailureClass.lease_expired, classifyError(protocol.ErrorCode.lease_expired));
    try std.testing.expectEqual(types.FailureClass.landlock_deny, classifyError(protocol.ErrorCode.landlock_denied));
    try std.testing.expectEqual(types.FailureClass.resource_kill, classifyError(protocol.ErrorCode.resource_killed));
    // execution_failed falls through to else => executor_crash.
    try std.testing.expectEqual(types.FailureClass.executor_crash, classifyError(protocol.ErrorCode.execution_failed));
}
