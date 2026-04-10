// Zombie configuration parser.
//
// The developer writes a .md file with YAML frontmatter + freeform instructions.
// zombiectl up parses the YAML frontmatter → JSON and uploads both to the API.
// The API stores source_markdown (raw .md) and config_json (compiled JSON) in core.zombies.
// At claim time, the worker calls:
//   - parseZombieConfig(alloc, config_json_bytes)  → ZombieConfig struct
//   - extractZombieInstructions(source_markdown)    → system prompt slice (borrowed)

const std = @import("std");
const Allocator = std.mem.Allocator;

const log = std.log.scoped(.zombie_config);

// Built-in skills. clawhub:// registry refs are also accepted (must be pinned).
const KNOWN_ZOMBIE_SKILLS = [_][]const u8{
    "agentmail", "slack",      "github",    "git",
    "linear",    "cloudflare", "pagerduty",
};

pub const ZombieConfigError = error{
    MissingRequiredField,
    InvalidTriggerType,
    InvalidTriggerSource,
    UnknownSkill,
    InvalidCredentialRef,
    InvalidBudget,
};

pub const ZombieTriggerType = enum { webhook, cron, api };

pub const ZombieTrigger = struct {
    trigger_type: ZombieTriggerType,
    // webhook: required. cron/api: null.
    source: ?[]const u8,
    // optional event filter, e.g. "message.received"
    event: ?[]const u8,
    // cron: required. webhook/api: null.
    schedule: ?[]const u8,
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

    pub fn deinit(self: *const ZombieConfig, alloc: Allocator) void {
        alloc.free(self.name);
        if (self.trigger.source) |s| alloc.free(s);
        if (self.trigger.event) |e| alloc.free(e);
        if (self.trigger.schedule) |s| alloc.free(s);
        freeStringSlice(alloc, self.skills);
        freeStringSlice(alloc, self.credentials);
        if (self.network) |net| freeStringSlice(alloc, net.allow);
        if (self.gates) |gates| config_gates.freeGatePolicy(alloc, gates);
    }
};

// parseZombieConfig parses the config_json column from core.zombies into a ZombieConfig.
// config_json was produced by zombiectl: YAML frontmatter → JSON conversion.
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

    // validate skills against known registry
    for (skills) |skill| {
        if (!isKnownZombieSkill(skill)) return ZombieConfigError.UnknownSkill;
    }
    // validate credentials are op:// vault refs
    for (credentials) |cred| {
        if (!std.mem.startsWith(u8, cred, "op://")) return ZombieConfigError.InvalidCredentialRef;
    }

    return ZombieConfig{
        .name = name,
        .trigger = trigger,
        .skills = skills,
        .credentials = credentials,
        .network = network,
        .budget = budget,
        .gates = gates,
    };
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

// --- internal helpers ---

fn parseZombieTrigger(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || ZombieConfigError)!ZombieTrigger {
    const type_val = obj.get("type") orelse return ZombieConfigError.MissingRequiredField;
    const type_str = switch (type_val) {
        .string => |s| s,
        else => return ZombieConfigError.MissingRequiredField,
    };
    const trigger_type = if (std.mem.eql(u8, type_str, "webhook"))
        ZombieTriggerType.webhook
    else if (std.mem.eql(u8, type_str, "cron"))
        ZombieTriggerType.cron
    else if (std.mem.eql(u8, type_str, "api"))
        ZombieTriggerType.api
    else
        return ZombieConfigError.InvalidTriggerType;

    const source: ?[]const u8 = blk: {
        const val = obj.get("source") orelse break :blk null;
        const s = switch (val) {
            .string => |str| str,
            else => return ZombieConfigError.InvalidTriggerSource,
        };
        break :blk try alloc.dupe(u8, s);
    };
    errdefer if (source) |s| alloc.free(s);

    // webhook requires source
    if (trigger_type == .webhook and source == null) return ZombieConfigError.InvalidTriggerSource;

    const event: ?[]const u8 = blk: {
        const val = obj.get("event") orelse break :blk null;
        const s = switch (val) {
            .string => |str| str,
            else => break :blk null,
        };
        break :blk try alloc.dupe(u8, s);
    };
    errdefer if (event) |e| alloc.free(e);

    const schedule: ?[]const u8 = blk: {
        const val = obj.get("schedule") orelse break :blk null;
        const s = switch (val) {
            .string => |str| str,
            else => break :blk null,
        };
        break :blk try alloc.dupe(u8, s);
    };

    return ZombieTrigger{
        .trigger_type = trigger_type,
        .source = source,
        .event = event,
        .schedule = schedule,
    };
}

fn parseZombieNetwork(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || ZombieConfigError)!ZombieNetwork {
    const allow_val = obj.get("allow") orelse return ZombieNetwork{ .allow = &.{} };
    const allow_arr = switch (allow_val) {
        .array => |a| a,
        else => return ZombieConfigError.MissingRequiredField,
    };
    return ZombieNetwork{ .allow = try dupeStringArray(alloc, allow_arr.items) };
}

fn parseZombieBudget(obj: std.json.ObjectMap) ZombieConfigError!ZombieBudget {
    const daily_val = obj.get("daily_dollars") orelse return ZombieConfigError.MissingRequiredField;
    const daily = switch (daily_val) {
        .float => |f| f,
        .integer => |i| @as(f64, @floatFromInt(i)),
        else => return ZombieConfigError.InvalidBudget,
    };
    if (daily <= 0.0 or daily > 1000.0) return ZombieConfigError.InvalidBudget;

    const monthly: ?f64 = blk: {
        const val = obj.get("monthly_dollars") orelse break :blk null;
        const f: f64 = switch (val) {
            .float => |fv| fv,
            .integer => |i| @as(f64, @floatFromInt(i)),
            else => return ZombieConfigError.InvalidBudget,
        };
        if (f <= 0.0 or f > 10000.0) return ZombieConfigError.InvalidBudget;
        break :blk f;
    };

    return ZombieBudget{ .daily_dollars = daily, .monthly_dollars = monthly };
}

fn dupeStringArray(alloc: Allocator, items: []const std.json.Value) ![]const []const u8 {
    const out = try alloc.alloc([]const u8, items.len);
    var i: usize = 0;
    errdefer {
        for (out[0..i]) |s| alloc.free(s);
        alloc.free(out);
    }
    for (items) |item| {
        const s = switch (item) {
            .string => |str| str,
            else => return ZombieConfigError.MissingRequiredField,
        };
        out[i] = try alloc.dupe(u8, s);
        i += 1;
    }
    return out;
}

fn freeStringSlice(alloc: Allocator, slice: []const []const u8) void {
    for (slice) |s| alloc.free(s);
    alloc.free(slice);
}

fn freeZombieTrigger(alloc: Allocator, t: ZombieTrigger) void {
    if (t.source) |s| alloc.free(s);
    if (t.event) |e| alloc.free(e);
    if (t.schedule) |s| alloc.free(s);
}

fn isKnownZombieSkill(skill: []const u8) bool {
    if (std.mem.startsWith(u8, skill, "clawhub://")) return isPinnedZombieSkillRef(skill);
    for (KNOWN_ZOMBIE_SKILLS) |known| {
        if (std.mem.eql(u8, skill, known)) return true;
    }
    return false;
}

fn isPinnedZombieSkillRef(ref: []const u8) bool {
    const at = std.mem.lastIndexOfScalar(u8, ref, '@') orelse return false;
    if (at + 1 >= ref.len) return false;
    const version = ref[at + 1 ..];
    if (std.ascii.eqlIgnoreCase(version, "latest")) return false;
    return std.ascii.isDigit(version[0]);
}

test "parseZombieConfig: valid config parses all fields" {
    const alloc = std.testing.allocator;
    const json =
        \\{
        \\  "name": "lead-collector",
        \\  "trigger": {"type": "webhook", "source": "agentmail", "event": "message.received"},
        \\  "skills": ["agentmail"],
        \\  "credentials": ["op://ZMB_LOCAL_DEV/agentmail/api_key"],
        \\  "network": {"allow": ["api.agentmail.to"]},
        \\  "budget": {"daily_dollars": 5.0}
        \\}
    ;
    var cfg = try parseZombieConfig(alloc, json);
    defer cfg.deinit(alloc);
    try std.testing.expectEqualStrings("lead-collector", cfg.name);
    try std.testing.expectEqual(ZombieTriggerType.webhook, cfg.trigger.trigger_type);
    try std.testing.expectEqualStrings("agentmail", cfg.trigger.source.?);
    try std.testing.expectEqual(@as(usize, 1), cfg.skills.len);
    try std.testing.expectEqualStrings("agentmail", cfg.skills[0]);
    try std.testing.expectApproxEqAbs(@as(f64, 5.0), cfg.budget.daily_dollars, 0.001);
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
