const std = @import("std");
const config_markdown = @import("config_markdown.zig");
const config_types = @import("config_types.zig");

const extractZombieInstructions = config_markdown.extractZombieInstructions;
const parseZombieFromTriggerMarkdown = config_markdown.parseZombieFromTriggerMarkdown;
const ZombieConfigError = config_types.ZombieConfigError;

test "parseZombieFromTriggerMarkdown: parses frontmatter into config" {
    const alloc = std.testing.allocator;
    const trigger_md =
        "---\nname: lead-collector\ntrigger:\n  type: webhook\n  source: agentmail\n" ++
        "chain:\n  - lead-enricher\ncredentials:\n  - agentmail_api_key\n" ++
        "budget:\n  daily_dollars: 5.0\nskills:\n  - agentmail\n---\n\n## Trigger Logic\n";
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
