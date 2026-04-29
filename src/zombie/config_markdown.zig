// Zombie config markdown/frontmatter extraction.
//
// TRIGGER.md carries YAML frontmatter between `---` delimiters. Two entry
// points:
//   - extractZombieInstructions: borrow the body below the frontmatter.
//   - parseZombieFromTriggerMarkdown: parse the frontmatter into ZombieConfig.
//
// Both share the same delimiter scanner — a YAML value like `foo: ---bar`
// must not be mistaken for the closing delimiter.

const std = @import("std");
const Allocator = std.mem.Allocator;

const config_types = @import("config_types.zig");
const config_parser = @import("config_parser.zig");
const config_validate = @import("config_validate.zig");
const yaml_frontmatter = @import("yaml_frontmatter.zig");

const ZombieConfig = config_types.ZombieConfig;
const SkillMetadata = config_types.SkillMetadata;
const ZombieConfigError = config_types.ZombieConfigError;

/// Return value of the frontmatter scanner.
const Frontmatter = struct {
    yaml: []const u8, // slice between the opening and closing `---`
    body: []const u8, // slice after the closing `---`, trimmed
};

/// Locate the YAML frontmatter in `markdown` and return the YAML and body
/// slices (both borrowed from `markdown`). Returns null if no well-formed
/// frontmatter block is present.
fn scanFrontmatter(markdown: []const u8) ?Frontmatter {
    const trimmed = std.mem.trim(u8, markdown, " \t\r\n");
    if (!std.mem.startsWith(u8, trimmed, "---")) return null;

    const after_open = trimmed[3..];
    const close = findClosingDelim(after_open) orelse return null;

    const yaml = after_open[0..close];
    const after_close = after_open[close + 4 ..];
    const body = if (after_close.len > 0 and after_close[0] == '\n')
        after_close[1..]
    else
        after_close;
    return .{ .yaml = yaml, .body = std.mem.trim(u8, body, " \t\r\n") };
}

/// Return the index of the closing `\n---` in `haystack` such that the
/// match is followed by `\n`, `\r`, or end-of-input. Guards against
/// `foo: ---bar` being mistaken for a delimiter.
fn findClosingDelim(haystack: []const u8) ?usize {
    var search_from: usize = 0;
    while (std.mem.indexOfPos(u8, haystack, search_from, "\n---")) |pos| {
        const rest = haystack[pos + 4 ..];
        if (rest.len == 0 or rest[0] == '\n' or rest[0] == '\r') return pos;
        search_from = pos + 1;
    }
    return null;
}

/// Returns the markdown body that follows the YAML frontmatter. Borrowed
/// slice — caller must not free it; lifetime is tied to `source_markdown`.
/// Returns an empty slice if no frontmatter is present.
pub fn extractZombieInstructions(source_markdown: []const u8) []const u8 {
    const fm = scanFrontmatter(source_markdown) orelse return "";
    return fm.body;
}

/// Extract YAML frontmatter from TRIGGER.md, convert it to JSON, and parse
/// into ZombieConfig. Caller owns the returned config and must call deinit.
pub fn parseZombieFromTriggerMarkdown(
    alloc: Allocator,
    trigger_markdown: []const u8,
) (Allocator.Error || ZombieConfigError)!ZombieConfig {
    const fm = scanFrontmatter(trigger_markdown) orelse
        return ZombieConfigError.MissingRequiredField;

    const json = yaml_frontmatter.yamlFrontmatterToJson(alloc, fm.yaml) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        error.ParseFailure => return ZombieConfigError.MissingRequiredField,
    };
    defer alloc.free(json);

    return config_parser.parseZombieConfig(alloc, json);
}

/// Aggregate returned by `parseTriggerMarkdownWithJson` — the parsed
/// ZombieConfig plus the JSON that was used to derive it. The JSON is the
/// canonical `config_json` shape (what `parseZombieConfig` accepts) and is
/// what the install handler persists into core.zombies. Owning both lets
/// callers store the JSON without re-serializing the config.
pub const ParsedTrigger = struct {
    config: ZombieConfig,
    config_json: []u8,

    pub fn deinit(self: *ParsedTrigger, alloc: Allocator) void {
        self.config.deinit(alloc);
        alloc.free(self.config_json);
    }
};

/// Same pipeline as `parseZombieFromTriggerMarkdown`, but also returns the
/// intermediate JSON. Callers (the install handler) want the JSON for
/// persistence and the parsed config for name extraction + validation
/// without doing the work twice.
pub fn parseTriggerMarkdownWithJson(
    alloc: Allocator,
    trigger_markdown: []const u8,
) (Allocator.Error || ZombieConfigError)!ParsedTrigger {
    const fm = scanFrontmatter(trigger_markdown) orelse
        return ZombieConfigError.MissingRequiredField;

    const json = yaml_frontmatter.yamlFrontmatterToJson(alloc, fm.yaml) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        error.ParseFailure => return ZombieConfigError.MissingRequiredField,
    };
    errdefer alloc.free(json);

    const config = try config_parser.parseZombieConfig(alloc, json);
    return .{ .config = config, .config_json = json };
}

/// Parse SKILL.md frontmatter into a SkillMetadata struct. Validates required
/// top-level keys (`name`, `description`, `version`); optional fields
/// (`when_to_use`, `tags`, `author`, `model`) pass through. Unknown top-level
/// keys are accepted silently — top level is permissive (other skill-host
/// vendors may add their own).
pub fn parseSkillMetadata(
    alloc: Allocator,
    source_markdown: []const u8,
) (Allocator.Error || ZombieConfigError)!SkillMetadata {
    const fm = scanFrontmatter(source_markdown) orelse
        return ZombieConfigError.MissingRequiredField;

    const json = yaml_frontmatter.yamlFrontmatterToJson(alloc, fm.yaml) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        error.ParseFailure => return ZombieConfigError.MissingRequiredField,
    };
    defer alloc.free(json);

    const parsed = std.json.parseFromSlice(std.json.Value, alloc, json, .{}) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return ZombieConfigError.MissingRequiredField,
    };
    defer parsed.deinit();

    const root = switch (parsed.value) {
        .object => |o| o,
        else => return ZombieConfigError.MissingRequiredField,
    };

    return buildSkillMetadata(alloc, root);
}

fn buildSkillMetadata(
    alloc: Allocator,
    root: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)!SkillMetadata {
    const name = try dupeRequiredString(alloc, root, "name");
    errdefer alloc.free(name);
    try config_validate.validateSkillName(name);
    const description = try dupeRequiredString(alloc, root, "description");
    errdefer alloc.free(description);
    const version = try dupeRequiredString(alloc, root, "version");
    errdefer alloc.free(version);
    try config_validate.validateSkillVersion(version);

    const when_to_use = try dupeOptionalString(alloc, root, "when_to_use");
    errdefer if (when_to_use) |s| alloc.free(s);
    const author = try dupeOptionalString(alloc, root, "author");
    errdefer if (author) |s| alloc.free(s);
    const model = try dupeOptionalString(alloc, root, "model");
    errdefer if (model) |s| alloc.free(s);

    const tags = try dupeOptionalStringArray(alloc, root, "tags");

    return .{
        .name = name,
        .description = description,
        .version = version,
        .when_to_use = when_to_use,
        .tags = tags,
        .author = author,
        .model = model,
    };
}

fn dupeRequiredString(
    alloc: Allocator,
    root: std.json.ObjectMap,
    key: []const u8,
) (Allocator.Error || ZombieConfigError)![]const u8 {
    const val = root.get(key) orelse return ZombieConfigError.MissingRequiredField;
    const s = switch (val) {
        .string => |str| str,
        else => return ZombieConfigError.MissingRequiredField,
    };
    if (s.len == 0) return ZombieConfigError.MissingRequiredField;
    return try alloc.dupe(u8, s);
}

fn dupeOptionalString(
    alloc: Allocator,
    root: std.json.ObjectMap,
    key: []const u8,
) Allocator.Error!?[]const u8 {
    const val = root.get(key) orelse return null;
    const s = switch (val) {
        .string => |str| str,
        else => return null,
    };
    if (s.len == 0) return null;
    return try alloc.dupe(u8, s);
}

fn dupeOptionalStringArray(
    alloc: Allocator,
    root: std.json.ObjectMap,
    key: []const u8,
) Allocator.Error![]const []const u8 {
    const val = root.get(key) orelse return try alloc.alloc([]const u8, 0);
    const arr = switch (val) {
        .array => |a| a,
        else => return try alloc.alloc([]const u8, 0),
    };
    var out = try alloc.alloc([]const u8, arr.items.len);
    errdefer alloc.free(out);
    var i: usize = 0;
    errdefer for (out[0..i]) |s| alloc.free(s);
    while (i < arr.items.len) : (i += 1) {
        const s = switch (arr.items[i]) {
            .string => |str| str,
            else => "",
        };
        out[i] = try alloc.dupe(u8, s);
    }
    return out;
}

test "parseTriggerMarkdownWithJson: returns both parsed config and owned JSON" {
    const alloc = std.testing.allocator;
    const trigger_md =
        \\---
        \\name: test-zombie
        \\x-usezombie:
        \\  trigger:
        \\    type: api
        \\  tools:
        \\    - agentmail
        \\  budget:
        \\    daily_dollars: 1.0
        \\---
    ;
    var parsed = try parseTriggerMarkdownWithJson(alloc, trigger_md);
    defer parsed.deinit(alloc);
    try std.testing.expectEqualStrings("test-zombie", parsed.config.name);
    var reparsed = try config_parser.parseZombieConfig(alloc, parsed.config_json);
    defer reparsed.deinit(alloc);
    try std.testing.expectEqualStrings("test-zombie", reparsed.name);
}

test "parseTriggerMarkdownWithJson: missing frontmatter → MissingRequiredField" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(
        ZombieConfigError.MissingRequiredField,
        parseTriggerMarkdownWithJson(alloc, "no frontmatter here"),
    );
}

test "parseTriggerMarkdownWithJson: parse failure inside frontmatter → JSON freed (no leak)" {
    const alloc = std.testing.allocator;
    // Missing required `name:` — yaml→json succeeds, parseZombieConfig fails.
    // The errdefer must free the JSON. std.testing.allocator panics on leak.
    const trigger_md =
        \\---
        \\x-usezombie:
        \\  trigger:
        \\    type: api
        \\  tools:
        \\    - agentmail
        \\  budget:
        \\    daily_dollars: 1.0
        \\---
    ;
    try std.testing.expectError(
        ZombieConfigError.MissingRequiredField,
        parseTriggerMarkdownWithJson(alloc, trigger_md),
    );
}
