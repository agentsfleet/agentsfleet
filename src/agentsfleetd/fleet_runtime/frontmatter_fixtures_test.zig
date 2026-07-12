// Loads SKILL.md / TRIGGER.md fixtures from tests/fixtures/fleetbundle/{skill,trigger}/
// at test time and asserts the expected parser outcome for each. The
// fixtures are parser inputs (positive + negative); this test pins their
// behavior to the parser so bundle authoring and parsing stay aligned.
//
// Tests run from the repo root (zig build sets cwd), so paths are relative
// to the project root.

const std = @import("std");
const common = @import("common");
const config = @import("config.zig");
const BYTES_PER_KIB = 1024;

const FIXTURES_BASE = "tests/fixtures/fleetbundle";
const PLATFORM_OPS_FIXTURE_NAME = "platform-ops-agent";
const PLATFORM_OPS_SKILL_PATH = "platform-ops/SKILL.md";
const PLATFORM_OPS_TRIGGER_PATH = "platform-ops/TRIGGER.md";
const MODEL_PLACEHOLDER = "{{model}}";
const MODEL_VALUE = "accounts/fireworks/models/kimi-k2.6";
const CONTEXT_CAP_PLACEHOLDER = "{{context_cap_tokens}}";
const CONTEXT_CAP_VALUE = "256000";
const FirstPartyFixture = struct {
    name: []const u8,
    directory: []const u8,
};
const FIRST_PARTY_FIXTURES = [_]FirstPartyFixture{
    .{ .name = "github-pr-reviewer", .directory = "github-pr-reviewer" },
    .{ .name = "security-reviewer", .directory = "security-reviewer" },
    .{ .name = "zoho-sprint-daily-summarizer", .directory = "zoho-sprint-daily-summarizer" },
};

fn loadFixture(alloc: std.mem.Allocator, rel_path: []const u8) ![]u8 {
    const path = try std.fs.path.join(alloc, &.{ FIXTURES_BASE, rel_path });
    defer alloc.free(path);
    return std.Io.Dir.cwd().readFileAlloc(common.globalIo(), path, alloc, .limited(64 * BYTES_PER_KIB));
}

test "fixture skill/minimal.md parses" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "skill/minimal.md");
    defer alloc.free(md);
    var meta = try config.parseSkillMetadata(alloc, md);
    defer meta.deinit(alloc);
    try std.testing.expectEqualStrings("minimal-skill", meta.name);
    try std.testing.expect(meta.tags.len == 0);
}

test "fixture skill/full.md parses with all optional fields" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "skill/full.md");
    defer alloc.free(md);
    var meta = try config.parseSkillMetadata(alloc, md);
    defer meta.deinit(alloc);
    try std.testing.expectEqualStrings("full-skill", meta.name);
    try std.testing.expectEqualStrings("1.2.3", meta.version);
    try std.testing.expect(meta.author != null);
    try std.testing.expect(meta.model != null);
    try std.testing.expect(meta.when_to_use != null);
    try std.testing.expect(meta.tags.len == 3);
}

test "fixture skill/missing_name.md → MissingRequiredField" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "skill/missing_name.md");
    defer alloc.free(md);
    try std.testing.expectError(
        config.FleetConfigError.MissingRequiredField,
        config.parseSkillMetadata(alloc, md),
    );
}

test "fixture trigger/minimal.md parses" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "trigger/minimal.md");
    defer alloc.free(md);
    var parsed = try config.parseTriggerMarkdownWithJson(alloc, md);
    defer parsed.deinit(alloc);
    const cfg = &parsed.config;
    try std.testing.expectEqualStrings("minimal-skill", cfg.name);
    try std.testing.expectEqual(@as(usize, 1), cfg.tools.len);
}

test "fixture trigger/full.md parses with full webhook signature" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "trigger/full.md");
    defer alloc.free(md);
    var parsed = try config.parseTriggerMarkdownWithJson(alloc, md);
    defer parsed.deinit(alloc);
    const cfg = &parsed.config;
    try std.testing.expectEqualStrings("full-skill", cfg.name);
    try std.testing.expectEqual(@as(usize, 1), cfg.triggers.len);
    try std.testing.expectEqualStrings("github", cfg.triggers[0].webhook.source);
    try std.testing.expect(cfg.triggers[0].webhook.signature != null);
    try std.testing.expect(cfg.network != null);
    try std.testing.expectEqual(@as(usize, 2), cfg.network.?.allow.len);
    try std.testing.expectEqual(@as(usize, 3), cfg.tools.len);
}

test "fixture trigger/with_model_and_context.md parses model + every context knob" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "trigger/with_model_and_context.md");
    defer alloc.free(md);
    var parsed = try config.parseTriggerMarkdownWithJson(alloc, md);
    defer parsed.deinit(alloc);
    const cfg = &parsed.config;
    try std.testing.expectEqualStrings("accounts/fireworks/models/kimi-k2.6", cfg.model.?);
    const ctx = cfg.context.?;
    try std.testing.expectEqual(@as(u32, 256000), ctx.context_cap_tokens);
    try std.testing.expectEqual(@as(u32, 0), ctx.tool_window); // "auto" → 0
    try std.testing.expectEqual(@as(u32, 5), ctx.memory_checkpoint_every);
    try std.testing.expectEqual(@as(f32, 0.75), ctx.stage_chunk_threshold);
}

test "fixture trigger/runtime_at_top_level.md → RuntimeKeysOutsideBlock" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "trigger/runtime_at_top_level.md");
    defer alloc.free(md);
    try std.testing.expectError(
        config.FleetConfigError.RuntimeKeysOutsideBlock,
        config.parseTriggerMarkdownWithJson(alloc, md),
    );
}

test "fixture trigger/unknown_runtime_key.md → UnknownRuntimeKey" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "trigger/unknown_runtime_key.md");
    defer alloc.free(md);
    try std.testing.expectError(
        config.FleetConfigError.UnknownRuntimeKey,
        config.parseTriggerMarkdownWithJson(alloc, md),
    );
}

test "fixture skill/name_mismatch — both files parse but identities disagree" {
    const alloc = std.testing.allocator;
    const skill_md = try loadFixture(alloc, "skill/name_mismatch/SKILL.md");
    defer alloc.free(skill_md);
    const trigger_md = try loadFixture(alloc, "skill/name_mismatch/TRIGGER.md");
    defer alloc.free(trigger_md);

    var meta = try config.parseSkillMetadata(alloc, skill_md);
    defer meta.deinit(alloc);
    var parsed = try config.parseTriggerMarkdownWithJson(alloc, trigger_md);
    defer parsed.deinit(alloc);
    const cfg = &parsed.config;

    // Both parse cleanly — the cross-file invariant is enforced by the
    // install handler, not the per-file parsers.
    try std.testing.expect(!std.mem.eql(u8, meta.name, cfg.name));
}

test "platform operations acceptance fixture parses as one matching fleet" {
    const alloc = std.testing.allocator;
    const skill_md = try loadFixture(alloc, PLATFORM_OPS_SKILL_PATH);
    defer alloc.free(skill_md);
    const trigger_template = try loadFixture(alloc, PLATFORM_OPS_TRIGGER_PATH);
    defer alloc.free(trigger_template);
    const trigger_with_model = try std.mem.replaceOwned(u8, alloc, trigger_template, MODEL_PLACEHOLDER, MODEL_VALUE);
    defer alloc.free(trigger_with_model);
    const trigger_md = try std.mem.replaceOwned(u8, alloc, trigger_with_model, CONTEXT_CAP_PLACEHOLDER, CONTEXT_CAP_VALUE);
    defer alloc.free(trigger_md);

    var meta = try config.parseSkillMetadata(alloc, skill_md);
    defer meta.deinit(alloc);
    var parsed = try config.parseTriggerMarkdownWithJson(alloc, trigger_md);
    defer parsed.deinit(alloc);

    try std.testing.expectEqualStrings(PLATFORM_OPS_FIXTURE_NAME, meta.name);
    try std.testing.expectEqualStrings(meta.name, parsed.config.name);
    try std.testing.expectEqualStrings(MODEL_VALUE, parsed.config.model.?);
    try std.testing.expectEqual(@as(u32, 256000), parsed.config.context.?.context_cap_tokens); // pin test: literal is the expected parsed value
}

test "first-party library fixtures use the supported HTTP request tool" {
    const alloc = std.testing.allocator;
    for (FIRST_PARTY_FIXTURES) |fixture| {
        const skill_path = try std.fmt.allocPrint(alloc, "{s}/SKILL.md", .{fixture.directory});
        defer alloc.free(skill_path);
        const trigger_path = try std.fmt.allocPrint(alloc, "{s}/TRIGGER.md", .{fixture.directory});
        defer alloc.free(trigger_path);
        const skill_md = try loadFixture(alloc, skill_path);
        defer alloc.free(skill_md);
        const trigger_md = try loadFixture(alloc, trigger_path);
        defer alloc.free(trigger_md);

        var meta = config.parseSkillMetadata(alloc, skill_md) catch |err| {
            std.debug.print("first-party SKILL fixture failed: {s}: {s}\n", .{ fixture.name, @errorName(err) });
            return err;
        };
        defer meta.deinit(alloc);
        var parsed = config.parseTriggerMarkdownWithJson(alloc, trigger_md) catch |err| {
            std.debug.print("first-party TRIGGER fixture failed: {s}: {s}\n", .{ fixture.name, @errorName(err) });
            return err;
        };
        defer parsed.deinit(alloc);

        try std.testing.expectEqualStrings(fixture.name, meta.name);
        try std.testing.expectEqualStrings(meta.name, parsed.config.name);
        try std.testing.expectEqual(@as(usize, 1), parsed.config.tools.len);
        try std.testing.expectEqualStrings("http_request", parsed.config.tools[0]);
    }
}
