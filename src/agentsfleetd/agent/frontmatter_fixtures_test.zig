// Loads SKILL.md / TRIGGER.md fixtures from samples/fixtures/frontmatter/
// at test time and asserts the expected parser outcome for each. The
// fixtures are user-facing canonical examples (positive + negative); this
// test pins their behavior to the parser so authoring-doc + parser stay
// aligned.
//
// Tests run from the repo root (zig build sets cwd), so paths are relative
// to the project root.

const std = @import("std");
const common = @import("common");
const config = @import("config.zig");
const BYTES_PER_KIB = 1024;

const FIXTURES_BASE = "samples/fixtures/frontmatter";

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
        config.AgentConfigError.MissingRequiredField,
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
        config.AgentConfigError.RuntimeKeysOutsideBlock,
        config.parseTriggerMarkdownWithJson(alloc, md),
    );
}

test "fixture trigger/unknown_runtime_key.md → UnknownRuntimeKey" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "trigger/unknown_runtime_key.md");
    defer alloc.free(md);
    try std.testing.expectError(
        config.AgentConfigError.UnknownRuntimeKey,
        config.parseTriggerMarkdownWithJson(alloc, md),
    );
}

test "fixture bundles/name_mismatch — both files parse but identities disagree" {
    const alloc = std.testing.allocator;
    const skill_md = try loadFixture(alloc, "bundles/name_mismatch/SKILL.md");
    defer alloc.free(skill_md);
    const trigger_md = try loadFixture(alloc, "bundles/name_mismatch/TRIGGER.md");
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

test "fixture bundles/platform_ops_installed_default — post-substitution TRIGGER.md parses" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "bundles/platform_ops_installed_default/TRIGGER.md");
    defer alloc.free(md);
    var parsed = try config.parseTriggerMarkdownWithJson(alloc, md);
    defer parsed.deinit(alloc);
    const cfg = &parsed.config;
    try std.testing.expectEqualStrings("platform-ops-agent", cfg.name);
    try std.testing.expectEqualStrings("accounts/fireworks/models/kimi-k2.6", cfg.model.?);
    try std.testing.expectEqual(@as(u32, 256000), cfg.context.?.context_cap_tokens);
    try std.testing.expectEqual(@as(usize, 1), cfg.triggers.len);
    try std.testing.expectEqualStrings("github", cfg.triggers[0].webhook.source);
}

test "fixture bundles/platform_ops_installed_self_managed — sentinel model/cap parses to null/zero" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "bundles/platform_ops_installed_self_managed/TRIGGER.md");
    defer alloc.free(md);
    var parsed = try config.parseTriggerMarkdownWithJson(alloc, md);
    defer parsed.deinit(alloc);
    const cfg = &parsed.config;
    // Empty-string model becomes null (self-managed overlay sentinel); cap stays 0.
    try std.testing.expect(cfg.model == null);
    try std.testing.expectEqual(@as(u32, 0), cfg.context.?.context_cap_tokens);
}

test "shipped sample samples/fixtures/platform-ops-sample SKILL.md frontmatter validates" {
    // Note: the trigger side of platform-ops uses `type: chat` and tools
    // (`http_request`, `memory_*`, `cron_*`) that the registry in
    // config_helpers.zig does not yet recognize. That is a pre-existing
    // drift between the shipped sample and the parser's known-types/
    // known-tools lists — surfaced here, fixed in a follow-up spec.
    // For M46 we only assert the SKILL.md side validates, which is the
    // half this milestone added.
    const alloc = std.testing.allocator;
    const skill_md = try std.Io.Dir.cwd().readFileAlloc(common.globalIo(), "samples/fixtures/platform-ops-sample/SKILL.md", alloc, .limited(64 * 1024));
    defer alloc.free(skill_md);
    var meta = try config.parseSkillMetadata(alloc, skill_md);
    defer meta.deinit(alloc);
    try std.testing.expectEqualStrings("platform-ops-agent", meta.name);
}
