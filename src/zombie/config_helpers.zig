// Zombie config sub-parsers: trigger, network, budget, skill validation.
//
// Extracted from config.zig to keep files under 400 lines.
// These are pure parse functions operating on std.json.ObjectMap.

const std = @import("std");
const Allocator = std.mem.Allocator;
const logging = @import("log");

const config_types = @import("config_types.zig");
const webhook_verify = @import("webhook_verify.zig");

const log = logging.scoped(.zombie_config);
const ZombieTrigger = config_types.ZombieTrigger;
const ZombieNetwork = config_types.ZombieNetwork;
const ZombieBudget = config_types.ZombieBudget;
const ZombieConfigError = config_types.ZombieConfigError;
const WebhookSignatureConfig = config_types.WebhookSignatureConfig;
const MAX_SIGNATURE_HEADER_LEN = config_types.MAX_SIGNATURE_HEADER_LEN;

// Trigger array + event allow-list bounds. Local to the parser so the
// type module stays free of validation knobs; tests reach in via the
// parser's behaviour, not the constants. RULE NSQ: every limit is a
// named constant — no inline `8`/`16`/`64` at validation sites.
const MAX_EVENTS_PER_TRIGGER: usize = 16;
const MAX_EVENT_NAME_LEN: usize = 64;

/// Parse a `triggers[]` array element. The api variant is admitted by
/// the type union for layout stability but rejected here — re-admission
/// lands with workspace-API-token surface.
pub fn parseZombieTrigger(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || ZombieConfigError)!ZombieTrigger {
    const type_str = blk: {
        const val = obj.get("type") orelse return ZombieConfigError.MissingRequiredField;
        break :blk switch (val) {
            .string => |s| s,
            else => return ZombieConfigError.MissingRequiredField,
        };
    };

    if (std.mem.eql(u8, type_str, "webhook")) {
        const source = try requireString(alloc, obj, "source") orelse return ZombieConfigError.InvalidTriggerSource;
        errdefer alloc.free(source);
        const events = try parseEvents(alloc, obj);
        errdefer if (events) |evs| {
            for (evs) |e| alloc.free(e);
            alloc.free(evs);
        };
        const credential_name = try optionalString(alloc, obj, "credential_name");
        errdefer if (credential_name) |c| alloc.free(c);
        const signature = try parseWebhookSignature(alloc, obj, source);
        return .{ .webhook = .{
            .source = source,
            .events = events,
            .credential_name = credential_name,
            .signature = signature,
        } };
    }
    if (std.mem.eql(u8, type_str, "cron")) {
        const schedule = try requireString(alloc, obj, "schedule") orelse return ZombieConfigError.MissingRequiredField;
        return .{ .cron = .{ .schedule = schedule } };
    }
    if (std.mem.eql(u8, type_str, "api")) {
        log.warn("trigger_type_api_not_yet_available", .{
            .hint = "type: api is not yet available; use webhook or cron",
        });
        return ZombieConfigError.InvalidTriggerType;
    }
    return ZombieConfigError.InvalidTriggerType;
}

/// Parse the `triggers:` array under `x-usezombie:`. Enforces:
///   * length in 1..MAX_TRIGGERS_PER_ZOMBIE
///   * at most one cron entry
///   * unique `(type, source)` tuple across webhook entries
/// On error every successfully-parsed trigger is freed before propagating.
fn parseEvents(
    alloc: Allocator,
    obj: std.json.ObjectMap,
) (Allocator.Error || ZombieConfigError)!?[]const []const u8 {
    const val = obj.get("events") orelse return null;
    const arr = switch (val) {
        .array => |a| a,
        else => {
            log.warn("events_must_be_array", .{});
            return ZombieConfigError.InvalidFieldType;
        },
    };
    if (arr.items.len == 0 or arr.items.len > MAX_EVENTS_PER_TRIGGER) {
        log.warn("events_count_out_of_bounds", .{ .count = arr.items.len, .max = MAX_EVENTS_PER_TRIGGER });
        return ZombieConfigError.InvalidFieldType;
    }
    var out = try alloc.alloc([]const u8, arr.items.len);
    var i: usize = 0;
    errdefer {
        for (out[0..i]) |s| alloc.free(s);
        alloc.free(out);
    }
    for (arr.items) |item| {
        const s = switch (item) {
            .string => |str| str,
            else => {
                log.warn("events_entry_not_string", .{});
                return ZombieConfigError.InvalidFieldType;
            },
        };
        if (s.len == 0 or s.len > MAX_EVENT_NAME_LEN) {
            log.warn("events_entry_length_out_of_bounds", .{ .len = s.len, .max = MAX_EVENT_NAME_LEN });
            return ZombieConfigError.InvalidFieldType;
        }
        for (s) |c| {
            if (std.ascii.isWhitespace(c)) {
                log.warn("events_entry_has_whitespace", .{});
                return ZombieConfigError.InvalidFieldType;
            }
        }
        out[i] = try alloc.dupe(u8, s);
        i += 1;
    }
    return out;
}

fn parseWebhookSignature(
    alloc: Allocator,
    obj: std.json.ObjectMap,
    source: []const u8,
) !?WebhookSignatureConfig {
    const sig_val = obj.get("signature") orelse return null;
    const sig_obj = switch (sig_val) {
        .object => |o| o,
        else => return null,
    };

    const secret_ref = try requireString(alloc, sig_obj, "secret_ref") orelse
        return ZombieConfigError.InvalidSignatureConfig;
    errdefer alloc.free(secret_ref);
    if (secret_ref.len == 0) return ZombieConfigError.InvalidSignatureConfig;

    const registry_hit = webhook_verify.detectProvider(source, webhook_verify.NoHeaders{});

    const header = header_blk: {
        if (try optionalString(alloc, sig_obj, "header")) |h| break :header_blk h;
        if (registry_hit) |cfg| break :header_blk try alloc.dupe(u8, cfg.sig_header);
        return ZombieConfigError.InvalidSignatureConfig;
    };
    errdefer alloc.free(header);
    if (header.len > MAX_SIGNATURE_HEADER_LEN) return ZombieConfigError.InvalidSignatureConfig;

    const prefix = prefix_blk: {
        if (try optionalString(alloc, sig_obj, "prefix")) |p| break :prefix_blk p;
        if (registry_hit) |cfg| break :prefix_blk try alloc.dupe(u8, cfg.prefix);
        break :prefix_blk try alloc.dupe(u8, "");
    };
    errdefer alloc.free(prefix);

    const ts_header = ts_blk: {
        if (try optionalString(alloc, sig_obj, "ts_header")) |t| break :ts_blk t;
        if (registry_hit) |cfg| {
            if (cfg.ts_header) |t| break :ts_blk try alloc.dupe(u8, t);
        }
        break :ts_blk null;
    };

    return .{
        .header = header,
        .prefix = prefix,
        .ts_header = ts_header,
        .secret_ref = secret_ref,
    };
}

fn requireString(alloc: Allocator, obj: std.json.ObjectMap, key: []const u8) !?[]const u8 {
    const val = obj.get(key) orelse return null;
    const s = switch (val) {
        .string => |str| str,
        else => return null,
    };
    return try alloc.dupe(u8, s);
}

fn optionalString(alloc: Allocator, obj: std.json.ObjectMap, key: []const u8) !?[]const u8 {
    const val = obj.get(key) orelse return null;
    const s = switch (val) {
        .string => |str| str,
        else => return null,
    };
    return try alloc.dupe(u8, s);
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

// Tests live in `config_helpers_test.zig` to keep this implementation
// file under the 350-line cap and so the validation matrix
// (parseWebhookSignature + parseEvents + parseZombieTriggers) lives in
// one place.
