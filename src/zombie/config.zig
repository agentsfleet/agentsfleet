// Zombie configuration parser.
//
// M2_002: directory-based zombie format (SKILL.md + TRIGGER.md).
// zombiectl up sends both files raw. The server parses TRIGGER.md frontmatter
// into config_json via parseZombieFromTriggerMarkdown. SKILL.md is stored as-is.
// At claim time, the worker calls:
//   - parseZombieConfig(alloc, config_json_bytes)  → ZombieConfig struct
//   - extractZombieInstructions(source_markdown)    → system prompt slice (borrowed)

const std = @import("std");
const Allocator = std.mem.Allocator;
const yaml_frontmatter = @import("yaml_frontmatter.zig");
const helpers = @import("config_helpers.zig");

const log = std.log.scoped(.zombie_config);

const parseZombieTrigger = helpers.parseZombieTrigger;
const parseZombieNetwork = helpers.parseZombieNetwork;
const parseZombieBudget = helpers.parseZombieBudget;
const dupeStringArray = helpers.dupeStringArray;
const freeStringSlice = helpers.freeStringSlice;
const freeZombieTrigger = helpers.freeZombieTrigger;
const isKnownZombieSkill = helpers.isKnownZombieSkill;

pub const ZombieConfigError = error{
    MissingRequiredField,
    InvalidTriggerType,
    InvalidTriggerSource,
    UnknownSkill,
    InvalidCredentialRef,
    InvalidBudget,
};

pub const ZombieStatus = enum {
    active,
    paused,
    stopped,
    killed,

    pub fn toSlice(self: ZombieStatus) []const u8 {
        return switch (self) {
            .active => "active",
            .paused => "paused",
            .stopped => "stopped",
            .killed => "killed",
        };
    }

    pub fn fromSlice(s: []const u8) ?ZombieStatus {
        if (std.mem.eql(u8, s, "active")) return .active;
        if (std.mem.eql(u8, s, "paused")) return .paused;
        if (std.mem.eql(u8, s, "stopped")) return .stopped;
        if (std.mem.eql(u8, s, "killed")) return .killed;
        return null;
    }

    pub fn isTerminal(self: ZombieStatus) bool {
        return self == .killed;
    }

    pub fn isRunnable(self: ZombieStatus) bool {
        return self == .active;
    }
};

pub const ZombieTriggerType = enum { webhook, cron, api, chain };

/// Tagged union for trigger config. Each variant carries only the fields it needs,
/// making invalid states (e.g. webhook without source) unrepresentable.
pub const ZombieTrigger = union(ZombieTriggerType) {
    webhook: struct { source: []const u8, event: ?[]const u8 },
    cron: struct { schedule: []const u8 },
    api: void,
    chain: struct { source: []const u8 },
};

pub const ZombieBudget = struct {
    daily_dollars: f64,
    monthly_dollars: ?f64,
};

pub const ZombieNetwork = struct {
    allow: []const []const u8,
};

const config_gates = @import("config_gates.zig");
pub const GateBehavior = config_gates.GateBehavior;
pub const GateRule = config_gates.GateRule;
pub const AnomalyPattern = config_gates.AnomalyPattern;
pub const AnomalyRule = config_gates.AnomalyRule;
pub const GatePolicy = config_gates.GatePolicy;

pub const ZombieConfig = struct {
    name: []const u8,
    trigger: ZombieTrigger,
    skills: []const []const u8,
    credentials: []const []const u8,
    network: ?ZombieNetwork,
    budget: ZombieBudget,
    gates: ?GatePolicy,
    // M2_002: ClaHub skill reference (e.g. "clawhub://queen/lead-hunter@1.0.1")
    // Resolution deferred to M3 — stored but not fetched.
    skill: ?[]const u8,
    // M2_002: Downstream zombies to chain events to.
    chain: []const []const u8,

    pub fn deinit(self: *const ZombieConfig, alloc: Allocator) void {
        alloc.free(self.name);
        switch (self.trigger) {
            .webhook => |w| {
                alloc.free(w.source);
                if (w.event) |e| alloc.free(e);
            },
            .cron => |c| alloc.free(c.schedule),
            .chain => |ch| alloc.free(ch.source),
            .api => {},
        }
        freeStringSlice(alloc, self.skills);
        freeStringSlice(alloc, self.credentials);
        if (self.network) |net| freeStringSlice(alloc, net.allow);
        if (self.gates) |gates| config_gates.freeGatePolicy(alloc, gates);
        if (self.skill) |s| alloc.free(s);
        freeStringSlice(alloc, self.chain);
    }
};

// parseZombieConfig parses the config_json column from core.zombies into a ZombieConfig.
// config_json is server-computed from TRIGGER.md frontmatter (M2_002).
// Caller owns the returned ZombieConfig and must call deinit.
pub fn parseZombieConfig(alloc: Allocator, config_json: []const u8) (Allocator.Error || ZombieConfigError)!ZombieConfig {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, config_json, .{}) catch {
        return ZombieConfigError.MissingRequiredField;
    };
    defer parsed.deinit();

    const root = switch (parsed.value) {
        .object => |o| o,
        else => return ZombieConfigError.MissingRequiredField,
    };

    // name — required, non-empty string
    const name = blk: {
        const val = root.get("name") orelse return ZombieConfigError.MissingRequiredField;
        const s = switch (val) {
            .string => |str| str,
            else => return ZombieConfigError.MissingRequiredField,
        };
        if (s.len == 0) return ZombieConfigError.MissingRequiredField;
        break :blk try alloc.dupe(u8, s);
    };
    errdefer alloc.free(name);

    // trigger — required object
    const trigger = blk: {
        const val = root.get("trigger") orelse return ZombieConfigError.MissingRequiredField;
        const obj = switch (val) {
            .object => |o| o,
            else => return ZombieConfigError.MissingRequiredField,
        };
        break :blk try parseZombieTrigger(alloc, obj);
    };
    errdefer freeZombieTrigger(alloc, trigger);

    // skills — required non-empty array
    const skills = blk: {
        const val = root.get("skills") orelse return ZombieConfigError.MissingRequiredField;
        const arr = switch (val) {
            .array => |a| a,
            else => return ZombieConfigError.MissingRequiredField,
        };
        if (arr.items.len == 0) return ZombieConfigError.MissingRequiredField;
        break :blk try dupeStringArray(alloc, arr.items);
    };
    errdefer freeStringSlice(alloc, skills);

    // credentials — optional; defaults to empty
    const credentials = blk: {
        const val = root.get("credentials") orelse break :blk try alloc.alloc([]const u8, 0);
        const arr = switch (val) {
            .array => |a| a,
            else => return ZombieConfigError.MissingRequiredField,
        };
        break :blk try dupeStringArray(alloc, arr.items);
    };
    errdefer freeStringSlice(alloc, credentials);

    // network — optional
    const network = blk: {
        const val = root.get("network") orelse break :blk null;
        const obj = switch (val) {
            .object => |o| o,
            else => return ZombieConfigError.MissingRequiredField,
        };
        break :blk try parseZombieNetwork(alloc, obj);
    };
    errdefer if (network) |net| freeStringSlice(alloc, net.allow);

    // budget — required object
    const budget = blk: {
        const val = root.get("budget") orelse return ZombieConfigError.MissingRequiredField;
        const obj = switch (val) {
            .object => |o| o,
            else => return ZombieConfigError.MissingRequiredField,
        };
        break :blk try parseZombieBudget(obj);
    };

    // gates — optional
    const gates: ?config_gates.GatePolicy = blk: {
        const val = root.get("gates") orelse break :blk null;
        const obj = switch (val) {
            .object => |o| o,
            else => return ZombieConfigError.MissingRequiredField,
        };
        break :blk config_gates.parseGatePolicy(alloc, obj) catch return ZombieConfigError.MissingRequiredField;
    };
    errdefer if (gates) |g| config_gates.freeGatePolicy(alloc, g);

    try validateSkillsAndCredentials(skills, credentials);

    const extended = try parseExtendedFields(alloc, root);

    return ZombieConfig{
        .name = name,
        .trigger = trigger,
        .skills = skills,
        .credentials = credentials,
        .network = network,
        .budget = budget,
        .gates = gates,
        .skill = extended.skill,
        .chain = extended.chain,
    };
}

const ExtendedFields = struct { skill: ?[]const u8, chain: []const []const u8 };

fn validateSkillsAndCredentials(skills: []const []const u8, credentials: []const []const u8) ZombieConfigError!void {
    for (skills) |skill_name| {
        if (!isKnownZombieSkill(skill_name)) return ZombieConfigError.UnknownSkill;
    }
    const MAX_CREDENTIAL_NAME_LEN = 128;
    for (credentials) |cred| {
        if (cred.len == 0 or cred.len > MAX_CREDENTIAL_NAME_LEN) return ZombieConfigError.InvalidCredentialRef;
        for (cred) |c| {
            if (!std.ascii.isAlphanumeric(c) and c != '_') return ZombieConfigError.InvalidCredentialRef;
        }
    }
}

fn parseExtendedFields(alloc: Allocator, root: std.json.ObjectMap) (Allocator.Error || ZombieConfigError)!ExtendedFields {
    const skill_ref: ?[]const u8 = blk: {
        const val = root.get("skill") orelse break :blk null;
        const s = switch (val) {
            .string => |str| str,
            else => break :blk null,
        };
        if (s.len == 0) break :blk null;
        break :blk try alloc.dupe(u8, s);
    };
    errdefer if (skill_ref) |s| alloc.free(s);

    const chain_arr: []const []const u8 = blk: {
        const val = root.get("chain") orelse break :blk try alloc.alloc([]const u8, 0);
        const arr = switch (val) {
            .array => |a| a,
            else => break :blk try alloc.alloc([]const u8, 0),
        };
        break :blk try dupeStringArray(alloc, arr.items);
    };

    return .{ .skill = skill_ref, .chain = chain_arr };
}

// extractZombieInstructions returns the markdown body from source_markdown after
// the closing --- of the YAML frontmatter. Returns a borrowed slice — caller must
// not free it; it points into source_markdown.
// Returns empty slice if no frontmatter delimiter is found.
pub fn extractZombieInstructions(source_markdown: []const u8) []const u8 {
    const trimmed = std.mem.trim(u8, source_markdown, " \t\r\n");
    if (!std.mem.startsWith(u8, trimmed, "---")) return "";

    // Find the closing --- on its own line.
    // Require the match is followed by \n, \r, or end-of-input so that a YAML
    // value like "foo: ---bar" cannot match.
    const after_open = trimmed[3..];
    const close = blk: {
        var search_from: usize = 0;
        while (std.mem.indexOfPos(u8, after_open, search_from, "\n---")) |pos| {
            const rest = after_open[pos + 4 ..];
            if (rest.len == 0 or rest[0] == '\n' or rest[0] == '\r') break :blk pos;
            search_from = pos + 1;
        }
        return "";
    };
    const after_close = after_open[close + 4 ..];

    // Skip the newline immediately following the closing ---
    const body = if (after_close.len > 0 and after_close[0] == '\n')
        after_close[1..]
    else
        after_close;

    return std.mem.trim(u8, body, " \t\r\n");
}

// validateZombieSkills checks every skill in the config against the known registry.
// Call this at upload time (API handler) to give the developer an actionable error.
pub fn validateZombieSkills(config: ZombieConfig) ZombieConfigError!void {
    for (config.skills) |skill| {
        if (!isKnownZombieSkill(skill)) {
            log.warn("zombie_config.validate unknown_skill={s}", .{skill});
            return ZombieConfigError.UnknownSkill;
        }
    }
}

// parseZombieFromTriggerMarkdown extracts YAML frontmatter from TRIGGER.md,
// converts it to JSON via the Zig JSON parser, and returns a ZombieConfig.
// The frontmatter is expected between --- delimiters. The body after the
// closing --- is ignored (it's human-readable documentation).
// Caller owns the returned ZombieConfig and must call deinit.
pub fn parseZombieFromTriggerMarkdown(alloc: Allocator, trigger_markdown: []const u8) (Allocator.Error || ZombieConfigError)!ZombieConfig {
    const trimmed = std.mem.trim(u8, trigger_markdown, " \t\r\n");
    if (!std.mem.startsWith(u8, trimmed, "---")) return ZombieConfigError.MissingRequiredField;

    const after_open = trimmed[3..];
    const close = blk: {
        var search_from: usize = 0;
        while (std.mem.indexOfPos(u8, after_open, search_from, "\n---")) |pos| {
            const rest = after_open[pos + 4 ..];
            if (rest.len == 0 or rest[0] == '\n' or rest[0] == '\r') break :blk pos;
            search_from = pos + 1;
        }
        return ZombieConfigError.MissingRequiredField;
    };
    const yaml_block = after_open[0..close];

    // Convert simple YAML frontmatter to JSON using line-by-line conversion.
    // This handles the flat/nested structure of TRIGGER.md frontmatter.
    const json = yaml_frontmatter.yamlFrontmatterToJson(alloc, yaml_block) catch {
        return ZombieConfigError.MissingRequiredField;
    };
    defer alloc.free(json);

    return parseZombieConfig(alloc, json);
}

test "parseZombieConfig: valid config parses all fields" {
    const alloc = std.testing.allocator;
    const json = "{\"name\":\"lead-collector\",\"trigger\":{\"type\":\"webhook\",\"source\":\"agentmail\",\"event\":\"message.received\"},\"skills\":[\"agentmail\"],\"credentials\":[\"agentmail_api_key\"],\"network\":{\"allow\":[\"api.agentmail.to\"]},\"budget\":{\"daily_dollars\":5.0},\"chain\":[\"lead-enricher\"]}";
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("lead-collector", cfg.name);
    try std.testing.expectEqualStrings("agentmail", cfg.trigger.webhook.source);
    try std.testing.expectApproxEqAbs(@as(f64, 5.0), cfg.budget.daily_dollars, 0.001);
    try std.testing.expectEqual(@as(usize, 1), cfg.chain.len);
    try std.testing.expectEqualStrings("lead-enricher", cfg.chain[0]);
    try std.testing.expect(cfg.skill == null);
}

test "parseZombieConfig: missing name returns MissingRequiredField" {
    const alloc = std.testing.allocator;
    const json =
        \\{"trigger": {"type": "webhook", "source": "agentmail"}, "skills": ["agentmail"], "budget": {"daily_dollars": 1.0}}
    ;
    try std.testing.expectError(ZombieConfigError.MissingRequiredField, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: invalid trigger type returns InvalidTriggerType" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name": "x", "trigger": {"type": "invalid"}, "skills": ["agentmail"], "budget": {"daily_dollars": 1.0}}
    ;
    try std.testing.expectError(ZombieConfigError.InvalidTriggerType, parseZombieConfig(alloc, json));
}

test "validateZombieSkills: unknown skill returns UnknownSkill" {
    const alloc = std.testing.allocator;
    const json =
        \\{"name": "x", "trigger": {"type": "api"}, "skills": ["unknown_tool"], "budget": {"daily_dollars": 1.0}}
    ;
    try std.testing.expectError(ZombieConfigError.UnknownSkill, parseZombieConfig(alloc, json));
}

test "parseZombieConfig: skill field parsed from JSON" {
    const alloc = std.testing.allocator;
    const json = "{\"name\":\"enricher\",\"trigger\":{\"type\":\"chain\",\"source\":\"lead-collector\"},\"skills\":[\"agentmail\"],\"skill\":\"clawhub://queen/lead-hunter@1.0.1\",\"budget\":{\"daily_dollars\":2.0}}";
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("clawhub://queen/lead-hunter@1.0.1", cfg.skill.?);
    try std.testing.expectEqualStrings("lead-collector", cfg.trigger.chain.source);
}

test "parseZombieConfig: credential names validated (no op:// paths)" {
    const alloc = std.testing.allocator;
    const json = "{\"name\":\"x\",\"trigger\":{\"type\":\"api\"},\"skills\":[\"agentmail\"],\"credentials\":[\"op://ZMB_LOCAL_DEV/agentmail/api_key\"],\"budget\":{\"daily_dollars\":1.0}}";
    try std.testing.expectError(ZombieConfigError.InvalidCredentialRef, parseZombieConfig(alloc, json));
}

test "parseZombieFromTriggerMarkdown: parses frontmatter into config" {
    const alloc = std.testing.allocator;
    const trigger_md = "---\nname: lead-collector\ntrigger:\n  type: webhook\n  source: agentmail\nchain:\n  - lead-enricher\ncredentials:\n  - agentmail_api_key\nbudget:\n  daily_dollars: 5.0\nskills:\n  - agentmail\n---\n\n## Trigger Logic\n";
    var cfg = try parseZombieFromTriggerMarkdown(alloc, trigger_md);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("lead-collector", cfg.name);
    try std.testing.expectEqualStrings("agentmail", cfg.trigger.webhook.source);
    try std.testing.expectEqual(@as(usize, 1), cfg.chain.len);
    try std.testing.expectEqualStrings("lead-enricher", cfg.chain[0]);
}

test "parseZombieFromTriggerMarkdown: no frontmatter returns error" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(ZombieConfigError.MissingRequiredField, parseZombieFromTriggerMarkdown(alloc, "No frontmatter."));
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
