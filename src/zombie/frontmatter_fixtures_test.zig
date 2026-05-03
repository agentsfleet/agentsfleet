// Loads SKILL.md / TRIGGER.md fixtures from samples/fixtures/frontmatter/
// at test time and asserts the expected parser outcome for each. The
// fixtures are user-facing canonical examples (positive + negative); this
// test pins their behavior to the parser so authoring-doc + parser stay
// aligned.
//
// Tests run from the repo root (zig build sets cwd), so paths are relative
// to the project root.

const std = @import("std");
const config = @import("config.zig");

const FIXTURES_BASE = "samples/fixtures/frontmatter";

fn loadFixture(alloc: std.mem.Allocator, rel_path: []const u8) ![]u8 {
    const path = try std.fs.path.join(alloc, &.{ FIXTURES_BASE, rel_path });
    defer alloc.free(path);
    return std.fs.cwd().readFileAlloc(alloc, path, 64 * 1024);
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
        config.ZombieConfigError.MissingRequiredField,
        config.parseSkillMetadata(alloc, md),
    );
}

test "fixture trigger/minimal.md parses" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "trigger/minimal.md");
    defer alloc.free(md);
    var cfg = try config.parseZombieFromTriggerMarkdown(alloc, md);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("minimal-skill", cfg.name);
    try std.testing.expectEqual(@as(usize, 1), cfg.tools.len);
}

test "fixture trigger/full.md parses with full webhook signature" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "trigger/full.md");
    defer alloc.free(md);
    var cfg = try config.parseZombieFromTriggerMarkdown(alloc, md);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("full-skill", cfg.name);
    try std.testing.expectEqualStrings("github", cfg.trigger.webhook.source);
    try std.testing.expect(cfg.trigger.webhook.signature != null);
    try std.testing.expect(cfg.network != null);
    try std.testing.expectEqual(@as(usize, 2), cfg.network.?.allow.len);
    try std.testing.expectEqual(@as(usize, 3), cfg.tools.len);
}

test "fixture trigger/with_model_and_context.md parses model + every context knob" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "trigger/with_model_and_context.md");
    defer alloc.free(md);
    var cfg = try config.parseZombieFromTriggerMarkdown(alloc, md);
    defer cfg.deinit(alloc);
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
        config.ZombieConfigError.RuntimeKeysOutsideBlock,
        config.parseZombieFromTriggerMarkdown(alloc, md),
    );
}

test "fixture trigger/unknown_runtime_key.md → UnknownRuntimeKey" {
    const alloc = std.testing.allocator;
    const md = try loadFixture(alloc, "trigger/unknown_runtime_key.md");
    defer alloc.free(md);
    try std.testing.expectError(
        config.ZombieConfigError.UnknownRuntimeKey,
        config.parseZombieFromTriggerMarkdown(alloc, md),
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
    var cfg = try config.parseZombieFromTriggerMarkdown(alloc, trigger_md);
    defer cfg.deinit(alloc);

    // Both parse cleanly — the cross-file invariant is enforced by the
    // install handler, not the per-file parsers.
    try std.testing.expect(!std.mem.eql(u8, meta.name, cfg.name));
}

test "shipped sample samples/platform-ops SKILL.md frontmatter validates" {
    // Note: the trigger side of platform-ops uses `type: chat` and tools
    // (`http_request`, `memory_*`, `cron_*`) that the registry in
    // config_helpers.zig does not yet recognize. That is a pre-existing
    // drift between the shipped sample and the parser's known-types/
    // known-tools lists — surfaced here, fixed in a follow-up spec.
    // For M46 we only assert the SKILL.md side validates, which is the
    // half this milestone added.
    const alloc = std.testing.allocator;
    const skill_md = try std.fs.cwd().readFileAlloc(alloc, "samples/platform-ops/SKILL.md", 64 * 1024);
    defer alloc.free(skill_md);
    var meta = try config.parseSkillMetadata(alloc, skill_md);
    defer meta.deinit(alloc);
    try std.testing.expectEqualStrings("platform-ops-zombie", meta.name);
}
