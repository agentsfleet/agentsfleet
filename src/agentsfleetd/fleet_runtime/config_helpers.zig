// Fleet config sub-parsers: trigger, network, budget, skill validation.
//
// Extracted from config.zig to keep files under 400 lines.
// These are pure parse functions operating on std.json.ObjectMap.

const std = @import("std");
const Allocator = std.mem.Allocator;
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");

const config_types = @import("config_types.zig");
const webhook_verify = @import("webhook_verify.zig");
// Upper bounds on a fleet's declared spend ceiling, in dollars (RULE UFS — a
// literal's name carries its meaning; these bound money, never milliseconds).
const MAX_DAILY_BUDGET_UNITS = 1000.0;
const MAX_BUDGET_UNITS = 10000.0;

const log = logging.scoped(.fleet_config);
const FleetTrigger = config_types.FleetTrigger;
const FleetNetwork = config_types.FleetNetwork;
const FleetBudget = config_types.FleetBudget;
const FleetConfigError = config_types.FleetConfigError;
const WebhookSignatureConfig = config_types.WebhookSignatureConfig;
const MAX_SIGNATURE_HEADER_LEN = config_types.MAX_SIGNATURE_HEADER_LEN;

// Trigger array + event allow-list bounds. Local to the parser so the
// type module stays free of validation knobs; tests reach in via the
// parser's behaviour, not the constants. RULE NSQ: every limit is a
// named constant — no inline `8`/`16`/`64` at validation sites.
pub const MAX_TRIGGERS_PER_AGENT: usize = 8;
const MAX_EVENTS_PER_TRIGGER: usize = 16;
const MAX_EVENT_NAME_LEN: usize = 64;
const MAX_REPOSITORIES_PER_TRIGGER: usize = 64;
const MAX_REPOSITORY_NAME_LEN: usize = 255;
const LOG_TRIGGER_ALLOW_LIST_INVALID = "trigger_allow_list_invalid";

/// Parse a `triggers[]` array element.
pub fn parseFleetTrigger(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || FleetConfigError)!FleetTrigger {
    const type_str = blk: {
        const val = obj.get("type") orelse return FleetConfigError.MissingRequiredField;
        break :blk switch (val) {
            .string => |s| s,
            else => return FleetConfigError.MissingRequiredField,
        };
    };

    if (std.mem.eql(u8, type_str, "webhook")) {
        const source = try requireString(alloc, obj, "source") orelse return FleetConfigError.InvalidTriggerSource;
        errdefer alloc.free(source);
        const events = try parseEvents(alloc, obj);
        errdefer if (events) |evs| {
            for (evs) |e| alloc.free(e);
            alloc.free(evs);
        };
        const repositories = try parseRepositories(alloc, obj);
        errdefer if (repositories) |repos| config_types.freeStringSlice(alloc, repos);
        const credential_name = try optionalString(alloc, obj, "credential_name");
        errdefer if (credential_name) |c| alloc.free(c);
        const signature = try parseWebhookSignature(alloc, obj, source);
        return .{ .webhook = .{
            .source = source,
            .events = events,
            .repositories = repositories,
            .credential_name = credential_name,
            .signature = signature,
        } };
    }
    if (std.mem.eql(u8, type_str, "cron")) {
        const schedule = try requireString(alloc, obj, "schedule") orelse return FleetConfigError.MissingRequiredField;
        errdefer alloc.free(schedule);
        const timezone = if (try optionalString(alloc, obj, "timezone")) |value|
            value
        else
            try alloc.dupe(u8, config_types.DEFAULT_CRON_TIMEZONE);
        errdefer alloc.free(timezone);
        const message = if (try optionalString(alloc, obj, "message")) |value|
            value
        else
            try alloc.dupe(u8, config_types.DEFAULT_CRON_MESSAGE);
        return .{ .cron = .{ .schedule = schedule, .timezone = timezone, .message = message } };
    }
    if (std.mem.eql(u8, type_str, "api")) {
        return .{ .api = {} };
    }
    return FleetConfigError.InvalidTriggerType;
}

/// Parse the `triggers:` array under `x-agentsfleet:`. Enforces:
///   * length in 1..MAX_TRIGGERS_PER_AGENT
///   * at most one cron entry
///   * unique `(type, source)` tuple across webhook entries
/// On error every successfully-parsed trigger is freed before propagating.
pub fn parseFleetTriggers(
    alloc: Allocator,
    items: []const std.json.Value,
) (Allocator.Error || FleetConfigError)![]const FleetTrigger {
    if (items.len == 0 or items.len > MAX_TRIGGERS_PER_AGENT) {
        log.warn("triggers_count_out_of_bounds", .{ .error_code = ec.ERR_AGENTSFLEET_INVALID_CONFIG, .count = items.len, .max = MAX_TRIGGERS_PER_AGENT });
        return FleetConfigError.InvalidFieldType;
    }
    var out = try alloc.alloc(FleetTrigger, items.len);
    var parsed_count: usize = 0;
    errdefer {
        for (out[0..parsed_count]) |t| config_types.freeFleetTrigger(alloc, t);
        alloc.free(out);
    }
    var cron_count: usize = 0;
    for (items, 0..) |item, idx| {
        const obj = switch (item) {
            .object => |o| o,
            else => return FleetConfigError.InvalidFieldType,
        };
        const trig = try parseFleetTrigger(alloc, obj);
        out[idx] = trig;
        parsed_count += 1;
        if (trig == .cron) {
            cron_count += 1;
            if (cron_count > 1) {
                log.warn("multiple_cron_triggers_rejected", .{
                    .error_code = ec.ERR_AGENTSFLEET_INVALID_CONFIG,
                });
                return FleetConfigError.InvalidTriggerType;
            }
        }
        for (out[0..idx]) |existing| {
            if (std.meta.activeTag(existing) != std.meta.activeTag(trig)) continue;
            const conflict = switch (trig) {
                .webhook => |w| std.mem.eql(u8, existing.webhook.source, w.source),
                .cron => false, // ≤1-cron rule handles this above
                .api => true,
            };
            if (conflict) {
                log.warn("duplicate_trigger_tuple", .{ .error_code = ec.ERR_AGENTSFLEET_INVALID_CONFIG, .index = idx });
                return FleetConfigError.InvalidTriggerType;
            }
        }
    }
    return out;
}

fn parseEvents(
    alloc: Allocator,
    obj: std.json.ObjectMap,
) (Allocator.Error || FleetConfigError)!?[]const []const u8 {
    return parseBoundedStrings(alloc, obj, "events", MAX_EVENTS_PER_TRIGGER, MAX_EVENT_NAME_LEN, noWhitespace);
}

fn parseRepositories(
    alloc: Allocator,
    obj: std.json.ObjectMap,
) (Allocator.Error || FleetConfigError)!?[]const []const u8 {
    return parseBoundedStrings(alloc, obj, "repositories", MAX_REPOSITORIES_PER_TRIGGER, MAX_REPOSITORY_NAME_LEN, validRepository);
}

fn parseBoundedStrings(
    alloc: Allocator,
    obj: std.json.ObjectMap,
    field: []const u8,
    max_count: usize,
    max_len: usize,
    valid: *const fn ([]const u8) bool,
) (Allocator.Error || FleetConfigError)!?[]const []const u8 {
    const val = obj.get(field) orelse return null;
    const arr = switch (val) {
        .array => |a| a,
        else => {
            log.warn(LOG_TRIGGER_ALLOW_LIST_INVALID, .{ .error_code = ec.ERR_AGENTSFLEET_INVALID_CONFIG, .field = field, .reason = "not_array" });
            return FleetConfigError.InvalidFieldType;
        },
    };
    if (arr.items.len == 0 or arr.items.len > max_count) {
        log.warn(LOG_TRIGGER_ALLOW_LIST_INVALID, .{ .error_code = ec.ERR_AGENTSFLEET_INVALID_CONFIG, .field = field, .reason = "count", .count = arr.items.len, .max = max_count });
        return FleetConfigError.InvalidFieldType;
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
                log.warn(LOG_TRIGGER_ALLOW_LIST_INVALID, .{ .error_code = ec.ERR_AGENTSFLEET_INVALID_CONFIG, .field = field, .reason = "not_string" });
                return FleetConfigError.InvalidFieldType;
            },
        };
        if (s.len == 0 or s.len > max_len or !valid(s)) {
            log.warn(LOG_TRIGGER_ALLOW_LIST_INVALID, .{ .error_code = ec.ERR_AGENTSFLEET_INVALID_CONFIG, .field = field, .reason = "entry", .len = s.len, .max = max_len });
            return FleetConfigError.InvalidFieldType;
        }
        out[i] = try alloc.dupe(u8, s);
        i += 1;
    }
    return out;
}

fn noWhitespace(s: []const u8) bool {
    for (s) |c| if (std.ascii.isWhitespace(c)) return false;
    return true;
}

fn validRepository(s: []const u8) bool {
    if (!noWhitespace(s)) return false;
    const slash = std.mem.indexOfScalar(u8, s, '/') orelse return false;
    return slash > 0 and slash + 1 < s.len and std.mem.indexOfScalarPos(u8, s, slash + 1, '/') == null;
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
        return FleetConfigError.InvalidSignatureConfig;
    errdefer alloc.free(secret_ref);
    if (secret_ref.len == 0) return FleetConfigError.InvalidSignatureConfig;

    const registry_hit = webhook_verify.detectProvider(source, webhook_verify.NoHeaders{});

    const header = header_blk: {
        if (try optionalString(alloc, sig_obj, "header")) |h| break :header_blk h;
        if (registry_hit) |cfg| break :header_blk try alloc.dupe(u8, cfg.sig_header);
        return FleetConfigError.InvalidSignatureConfig;
    };
    errdefer alloc.free(header);
    if (header.len > MAX_SIGNATURE_HEADER_LEN) return FleetConfigError.InvalidSignatureConfig;

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

pub fn parseFleetNetwork(alloc: Allocator, obj: std.json.ObjectMap) (Allocator.Error || FleetConfigError)!FleetNetwork {
    const allow_val = obj.get("allow") orelse return FleetNetwork{ .allow = &.{} };
    const allow_arr = switch (allow_val) {
        .array => |a| a,
        else => return FleetConfigError.MissingRequiredField,
    };
    return FleetNetwork{ .allow = try dupeStringArray(alloc, allow_arr.items) };
}

pub fn parseFleetBudget(obj: std.json.ObjectMap) FleetConfigError!FleetBudget {
    const daily_val = obj.get("daily_dollars") orelse return FleetConfigError.MissingRequiredField;
    const daily = switch (daily_val) {
        .float => |f| f,
        .integer => |i| @as(f64, @floatFromInt(i)),
        else => return FleetConfigError.InvalidBudget,
    };
    if (daily <= 0.0 or daily > MAX_DAILY_BUDGET_UNITS) return FleetConfigError.InvalidBudget;

    const monthly: ?f64 = blk: {
        const val = obj.get("monthly_dollars") orelse break :blk null;
        const f: f64 = switch (val) {
            .float => |fv| fv,
            .integer => |i| @as(f64, @floatFromInt(i)),
            else => return FleetConfigError.InvalidBudget,
        };
        if (f <= 0.0 or f > MAX_BUDGET_UNITS) return FleetConfigError.InvalidBudget;
        break :blk f;
    };

    return FleetBudget{ .daily_dollars = daily, .monthly_dollars = monthly };
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
            else => return FleetConfigError.MissingRequiredField,
        };
        out[i] = try alloc.dupe(u8, s);
        i += 1;
    }
    return out;
}

// Tests live in `config_helpers_test.zig` to keep this implementation
// file under the 350-line cap and so the validation matrix
// (parseWebhookSignature + parseEvents + parseFleetTriggers) lives in
// one place.
