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

// --- T1: Happy path — loadPrompts reads all three prompt files ---

test "loadPrompts reads files from absolute config dir" {
    const alloc = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();

    try tmp.dir.writeFile(.{ .sub_path = "echo-prompt.md", .data = "echo content" });
    try tmp.dir.writeFile(.{ .sub_path = "scout-prompt.md", .data = "scout content" });
    try tmp.dir.writeFile(.{ .sub_path = "warden-prompt.md", .data = "warden content" });

    const path = try tmp.dir.realpathAlloc(alloc, ".");
    defer alloc.free(path);

    const prompts = try agents.loadPrompts(alloc, path);
    defer {
        alloc.free(prompts.echo);
        alloc.free(prompts.scout);
        alloc.free(prompts.warden);
    }

    try std.testing.expectEqualStrings("echo content", prompts.echo);
    try std.testing.expectEqualStrings("scout content", prompts.scout);
    try std.testing.expectEqualStrings("warden content", prompts.warden);
}

// --- T2: Edge cases — unicode content, empty files, large files ---

test "loadPrompts handles unicode and multibyte content" {
    const alloc = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();

    const unicode_content = "Spec: 中文 emoji 👨‍👩‍👧‍👦 café ☕ RTL: مرحبا";
    try tmp.dir.writeFile(.{ .sub_path = "echo-prompt.md", .data = unicode_content });
    try tmp.dir.writeFile(.{ .sub_path = "scout-prompt.md", .data = "" });
    try tmp.dir.writeFile(.{ .sub_path = "warden-prompt.md", .data = "ok" });

    const path = try tmp.dir.realpathAlloc(alloc, ".");
    defer alloc.free(path);

    const prompts = try agents.loadPrompts(alloc, path);
    defer {
        alloc.free(prompts.echo);
        alloc.free(prompts.scout);
        alloc.free(prompts.warden);
    }

    try std.testing.expectEqualStrings(unicode_content, prompts.echo);
    try std.testing.expectEqualStrings("", prompts.scout);
    try std.testing.expectEqualStrings("ok", prompts.warden);
}

// --- T3: Error paths — missing dir, partial files ---

test "loadPrompts fails when config dir missing" {
    const result = agents.loadPrompts(std.testing.allocator, "/tmp/nonexistent-zombie-test-dir-abc123");
    try std.testing.expectError(error.FileNotFound, result);
}

test "loadPrompts fails when one prompt file missing" {
    const alloc = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();

    // Only write echo and scout — warden is missing
    try tmp.dir.writeFile(.{ .sub_path = "echo-prompt.md", .data = "echo" });
    try tmp.dir.writeFile(.{ .sub_path = "scout-prompt.md", .data = "scout" });

    const path = try tmp.dir.realpathAlloc(alloc, ".");
    defer alloc.free(path);

    const result = agents.loadPrompts(alloc, path);
    try std.testing.expectError(error.FileNotFound, result);
}

// --- T11: Memory safety — no leaks via testing allocator ---

test "loadPrompts frees all memory on error (no leaks)" {
    // std.testing.allocator will catch any leaked allocations
    const result = agents.loadPrompts(std.testing.allocator, "/tmp/nonexistent-zombie-test-dir-abc123");
    try std.testing.expectError(error.FileNotFound, result);
    // If we reach here without allocator panic, no memory was leaked
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
