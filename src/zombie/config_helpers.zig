// Zombie config sub-parsers: trigger, network, budget, skill validation.
//
// Extracted from config.zig to keep files under 400 lines.
// These are pure parse functions operating on std.json.ObjectMap.

const std = @import("std");
const Allocator = std.mem.Allocator;

const config = @import("config.zig");
const ZombieTriggerType = config.ZombieTriggerType;
const ZombieTrigger = config.ZombieTrigger;
const ZombieNetwork = config.ZombieNetwork;
const ZombieBudget = config.ZombieBudget;
const ZombieConfigError = config.ZombieConfigError;

// Built-in skills. clawhub:// registry refs are also accepted (must be pinned).
const KNOWN_ZOMBIE_SKILLS = [_][]const u8{
    "agentmail", "slack",      "github",    "git",
    "linear",    "cloudflare", "pagerduty",
};

pub fn parseZombieTrigger(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || ZombieConfigError)!ZombieTrigger {
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
    else if (std.mem.eql(u8, type_str, "chain"))
        ZombieTriggerType.chain
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

pub fn parseZombieNetwork(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || ZombieConfigError)!ZombieNetwork {
    const allow_val = obj.get("allow") orelse return ZombieNetwork{ .allow = &.{} };
    const allow_arr = switch (allow_val) {
        .array => |a| a,
        else => return ZombieConfigError.MissingRequiredField,
    };
    return ZombieNetwork{ .allow = try dupeStringArray(alloc, allow_arr.items) };
}

pub fn parseZombieBudget(obj: std.json.ObjectMap) ZombieConfigError!ZombieBudget {
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

pub fn dupeStringArray(alloc: Allocator, items: []const std.json.Value) ![]const []const u8 {
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

pub fn freeStringSlice(alloc: Allocator, slice: []const []const u8) void {
    for (slice) |s| alloc.free(s);
    alloc.free(slice);
}

pub fn freeZombieTrigger(alloc: Allocator, t: ZombieTrigger) void {
    if (t.source) |s| alloc.free(s);
    if (t.event) |e| alloc.free(e);
    if (t.schedule) |s| alloc.free(s);
}

pub fn isKnownZombieSkill(skill: []const u8) bool {
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

test "isKnownZombieSkill: built-in and clawhub refs" {
    try std.testing.expect(isKnownZombieSkill("agentmail"));
    try std.testing.expect(isKnownZombieSkill("slack"));
    try std.testing.expect(!isKnownZombieSkill("unknown_tool"));
    try std.testing.expect(isKnownZombieSkill("clawhub://queen/lead-hunter@1.0.1"));
    try std.testing.expect(!isKnownZombieSkill("clawhub://queen/lead-hunter@latest"));
    try std.testing.expect(!isKnownZombieSkill("clawhub://queen/lead-hunter"));
}
