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
const PLATFORM_OPS_FIXTURE_NAME = "platform-ops";
const PLATFORM_OPS_SKILL_PATH = "platform-ops/SKILL.md";
const PLATFORM_OPS_TRIGGER_PATH = "platform-ops/TRIGGER.md";
const MODEL_PLACEHOLDER = "{{model}}";
const MODEL_VALUE = "accounts/fireworks/models/kimi-k2.6";
const CONTEXT_CAP_PLACEHOLDER = "{{context_cap_tokens}}";
const CONTEXT_CAP_VALUE = "256000";
const ZOHO_DAILY_SUMMARIZER = "zoho-sprint-daily-summarizer";
const ZOHO_CRON_SCHEDULE = "0 9 * * *";
const ZOHO_CRON_TIMEZONE = "Asia/Kolkata";
const ZOHO_CRON_MESSAGE = "Summarize today's Zoho Sprints activity";
const TOOL_HTTP_REQUEST = "http_request";
// One slug per first-party bundle, and the slug IS the identity: it names the
// fixture directory, the `agentsfleet/<slug>` repository operators onboard from,
// the `name:` both SKILL.md and TRIGGER.md must declare, and — because the
// importer takes the catalog row id straight from that frontmatter name — the
// fleet-library id. A bundle whose declared name drifts from its directory
// onboards as a second catalog entry instead of filling the seeded one, so the
// list is a single string rather than a (name, directory) pair that can disagree.
const FIRST_PARTY_FIXTURE_SLUGS = [_][]const u8{
    "github-pr-reviewer",
    "security-reviewer",
    ZOHO_DAILY_SUMMARIZER,
    "zoho-recruiter-daily-summarizer",
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
    for (FIRST_PARTY_FIXTURE_SLUGS) |slug| {
        const skill_path = try std.fmt.allocPrint(alloc, "{s}/SKILL.md", .{slug});
        defer alloc.free(skill_path);
        const trigger_path = try std.fmt.allocPrint(alloc, "{s}/TRIGGER.md", .{slug});
        defer alloc.free(trigger_path);
        const skill_md = try loadFixture(alloc, skill_path);
        defer alloc.free(skill_md);
        const trigger_md = try loadFixture(alloc, trigger_path);
        defer alloc.free(trigger_md);

        var meta = config.parseSkillMetadata(alloc, skill_md) catch |err| {
            std.debug.print("first-party SKILL fixture failed: {s}: {s}\n", .{ slug, @errorName(err) });
            return err;
        };
        defer meta.deinit(alloc);
        var parsed = config.parseTriggerMarkdownWithJson(alloc, trigger_md) catch |err| {
            std.debug.print("first-party TRIGGER fixture failed: {s}: {s}\n", .{ slug, @errorName(err) });
            return err;
        };
        defer parsed.deinit(alloc);

        // The three-way identity: directory slug == SKILL name == TRIGGER name.
        try std.testing.expectEqualStrings(slug, meta.name);
        try std.testing.expectEqualStrings(meta.name, parsed.config.name);
        try std.testing.expectEqual(@as(usize, 1), parsed.config.tools.len);
        try std.testing.expectEqualStrings(TOOL_HTTP_REQUEST, parsed.config.tools[0]);
    }
}

test "declarative schedule has no local cron tool" {
    const alloc = std.testing.allocator;
    const trigger_path = try std.fmt.allocPrint(alloc, "{s}/TRIGGER.md", .{ZOHO_DAILY_SUMMARIZER});
    defer alloc.free(trigger_path);
    const trigger_md = try loadFixture(alloc, trigger_path);
    defer alloc.free(trigger_md);

    var parsed = try config.parseTriggerMarkdownWithJson(alloc, trigger_md);
    defer parsed.deinit(alloc);

    try std.testing.expectEqualStrings(ZOHO_DAILY_SUMMARIZER, parsed.config.name);
    try std.testing.expectEqual(@as(usize, 1), parsed.config.triggers.len);
    try std.testing.expectEqualStrings(ZOHO_CRON_SCHEDULE, parsed.config.triggers[0].cron.schedule);
    try std.testing.expectEqualStrings(ZOHO_CRON_TIMEZONE, parsed.config.triggers[0].cron.timezone);
    try std.testing.expectEqualStrings(ZOHO_CRON_MESSAGE, parsed.config.triggers[0].cron.message);
    try std.testing.expectEqual(@as(usize, 1), parsed.config.tools.len);
    try std.testing.expectEqualStrings(TOOL_HTTP_REQUEST, parsed.config.tools[0]);
}
