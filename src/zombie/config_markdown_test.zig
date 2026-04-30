const std = @import("std");
const config_markdown = @import("config_markdown.zig");
const config_types = @import("config_types.zig");

const extractZombieInstructions = config_markdown.extractZombieInstructions;
const parseZombieFromTriggerMarkdown = config_markdown.parseZombieFromTriggerMarkdown;
const parseTriggerMarkdownWithJson = config_markdown.parseTriggerMarkdownWithJson;
const parseSkillMetadata = config_markdown.parseSkillMetadata;
const ZombieConfigError = config_types.ZombieConfigError;

test "parseZombieFromTriggerMarkdown: parses frontmatter into config" {
    const alloc = std.testing.allocator;
    const trigger_md =
        \\---
        \\name: lead-collector
        \\x-usezombie:
        \\  trigger:
        \\    type: webhook
        \\    source: agentmail
        \\  chain:
        \\    - lead-enricher
        \\  credentials:
        \\    - agentmail_api_key
        \\  budget:
        \\    daily_dollars: 5.0
        \\  tools:
        \\    - agentmail
        \\---
        \\
        \\## Trigger Logic
    ;
    var cfg = try parseZombieFromTriggerMarkdown(alloc, trigger_md);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("lead-collector", cfg.name);
    try std.testing.expectEqualStrings("agentmail", cfg.trigger.webhook.source);
    try std.testing.expectEqual(@as(usize, 1), cfg.chain.len);
    try std.testing.expectEqualStrings("lead-enricher", cfg.chain[0]);
}

test "parseZombieFromTriggerMarkdown: no frontmatter returns error" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(
        ZombieConfigError.MissingRequiredField,
        parseZombieFromTriggerMarkdown(alloc, "No frontmatter."),
    );
}

test "parseZombieFromTriggerMarkdown: unterminated frontmatter returns error" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(
        ZombieConfigError.MissingRequiredField,
        parseZombieFromTriggerMarkdown(alloc, "---\nname: x\n"),
    );
}

test "extractZombieInstructions: returns body after frontmatter" {
    const md =
        \\---
        \\name: lead-collector
        \\---
        \\
        \\You are a lead collector.
    ;
    const instructions = extractZombieInstructions(md);
    try std.testing.expectEqualStrings("You are a lead collector.", instructions);
}

test "extractZombieInstructions: no frontmatter returns empty" {
    const instructions = extractZombieInstructions("Just plain markdown with no frontmatter.");
    try std.testing.expectEqualStrings("", instructions);
}

test "extractZombieInstructions: foo: ---bar inside YAML is not the closing delim" {
    const md =
        \\---
        \\name: foo: ---bar
        \\---
        \\
        \\Body.
    ;
    const instructions = extractZombieInstructions(md);
    try std.testing.expectEqualStrings("Body.", instructions);
}

test "extractZombieInstructions: empty body after frontmatter" {
    const md =
        \\---
        \\name: x
        \\---
    ;
    const instructions = extractZombieInstructions(md);
    try std.testing.expectEqualStrings("", instructions);
}

test "parseSkillMetadata: required fields populated, optional null" {
    const alloc = std.testing.allocator;
    const skill_md =
        \\---
        \\name: platform-ops-zombie
        \\description: Diagnoses platform health.
        \\version: 0.1.0
        \\---
        \\
        \\You are Platform Ops Zombie.
    ;
    var meta = try parseSkillMetadata(alloc, skill_md);
    defer meta.deinit(alloc);
    try std.testing.expectEqualStrings("platform-ops-zombie", meta.name);
    try std.testing.expectEqualStrings("Diagnoses platform health.", meta.description);
    try std.testing.expectEqualStrings("0.1.0", meta.version);
    try std.testing.expect(meta.when_to_use == null);
    try std.testing.expect(meta.author == null);
    try std.testing.expect(meta.model == null);
    try std.testing.expectEqual(@as(usize, 0), meta.tags.len);
}

test "parseSkillMetadata: full optional fields parsed" {
    const alloc = std.testing.allocator;
    const skill_md =
        \\---
        \\name: full
        \\description: All fields.
        \\version: 1.2.3
        \\when_to_use: When you need everything
        \\tags: [a, b, c]
        \\author: usezombie
        \\model: claude-sonnet-4-6
        \\---
        \\
        \\Body.
    ;
    var meta = try parseSkillMetadata(alloc, skill_md);
    defer meta.deinit(alloc);
    try std.testing.expectEqualStrings("When you need everything", meta.when_to_use.?);
    try std.testing.expectEqual(@as(usize, 3), meta.tags.len);
    try std.testing.expectEqualStrings("a", meta.tags[0]);
    try std.testing.expectEqualStrings("usezombie", meta.author.?);
    try std.testing.expectEqualStrings("claude-sonnet-4-6", meta.model.?);
}

test "parseSkillMetadata: missing name → MissingRequiredField" {
    const alloc = std.testing.allocator;
    const skill_md =
        \\---
        \\description: No name here.
        \\version: 0.1.0
        \\---
    ;
    try std.testing.expectError(
        ZombieConfigError.MissingRequiredField,
        parseSkillMetadata(alloc, skill_md),
    );
}

test "parseSkillMetadata: non-string tag element → InvalidTagFormat" {
    const alloc = std.testing.allocator;
    const skill_md =
        \\---
        \\name: x
        \\description: Foo
        \\version: 0.1.0
        \\tags: [leads, 42, true]
        \\---
    ;
    try std.testing.expectError(
        ZombieConfigError.InvalidTagFormat,
        parseSkillMetadata(alloc, skill_md),
    );
}

test "parseSkillMetadata: all-string tags pass" {
    const alloc = std.testing.allocator;
    const skill_md =
        \\---
        \\name: x
        \\description: Foo
        \\version: 0.1.0
        \\tags: [leads, email, agentmail]
        \\---
    ;
    var meta = try parseSkillMetadata(alloc, skill_md);
    defer meta.deinit(alloc);
    try std.testing.expectEqual(@as(usize, 3), meta.tags.len);
}

test "parseSkillMetadata: tags as non-array → silently ignored (returns empty)" {
    const alloc = std.testing.allocator;
    const skill_md =
        \\---
        \\name: x
        \\description: Foo
        \\version: 0.1.0
        \\tags: not-an-array
        \\---
    ;
    var meta = try parseSkillMetadata(alloc, skill_md);
    defer meta.deinit(alloc);
    try std.testing.expectEqual(@as(usize, 0), meta.tags.len);
}

test "parseSkillMetadata: unknown top-level keys pass through silently" {
    const alloc = std.testing.allocator;
    const skill_md =
        \\---
        \\name: x
        \\description: Foo
        \\version: 0.1.0
        \\some_future_vendor_key: arbitrary
        \\---
    ;
    var meta = try parseSkillMetadata(alloc, skill_md);
    defer meta.deinit(alloc);
    try std.testing.expectEqualStrings("x", meta.name);
}

// Pins the write-path JSON shape: parseTriggerMarkdownWithJson MUST produce
// JSON with `x-usezombie:` at the top level and runtime keys nested under
// it. This is the contract the 3 production read-path SQL queries rely on
// (config_json->'x-usezombie'->'trigger'->>'source' etc.). If the parser
// regresses to top-level runtime keys, those queries return null and the
// regression is silent in production until a webhook fails.
test "parseTriggerMarkdownWithJson: JSON shape has x-usezombie at top, runtime keys nested" {
    const alloc = std.testing.allocator;
    const trigger_md =
        \\---
        \\name: shape-pin
        \\x-usezombie:
        \\  trigger:
        \\    type: webhook
        \\    source: agentmail
        \\  tools:
        \\    - agentmail
        \\  budget:
        \\    daily_dollars: 1.0
        \\---
    ;
    var parsed = try parseTriggerMarkdownWithJson(alloc, trigger_md);
    defer parsed.deinit(alloc);

    const j = try std.json.parseFromSlice(std.json.Value, alloc, parsed.config_json, .{});
    defer j.deinit();
    const root = j.value.object;

    // x-usezombie block exists at top.
    const x = root.get("x-usezombie") orelse return error.MissingUsezombieBlock;
    try std.testing.expect(x == .object);
    try std.testing.expect(x.object.get("trigger") != null);
    try std.testing.expect(x.object.get("tools") != null);
    try std.testing.expect(x.object.get("budget") != null);

    // Runtime keys MUST NOT appear at the top level — that would break
    // config_json->'x-usezombie'->'trigger' lookups in production.
    try std.testing.expect(root.get("trigger") == null);
    try std.testing.expect(root.get("tools") == null);
    try std.testing.expect(root.get("budget") == null);

    // Nested values reach down correctly.
    const trig = x.object.get("trigger").?.object;
    try std.testing.expectEqualStrings("webhook", trig.get("type").?.string);
    try std.testing.expectEqualStrings("agentmail", trig.get("source").?.string);
}
