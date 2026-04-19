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
const yaml_frontmatter = @import("yaml_frontmatter.zig");

const ZombieConfig = config_types.ZombieConfig;
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

    const json = try yaml_frontmatter.yamlFrontmatterToJson(alloc, fm.yaml);
    defer alloc.free(json);

    return config_parser.parseZombieConfig(alloc, json);
}
