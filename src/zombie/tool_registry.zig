// Tool registry — maps skill names to domain allowlists and tool metadata.
//
// Compile-time registry of known skills. Each skill declares:
//   - name: skill identifier (matches TRIGGER.md skills: list)
//   - domains: external API domains the skill needs to reach
//   - description: one-line description for NullClaw agent tool spec
//
// resolveSkillDomains() returns the merged domain list for a set of skills.
// validateSkills() checks that all requested skills are known.
// This registry is the single source of truth for skill → network mapping.

const std = @import("std");
const Allocator = std.mem.Allocator;
const ec = @import("../errors/codes.zig");

const log = std.log.scoped(.tool_registry);

pub const SkillEntry = struct {
    name: []const u8,
    domains: []const []const u8,
    description: []const u8,
};

// Compile-time skill registry. Add new skills here.
pub const SKILL_REGISTRY = [_]SkillEntry{
    .{
        .name = "agentmail",
        .domains = &ec.SKILL_DOMAINS_AGENTMAIL,
        .description = "Send and receive email via Agentmail API",
    },
    .{
        .name = "slack",
        .domains = &ec.SKILL_DOMAINS_SLACK,
        .description = "Read and post messages in Slack channels",
    },
    .{
        .name = "github",
        .domains = &ec.SKILL_DOMAINS_GITHUB,
        .description = "Create PRs, read repos, manage issues on GitHub",
    },
    .{
        .name = "git",
        .domains = &ec.SKILL_DOMAINS_GIT,
        .description = "Clone, branch, commit, and push git repositories",
    },
    .{
        .name = "linear",
        .domains = &ec.SKILL_DOMAINS_LINEAR,
        .description = "Create and manage issues in Linear",
    },
    .{
        .name = "cloudflare",
        .domains = &ec.SKILL_DOMAINS_CLOUDFLARE,
        .description = "Manage Cloudflare tunnels, DNS, and workers",
    },
    .{
        .name = "pagerduty",
        .domains = &ec.SKILL_DOMAINS_PAGERDUTY,
        .description = "Create incidents and manage on-call in PagerDuty",
    },
};

pub const REGISTRY_COUNT: usize = SKILL_REGISTRY.len;

/// Look up a skill entry by name. Returns null for unknown skills.
pub fn findSkill(name: []const u8) ?*const SkillEntry {
    for (&SKILL_REGISTRY) |*entry| {
        if (std.mem.eql(u8, entry.name, name)) return entry;
    }
    return null;
}

/// Collect unique domains for a set of skills. Caller owns returned slice.
pub fn resolveSkillDomains(
    alloc: Allocator,
    skills: []const []const u8,
) ![]const []const u8 {
    var seen = std.StringHashMap(void).init(alloc);
    defer seen.deinit();

    var domains: std.ArrayList([]const u8) = .{};
    errdefer domains.deinit(alloc);

    for (skills) |skill_name| {
        const entry = findSkill(skill_name) orelse continue;
        for (entry.domains) |domain| {
            const gop = try seen.getOrPut(domain);
            if (!gop.found_existing) {
                try domains.append(alloc, domain);
            }
        }
    }
    return domains.toOwnedSlice(alloc);
}

/// Validate that all skills are in the registry. Returns first unknown skill name.
pub fn validateSkills(skills: []const []const u8) ?[]const u8 {
    for (skills) |skill_name| {
        if (findSkill(skill_name) == null) return skill_name;
    }
    return null;
}

// ── Tests ─────────────────────────────────────────────────────────────────

test "findSkill returns entry for known skill" {
    const entry = findSkill("slack").?;
    try std.testing.expectEqualStrings("slack", entry.name);
    try std.testing.expect(entry.domains.len > 0);
    try std.testing.expectEqualStrings("api.slack.com", entry.domains[0]);
}

test "findSkill returns null for unknown skill" {
    try std.testing.expect(findSkill("unknown_tool") == null);
}

test "resolveSkillDomains returns unique domains for multiple skills" {
    const alloc = std.testing.allocator;
    const skills = &[_][]const u8{ "slack", "github" };
    const domains = try resolveSkillDomains(alloc, skills);
    defer alloc.free(domains);

    // slack = api.slack.com, github = api.github.com + github.com
    try std.testing.expect(domains.len == 3);

    var found_slack = false;
    var found_gh_api = false;
    var found_gh = false;
    for (domains) |d| {
        if (std.mem.eql(u8, d, "api.slack.com")) found_slack = true;
        if (std.mem.eql(u8, d, "api.github.com")) found_gh_api = true;
        if (std.mem.eql(u8, d, "github.com")) found_gh = true;
    }
    try std.testing.expect(found_slack);
    try std.testing.expect(found_gh_api);
    try std.testing.expect(found_gh);
}

test "resolveSkillDomains deduplicates shared domains (git + github)" {
    const alloc = std.testing.allocator;
    // git = github.com only, github = api.github.com + github.com
    const skills = &[_][]const u8{ "git", "github" };
    const domains = try resolveSkillDomains(alloc, skills);
    defer alloc.free(domains);

    // git adds github.com, github adds api.github.com + github.com (deduped) = 2
    try std.testing.expectEqual(@as(usize, 2), domains.len);
}

test "resolveSkillDomains returns empty for empty skills" {
    const alloc = std.testing.allocator;
    const skills = &[_][]const u8{};
    const domains = try resolveSkillDomains(alloc, skills);
    defer alloc.free(domains);
    try std.testing.expectEqual(@as(usize, 0), domains.len);
}

test "resolveSkillDomains skips unknown skills without error" {
    const alloc = std.testing.allocator;
    const skills = &[_][]const u8{ "slack", "nonexistent" };
    const domains = try resolveSkillDomains(alloc, skills);
    defer alloc.free(domains);
    // Only slack domains, nonexistent skipped
    try std.testing.expectEqual(@as(usize, 1), domains.len);
}

test "validateSkills returns null for all valid skills" {
    const skills = &[_][]const u8{ "slack", "github", "git" };
    try std.testing.expect(validateSkills(skills) == null);
}

test "validateSkills returns first unknown skill" {
    const skills = &[_][]const u8{ "slack", "bad_tool", "github" };
    const bad = validateSkills(skills).?;
    try std.testing.expectEqualStrings("bad_tool", bad);
}

test "validateSkills returns null for empty list" {
    const skills = &[_][]const u8{};
    try std.testing.expect(validateSkills(skills) == null);
}

test "SKILL_REGISTRY has correct count" {
    try std.testing.expectEqual(@as(usize, 7), REGISTRY_COUNT);
}

test "all registry entries have non-empty name and domains" {
    for (&SKILL_REGISTRY) |*entry| {
        try std.testing.expect(entry.name.len > 0);
        try std.testing.expect(entry.domains.len > 0);
        try std.testing.expect(entry.description.len > 0);
    }
}

