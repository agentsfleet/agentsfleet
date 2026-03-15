const std = @import("std");
const types = @import("../types.zig");
const agents = @import("agents.zig");

fn testCustomSkill(
    alloc: std.mem.Allocator,
    role_id: []const u8,
    skill_id: []const u8,
    input: agents.RoleInput,
) !agents.AgentResult {
    _ = alloc;
    _ = skill_id;
    _ = input;
    return .{ .content = role_id, .token_count = 1, .wall_seconds = 0, .exit_ok = true };
}

test "parseObserverBackend supports known values" {
    try std.testing.expectEqual(@as(?agents.ObserverBackend, .log), agents.parseObserverBackend("log"));
    try std.testing.expectEqual(@as(?agents.ObserverBackend, .log), agents.parseObserverBackend("LOG"));
    try std.testing.expectEqual(@as(?agents.ObserverBackend, .noop), agents.parseObserverBackend("noop"));
    try std.testing.expectEqual(@as(?agents.ObserverBackend, .verbose), agents.parseObserverBackend("verbose"));
    try std.testing.expectEqual(@as(?agents.ObserverBackend, null), agents.parseObserverBackend("otel"));
}

test "lookupRole resolves built-in role ids" {
    const echo = agents.lookupRole("echo") orelse return error.TestExpectedRole;
    try std.testing.expectEqual(types.Actor.echo, echo.actor);
    try std.testing.expectEqual(agents.SkillKind.echo, echo.kind);

    const scout = agents.lookupRole("SCOUT") orelse return error.TestExpectedRole;
    try std.testing.expectEqual(types.Actor.scout, scout.actor);
    try std.testing.expectEqual(agents.SkillKind.scout, scout.kind);

    try std.testing.expectEqual(@as(?agents.RoleBinding, null), agents.lookupRole("security"));
}

test "custom skill registration and dispatch works for non built-in role" {
    var registry = agents.SkillRegistry.init(std.testing.allocator);
    defer registry.deinit();

    try registry.registerCustomSkill("security-reviewer", .orchestrator, testCustomSkill);
    const binding = agents.resolveRoleWithRegistry(&registry, "security", "security-reviewer") orelse return error.TestExpectedRole;
    try std.testing.expectEqual(agents.SkillKind.custom, binding.kind);
    try std.testing.expectEqual(types.Actor.orchestrator, binding.actor);

    const fake_prompts = agents.PromptFiles{ .echo = "echo prompt", .scout = "scout prompt", .warden = "warden prompt" };
    const result = try agents.runByRole(std.testing.allocator, binding, .{ .workspace_path = "/tmp", .prompts = &fake_prompts });
    try std.testing.expectEqualStrings("security", result.content);
}

test "runByRole validates required stage input fields" {
    const binding = agents.lookupRole("warden") orelse return error.TestExpectedRole;
    const fake_prompts = agents.PromptFiles{ .echo = "echo prompt", .scout = "scout prompt", .warden = "warden prompt" };

    try std.testing.expectError(agents.RoleError.MissingRoleInput, agents.runByRole(std.testing.allocator, binding, .{
        .workspace_path = "/tmp",
        .prompts = &fake_prompts,
        .spec_content = "spec",
        .plan_content = "plan",
    }));
}
