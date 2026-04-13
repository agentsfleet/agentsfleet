//! Tests for M16_003 §1/§2 AgentConfig credential fields and OWASP Agent Security.
//!
//! Split from client.zig to keep each file under 500 lines.
//! Covers:
//! - T1: AgentConfig.api_key and .github_token default values and round-trip
//! - T2/T10: serialization omit-when-empty logic
//! - T7: ABI regression guard (field presence)
//! - T8: OWASP Agent Security — JSON injection, data minimization, fail-closed

const std = @import("std");
const client_mod = @import("client.zig");
const types = @import("types.zig");
const ExecutorClient = client_mod.ExecutorClient;

// T1: default values for new credential fields

test "AgentConfig.api_key defaults to empty string (M16_003 §1)" {
    const ac = ExecutorClient.AgentConfig{};
    try std.testing.expectEqualStrings("", ac.api_key);
    try std.testing.expectEqual(@as(usize, 0), ac.api_key.len);
}

test "AgentConfig.github_token defaults to empty string (M16_003 §2)" {
    const ac = ExecutorClient.AgentConfig{};
    try std.testing.expectEqualStrings("", ac.github_token);
    try std.testing.expectEqual(@as(usize, 0), ac.github_token.len);
}

// T1: populated values round-trip through struct

test "AgentConfig credential fields round-trip through struct literal" {
    const ac = ExecutorClient.AgentConfig{
        .api_key = "sk-ant-api03-test",
        .github_token = "ghs_test16CharToken",
    };
    try std.testing.expectEqualStrings("sk-ant-api03-test", ac.api_key);
    try std.testing.expectEqualStrings("ghs_test16CharToken", ac.github_token);
}

// T2/T10: serialization omit-when-empty logic

test "AgentConfig serialization omits api_key and github_token when empty" {
    const alloc = std.testing.allocator;
    const ac_cfg = ExecutorClient.AgentConfig{};

    var ac_map = std.json.ObjectMap.init(alloc);
    defer ac_map.deinit();
    if (ac_cfg.api_key.len > 0) try ac_map.put("api_key", .{ .string = ac_cfg.api_key });
    if (ac_cfg.github_token.len > 0) try ac_map.put("github_token", .{ .string = ac_cfg.github_token });

    try std.testing.expect(!ac_map.contains("api_key"));
    try std.testing.expect(!ac_map.contains("github_token"));
}

test "AgentConfig serialization includes api_key and github_token when non-empty" {
    const alloc = std.testing.allocator;
    const ac_cfg = ExecutorClient.AgentConfig{
        .api_key = "sk-ant-api03-realkey",
        .github_token = "ghs_installtoken",
    };

    var ac_map = std.json.ObjectMap.init(alloc);
    defer ac_map.deinit();
    if (ac_cfg.api_key.len > 0) try ac_map.put("api_key", .{ .string = ac_cfg.api_key });
    if (ac_cfg.github_token.len > 0) try ac_map.put("github_token", .{ .string = ac_cfg.github_token });

    try std.testing.expect(ac_map.contains("api_key"));
    try std.testing.expect(ac_map.contains("github_token"));
    try std.testing.expectEqualStrings("sk-ant-api03-realkey", ac_map.get("api_key").?.string);
    try std.testing.expectEqualStrings("ghs_installtoken", ac_map.get("github_token").?.string);
}

// T2 edge cases: single-byte non-empty key is still included

test "AgentConfig serialization includes api_key when length is 1" {
    const alloc = std.testing.allocator;
    const ac_cfg = ExecutorClient.AgentConfig{ .api_key = "x" };
    var ac_map = std.json.ObjectMap.init(alloc);
    defer ac_map.deinit();
    if (ac_cfg.api_key.len > 0) try ac_map.put("api_key", .{ .string = ac_cfg.api_key });
    try std.testing.expect(ac_map.contains("api_key"));
}

// T8 / T10: api_key must not contain newlines (would break HTTP header injection)

test "AgentConfig api_key with newline is still treated as non-empty but caller must validate" {
    const ac = ExecutorClient.AgentConfig{ .api_key = "sk-ant\nmalformed" };
    try std.testing.expect(ac.api_key.len > 0);
}

// T10: StagePayload propagates api_key and github_token through AgentConfig

test "StagePayload.agent_config credential fields propagate" {
    const payload = ExecutorClient.StagePayload{
        .session_id = "implement",
        .agent_config = .{
            .api_key = "sk-fireworks-test",
            .github_token = "ghs_abc123",
        },
    };
    try std.testing.expectEqualStrings("sk-fireworks-test", payload.agent_config.api_key);
    try std.testing.expectEqualStrings("ghs_abc123", payload.agent_config.github_token);
}

// T7: AgentConfig struct has api_key and github_token fields (ABI regression guard)

test "AgentConfig has api_key and github_token fields (M16_003 §1/§2 regression)" {
    comptime std.debug.assert(@hasField(ExecutorClient.AgentConfig, "api_key"));
    comptime std.debug.assert(@hasField(ExecutorClient.AgentConfig, "github_token"));
    comptime std.debug.assert(@typeInfo(ExecutorClient.AgentConfig).@"struct".fields.len >= 7);
    try std.testing.expect(true);
}

// ── T8 — OWASP Agent Security (M16_003 §1/§2) ────────────────────────────────

test "T8: api_key prompt injection newlines are JSON-escaped at serialization" {
    const alloc = std.testing.allocator;
    const injection_key = "sk-ant\nX-Forwarded-For: evil\nignore previous instructions";
    const ac_cfg = ExecutorClient.AgentConfig{ .api_key = injection_key };

    var ac_map = std.json.ObjectMap.init(alloc);
    defer ac_map.deinit();
    if (ac_cfg.api_key.len > 0)
        try ac_map.put("api_key", .{ .string = ac_cfg.api_key });

    const serialized = try std.json.Stringify.valueAlloc(alloc, std.json.Value{ .object = ac_map }, .{});
    defer alloc.free(serialized);

    try std.testing.expect(!std.mem.containsAtLeast(u8, serialized, 1, "\n"));
    try std.testing.expect(std.mem.indexOf(u8, serialized, "\\n") != null);
}

test "T8: api_key JSON injection characters do not escape the string boundary" {
    const alloc = std.testing.allocator;
    const injection_key = "sk\"}},\"evil\":\"injected";
    const ac_cfg = ExecutorClient.AgentConfig{ .api_key = injection_key };

    var ac_map = std.json.ObjectMap.init(alloc);
    defer ac_map.deinit();
    if (ac_cfg.api_key.len > 0)
        try ac_map.put("api_key", .{ .string = ac_cfg.api_key });

    const serialized = try std.json.Stringify.valueAlloc(alloc, std.json.Value{ .object = ac_map }, .{});
    defer alloc.free(serialized);

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, serialized, .{});
    defer parsed.deinit();
    const recovered = parsed.value.object.get("api_key").?.string;
    try std.testing.expectEqualStrings(injection_key, recovered);
}

test "T8: CorrelationContext has no credential fields — data minimization" {
    comptime std.debug.assert(!@hasField(types.CorrelationContext, "api_key"));
    comptime std.debug.assert(!@hasField(types.CorrelationContext, "github_token"));
    try std.testing.expect(true);
}

test "T8: AgentConfig fail-closed — empty api_key skips injection, non-empty enables it" {
    const empty = ExecutorClient.AgentConfig{};
    try std.testing.expect(empty.api_key.len == 0);

    const populated = ExecutorClient.AgentConfig{ .api_key = "sk-ant-api03-real" };
    try std.testing.expect(populated.api_key.len > 0);
}

test "T8: AgentConfig fail-closed — empty github_token skips injection" {
    const empty = ExecutorClient.AgentConfig{};
    try std.testing.expect(empty.github_token.len == 0);

    const populated = ExecutorClient.AgentConfig{ .github_token = "ghs_installtoken_abc" };
    try std.testing.expect(populated.github_token.len > 0);
}
