const std = @import("std");
const config_markdown = @import("config_markdown.zig");
const config_types = @import("config_types.zig");

const extractAgentInstructions = config_markdown.extractAgentInstructions;
const parseTriggerMarkdownWithJson = config_markdown.parseTriggerMarkdownWithJson;
const parseSkillMetadata = config_markdown.parseSkillMetadata;
const AgentConfigError = config_types.AgentConfigError;

test "parseTriggerMarkdownWithJson: parses frontmatter into config" {
    const alloc = std.testing.allocator;
    const trigger_md =
        \\---
        \\name: platform-ops
        \\x-agentsfleet:
        \\  triggers:
        \\    - type: webhook
        \\      source: agentmail
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
    var parsed = try parseTriggerMarkdownWithJson(alloc, trigger_md);
    defer parsed.deinit(alloc);
    const cfg = &parsed.config;
    try std.testing.expectEqualStrings("platform-ops", cfg.name);
    try std.testing.expectEqual(@as(usize, 1), cfg.triggers.len);
    try std.testing.expectEqualStrings("agentmail", cfg.triggers[0].webhook.source);
}

test "parseTriggerMarkdownWithJson: no frontmatter returns error" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(
        AgentConfigError.MissingRequiredField,
        parseTriggerMarkdownWithJson(alloc, "No frontmatter."),
    );
}

test "parseTriggerMarkdownWithJson: unterminated frontmatter returns error" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(
        AgentConfigError.MissingRequiredField,
        parseTriggerMarkdownWithJson(alloc, "---\nname: x\n"),
    );
}

test "extractAgentInstructions: returns body after frontmatter" {
    const md =
        \\---
        \\name: platform-ops
        \\---
        \\
        \\You are a lead collector.
    ;
    const instructions = extractAgentInstructions(md);
    try std.testing.expectEqualStrings("You are a lead collector.", instructions);
}

test "extractAgentInstructions: no frontmatter returns empty" {
    const instructions = extractAgentInstructions("Just plain markdown with no frontmatter.");
    try std.testing.expectEqualStrings("", instructions);
}

test "extractAgentInstructions: foo: ---bar inside YAML is not the closing delim" {
    const md =
        \\---
        \\name: foo: ---bar
        \\---
        \\
        \\Body.
    ;
    const instructions = extractAgentInstructions(md);
    try std.testing.expectEqualStrings("Body.", instructions);
}

test "extractAgentInstructions: empty body after frontmatter" {
    const md =
        \\---
        \\name: x
        \\---
    ;
    const instructions = extractAgentInstructions(md);
    try std.testing.expectEqualStrings("", instructions);
}

test "parseSkillMetadata: required fields populated, optional null" {
    const alloc = std.testing.allocator;
    const skill_md =
        \\---
        \\name: platform-ops-agent
        \\description: Diagnoses platform health.
        \\version: 0.1.0
        \\---
        \\
        \\You are Platform Ops Agent.
    ;
    var meta = try parseSkillMetadata(alloc, skill_md);
    defer meta.deinit(alloc);
    try std.testing.expectEqualStrings("platform-ops-agent", meta.name);
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
        AgentConfigError.MissingRequiredField,
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
        AgentConfigError.InvalidTagFormat,
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
// JSON with `x-agentsfleet:` at the top level and runtime keys nested under
// it. This is the contract the production read-path SQL queries rely on
// (`config_json->'x-agentsfleet'->'triggers'->0->>'source'` etc.). If the
// parser regresses to top-level runtime keys, those queries return null
// and the regression is silent in production until a webhook fails.
test "parseTriggerMarkdownWithJson: JSON shape has x-agentsfleet at top, runtime keys nested" {
    const alloc = std.testing.allocator;
    const trigger_md =
        \\---
        \\name: shape-pin
        \\x-agentsfleet:
        \\  triggers:
        \\    - type: webhook
        \\      source: agentmail
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

    // x-agentsfleet block exists at top.
    const x = root.get("x-agentsfleet") orelse return error.MissingUseagentBlock;
    try std.testing.expect(x == .object);
    try std.testing.expect(x.object.get("triggers") != null);
    try std.testing.expect(x.object.get("tools") != null);
    try std.testing.expect(x.object.get("budget") != null);

    // Runtime keys MUST NOT appear at the top level — that would break
    // config_json->'x-agentsfleet'->'triggers' lookups in production.
    try std.testing.expect(root.get("triggers") == null);
    try std.testing.expect(root.get("tools") == null);
    try std.testing.expect(root.get("budget") == null);

    // Nested values reach down correctly.
    const trigs = x.object.get("triggers").?.array.items;
    try std.testing.expectEqual(@as(usize, 1), trigs.len);
    const trig = trigs[0].object;
    try std.testing.expectEqualStrings("webhook", trig.get("type").?.string);
    try std.testing.expectEqualStrings("agentmail", trig.get("source").?.string);
}
